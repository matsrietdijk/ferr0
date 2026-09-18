use std::{fmt, time::Duration};

use anyhow::{Context, Result, bail};
use reqwest::{
    StatusCode, Url,
    blocking::{Client as Http, RequestBuilder},
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

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
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

impl Message {
    pub fn user(content: String) -> Self {
        Self {
            role: "user".into(),
            content,
        }
    }
}

#[derive(Serialize)]
struct AddRequest<'a> {
    messages: &'a [Message],
    #[serde(flatten)]
    scope: &'a Scope,
}

pub struct SearchOptions {
    pub limit: Option<u32>,
    pub threshold: f64,
    pub filter: Map<String, Value>,
    pub show_expired: bool,
}

#[derive(Serialize)]
struct SearchRequest<'a> {
    query: &'a str,
    #[serde(skip_serializing_if = "Map::is_empty")]
    filters: Map<String, Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_k: Option<u32>,
    threshold: f64,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    show_expired: bool,
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
        let http = Http::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .context("cannot build the HTTP client")?;
        Ok(Self {
            http,
            base,
            api_key,
        })
    }

    pub fn add(&self, messages: &[Message], scope: &Scope) -> Result<Value> {
        let body = AddRequest { messages, scope };
        self.send(self.http.post(self.endpoint(&["memories"])).json(&body))
    }

    pub fn search(&self, query: &str, scope: &Scope, options: SearchOptions) -> Result<Value> {
        if !(0.0..=1.0).contains(&options.threshold) {
            bail!("--threshold must be between 0.0 and 1.0");
        }
        let mut filters = options.filter;
        if !filters.contains_key("AND") && !filters.contains_key("OR") {
            for (key, value) in scope.pairs() {
                if filters.contains_key(key) {
                    bail!(
                        "--filter cannot set {key} because the scope already sets it; use --{} instead",
                        key.replace('_', "-")
                    );
                }
                filters.insert(key.into(), value.into());
            }
        }
        let body = SearchRequest {
            query,
            filters,
            top_k: options.limit,
            threshold: options.threshold,
            show_expired: options.show_expired,
        };
        self.send(self.http.post(self.endpoint(&["search"])).json(&body))
    }

    pub fn list(&self, scope: &Scope, limit: Option<u32>, show_expired: bool) -> Result<Value> {
        let limit = limit.map(|limit| limit.to_string());
        let pairs: Vec<_> = scope
            .pairs()
            .chain(limit.as_deref().map(|limit| ("top_k", limit)))
            .chain(show_expired.then_some(("show_expired", "true")))
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

    fn options() -> SearchOptions {
        SearchOptions {
            limit: None,
            threshold: 0.3,
            filter: Map::new(),
            show_expired: false,
        }
    }

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
        let response = client
            .add(&[Message::user("likes tea".into())], &user("alice"))
            .unwrap();

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
                "threshold": 0.3,
            })))
            .with_body("{}")
            .create();

        let options = SearchOptions {
            limit: Some(3),
            ..options()
        };
        let client = Client::new(&server.url(), None).unwrap();
        client.search("drinks", &user("alice"), options).unwrap();

        mock.assert();
    }

    #[test]
    fn search_without_scope_omits_filters() {
        let mut server = Server::new();
        let mock = server
            .mock("POST", "/search")
            .match_body(Matcher::Json(json!({"query": "drinks", "threshold": 0.3})))
            .with_body("{}")
            .create();

        let client = Client::new(&server.url(), None).unwrap();
        client
            .search("drinks", &Scope::default(), options())
            .unwrap();

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
        client.list(&user("alice"), Some(5), false).unwrap();

        mock.assert();
    }

    #[test]
    fn search_merges_filter_with_scope_and_sends_threshold() {
        let mut server = Server::new();
        let mock = server
            .mock("POST", "/search")
            .match_body(Matcher::Json(json!({
                "query": "drinks",
                "filters": {"user_id": "alice", "agent_id": "claude-code", "category": "food"},
                "threshold": 0.5,
            })))
            .with_body("{}")
            .create();

        let options = SearchOptions {
            threshold: 0.5,
            filter: json!({"agent_id": "claude-code", "category": "food"})
                .as_object()
                .unwrap()
                .clone(),
            ..options()
        };
        let client = Client::new(&server.url(), None).unwrap();
        client.search("drinks", &user("alice"), options).unwrap();

        mock.assert();
    }

    #[test]
    fn search_rejects_a_filter_that_overrides_the_scope() {
        let options = SearchOptions {
            filter: json!({"user_id": "bob"}).as_object().unwrap().clone(),
            ..options()
        };
        let client = Client::new("http://localhost:1", None).unwrap();
        let error = client
            .search("drinks", &user("alice"), options)
            .unwrap_err();

        assert_eq!(
            error.to_string(),
            "--filter cannot set user_id because the scope already sets it; use --user-id instead"
        );
    }

    #[test]
    fn search_sends_a_logical_filter_in_place_of_the_scope() {
        let filter = json!({"OR": [{"user_id": "alice"}, {"user_id": "bob"}]});
        let mut server = Server::new();
        let mock = server
            .mock("POST", "/search")
            .match_body(Matcher::Json(json!({
                "query": "drinks",
                "filters": filter,
                "threshold": 0.3,
            })))
            .with_body("{}")
            .create();

        let options = SearchOptions {
            filter: filter.as_object().unwrap().clone(),
            ..options()
        };
        let client = Client::new(&server.url(), None).unwrap();
        client.search("drinks", &user("alice"), options).unwrap();

        mock.assert();
    }

    #[test]
    fn search_rejects_a_threshold_outside_zero_to_one() {
        let client = Client::new("http://localhost:1", None).unwrap();
        for threshold in [-0.1, 1.1, f64::NAN, f64::INFINITY] {
            let options = SearchOptions {
                threshold,
                ..options()
            };
            let error = client
                .search("drinks", &Scope::default(), options)
                .unwrap_err();
            assert_eq!(error.to_string(), "--threshold must be between 0.0 and 1.0");
        }
    }

    #[test]
    fn search_and_list_send_show_expired() {
        let mut server = Server::new();
        let search = server
            .mock("POST", "/search")
            .match_body(Matcher::Json(
                json!({"query": "drinks", "threshold": 0.3, "show_expired": true}),
            ))
            .with_body("{}")
            .create();
        let list = server
            .mock("GET", "/memories")
            .match_query(Matcher::UrlEncoded("show_expired".into(), "true".into()))
            .with_body("{}")
            .create();

        let options = SearchOptions {
            show_expired: true,
            ..options()
        };
        let client = Client::new(&server.url(), None).unwrap();
        client.search("drinks", &Scope::default(), options).unwrap();
        client.list(&Scope::default(), None, true).unwrap();

        search.assert();
        list.assert();
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
        let error = client.list(&Scope::default(), None, false).unwrap_err();

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
