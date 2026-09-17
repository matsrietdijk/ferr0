use std::fmt;

use anyhow::{Context, Result};
use reqwest::{
    StatusCode, Url,
    blocking::{Client as Http, RequestBuilder},
};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug)]
pub struct ServerError {
    pub status: StatusCode,
    detail: String,
}

impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "server returned {}", self.status)?;
        if !self.detail.is_empty() {
            write!(f, ": {}", self.detail)?;
        }
        Ok(())
    }
}

impl std::error::Error for ServerError {}

#[derive(Debug, Default, Serialize)]
pub struct Scope {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
}

impl Scope {
    fn pairs(&self) -> impl Iterator<Item = (&'static str, &str)> {
        [
            ("user_id", &self.user_id),
            ("agent_id", &self.agent_id),
            ("run_id", &self.run_id),
        ]
        .into_iter()
        .filter_map(|(key, value)| value.as_deref().map(|value| (key, value)))
    }

    fn is_empty(&self) -> bool {
        self.pairs().next().is_none()
    }
}

#[derive(Serialize)]
struct Message<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct AddRequest<'a> {
    messages: [Message<'a>; 1],
    #[serde(flatten)]
    scope: &'a Scope,
}

#[derive(Serialize)]
struct SearchRequest<'a> {
    query: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    filters: Option<&'a Scope>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_k: Option<u32>,
}

#[derive(Serialize)]
struct UpdateRequest<'a> {
    text: &'a str,
}

pub struct Client {
    http: Http,
    base: Url,
    api_key: Option<String>,
}

impl Client {
    pub fn new(base_url: &str, api_key: Option<String>) -> Result<Self> {
        let base = Url::parse(base_url)
            .ok()
            .filter(|url| !url.cannot_be_a_base())
            .with_context(|| format!("invalid server URL: {base_url}"))?;
        Ok(Self {
            http: Http::new(),
            base,
            api_key,
        })
    }

    pub fn add(&self, text: &str, scope: &Scope) -> Result<Value> {
        let body = AddRequest {
            messages: [Message {
                role: "user",
                content: text,
            }],
            scope,
        };
        self.send(self.http.post(self.endpoint(&["memories"])).json(&body))
    }

    pub fn search(&self, query: &str, scope: &Scope, limit: Option<u32>) -> Result<Value> {
        let body = SearchRequest {
            query,
            filters: (!scope.is_empty()).then_some(scope),
            top_k: limit,
        };
        self.send(self.http.post(self.endpoint(&["search"])).json(&body))
    }

    pub fn list(&self, scope: &Scope, limit: Option<u32>) -> Result<Value> {
        let limit = limit.map(|limit| limit.to_string());
        let pairs: Vec<_> = scope
            .pairs()
            .chain(limit.as_deref().map(|limit| ("top_k", limit)))
            .collect();
        let mut url = self.endpoint(&["memories"]);
        if !pairs.is_empty() {
            url.query_pairs_mut().extend_pairs(pairs);
        }
        self.send(self.http.get(url))
    }

    pub fn get(&self, id: &str) -> Result<Value> {
        self.send(self.http.get(self.endpoint(&["memories", id])))
    }

    pub fn update(&self, id: &str, text: &str) -> Result<Value> {
        let url = self.endpoint(&["memories", id]);
        self.send(self.http.put(url).json(&UpdateRequest { text }))
    }

    pub fn me(&self) -> Result<Value> {
        self.send(self.http.get(self.endpoint(&["auth", "me"])))
    }

    pub fn delete(&self, id: &str) -> Result<Value> {
        self.send(self.http.delete(self.endpoint(&["memories", id])))
    }

    fn endpoint(&self, segments: &[&str]) -> Url {
        let mut url = self.base.clone();
        url.path_segments_mut()
            .expect("base URL is validated in Client::new")
            .pop_if_empty()
            .extend(segments);
        url
    }

    fn send(&self, request: RequestBuilder) -> Result<Value> {
        let request = match &self.api_key {
            Some(key) => request.header("X-API-Key", key),
            None => request,
        };
        let response = request.send().context("cannot reach the Mem0 server")?;
        let status = response.status();
        let text = response
            .text()
            .context("cannot read the Mem0 server response")?;
        let body = serde_json::from_str(&text).unwrap_or(Value::String(text));
        if !status.is_success() {
            let detail = body.get("detail").unwrap_or(&body);
            let detail = detail
                .as_str()
                .map_or_else(|| detail.to_string(), str::to_string);
            return Err(ServerError { status, detail }.into());
        }
        Ok(body)
    }
}

#[cfg(test)]
mod tests {
    use mockito::{Matcher, Server};
    use serde_json::json;

    use super::*;

    fn user(id: &str) -> Scope {
        Scope {
            user_id: Some(id.into()),
            ..Scope::default()
        }
    }

    #[test]
    fn add_posts_a_user_message_with_scope_and_api_key() {
        let mut server = Server::new();
        let mock = server
            .mock("POST", "/memories")
            .match_header("x-api-key", "secret")
            .match_body(Matcher::Json(json!({
                "messages": [{"role": "user", "content": "likes tea"}],
                "user_id": "alice",
            })))
            .with_body(r#"{"results": []}"#)
            .create();

        let client = Client::new(&server.url(), Some("secret".into())).unwrap();
        let response = client.add("likes tea", &user("alice")).unwrap();

        mock.assert();
        assert_eq!(response, json!({"results": []}));
    }

    #[test]
    fn search_sends_scope_as_filters() {
        let mut server = Server::new();
        let mock = server
            .mock("POST", "/search")
            .match_header("x-api-key", Matcher::Missing)
            .match_body(Matcher::Json(json!({
                "query": "drinks",
                "filters": {"user_id": "alice"},
                "top_k": 3,
            })))
            .with_body("{}")
            .create();

        let client = Client::new(&server.url(), None).unwrap();
        client.search("drinks", &user("alice"), Some(3)).unwrap();

        mock.assert();
    }

    #[test]
    fn search_without_scope_omits_filters() {
        let mut server = Server::new();
        let mock = server
            .mock("POST", "/search")
            .match_body(Matcher::Json(json!({"query": "drinks"})))
            .with_body("{}")
            .create();

        let client = Client::new(&server.url(), None).unwrap();
        client.search("drinks", &Scope::default(), None).unwrap();

        mock.assert();
    }

    #[test]
    fn list_sends_scope_and_limit_as_query() {
        let mut server = Server::new();
        let mock = server
            .mock("GET", "/memories")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("user_id".into(), "alice".into()),
                Matcher::UrlEncoded("top_k".into(), "5".into()),
            ]))
            .with_body("{}")
            .create();

        let client = Client::new(&server.url(), None).unwrap();
        client.list(&user("alice"), Some(5)).unwrap();

        mock.assert();
    }

    #[test]
    fn get_fetches_the_memory_by_encoded_id() {
        let mut server = Server::new();
        let mock = server
            .mock("GET", "/memories/a%2Fb")
            .match_header("x-api-key", "secret")
            .with_body(r#"{"id": "a/b", "memory": "likes tea"}"#)
            .create();

        let client = Client::new(&server.url(), Some("secret".into())).unwrap();
        let response = client.get("a/b").unwrap();

        mock.assert();
        assert_eq!(response, json!({"id": "a/b", "memory": "likes tea"}));
    }

    #[test]
    fn update_and_delete_target_the_memory_under_a_base_path() {
        let mut server = Server::new();
        let update = server
            .mock("PUT", "/mem0/memories/a%2Fb")
            .match_body(Matcher::Json(json!({"text": "likes coffee"})))
            .with_body("{}")
            .create();
        let delete = server
            .mock("DELETE", "/mem0/memories/a%2Fb")
            .with_body("{}")
            .create();

        let client = Client::new(&format!("{}/mem0/", server.url()), None).unwrap();
        client.update("a/b", "likes coffee").unwrap();
        client.delete("a/b").unwrap();

        update.assert();
        delete.assert();
    }

    #[test]
    fn error_status_reports_server_detail() {
        let mut server = Server::new();
        server
            .mock("GET", "/memories")
            .with_status(401)
            .with_body(r#"{"detail": "Invalid API key"}"#)
            .create();

        let client = Client::new(&server.url(), None).unwrap();
        let error = client.list(&Scope::default(), None).unwrap_err();

        assert_eq!(
            error.to_string(),
            "server returned 401 Unauthorized: Invalid API key"
        );
    }

    #[test]
    fn rejects_url_that_cannot_be_a_base() {
        assert!(Client::new("mailto:someone@example.com", None).is_err());
    }
}
