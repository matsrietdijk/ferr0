use std::time::Duration;

use reqwest::StatusCode;
use serde_json::{Value, json};

use crate::client::ServerError;

pub fn render(response: &Value, empty: &str) -> String {
    if let Some(results) = response.get("results").and_then(Value::as_array) {
        if results.is_empty() {
            return empty.to_string();
        }
        return results
            .iter()
            .map(render_memory)
            .collect::<Vec<_>>()
            .join("\n");
    }
    if response.get("memory").is_some_and(Value::is_string) {
        return render_memory(response);
    }
    if let Some(message) = response.get("message").and_then(Value::as_str) {
        return message.to_string();
    }
    pretty(response)
}

pub fn pretty(response: &Value) -> String {
    serde_json::to_string_pretty(response).unwrap_or_else(|_| response.to_string())
}

const BACKEND: &str = "self-hosted";

pub fn status(url: &str, error: Option<&anyhow::Error>) -> Value {
    json!({
        "connected": error.is_none(),
        "backend": BACKEND,
        "base_url": if error.is_none() { url } else { "" },
    })
}

pub fn render_status(url: &str, error: Option<&anyhow::Error>, latency: Duration) -> String {
    let mut lines = Vec::new();
    match error {
        None => {
            lines.push("● Connected".to_string());
            lines.push(format!("Backend:  {BACKEND}"));
            lines.push(format!("API URL:  {url}"));
        }
        Some(error) => {
            lines.push("● Disconnected".to_string());
            lines.push(format!("Backend:  {BACKEND}"));
            lines.push(format!("Error:    {error:#}"));
            if error
                .downcast_ref::<ServerError>()
                .is_some_and(|error| error.status == StatusCode::UNAUTHORIZED)
            {
                lines.push("Run `ferr0 setup` to reconfigure your API key".to_string());
            }
        }
    }
    lines.push(format!("Latency:  {:.2}s", latency.as_secs_f64()));
    lines.join("\n")
}

pub fn error(error: &anyhow::Error) -> Value {
    let mut body = json!({"message": format!("{error:#}")});
    if let Some(server) = error.downcast_ref::<ServerError>() {
        body["status"] = server.status.as_u16().into();
    }
    json!({"error": body})
}

fn render_memory(memory: &Value) -> String {
    let text = |key| memory.get(key).and_then(Value::as_str);
    let mut parts = Vec::new();
    if let Some(event) = text("event") {
        parts.push(event.to_string());
    }
    if let Some(score) = memory.get("score").and_then(Value::as_f64) {
        parts.push(format!("{score:.3}"));
    }
    parts.push(text("id").unwrap_or("-").to_string());
    parts.push(text("memory").unwrap_or_default().to_string());
    parts.join("  ")
}

#[cfg(test)]
mod tests {
    use anyhow::anyhow;
    use mockito::Server;

    use super::*;
    use crate::client::{Client, Scope};

    #[test]
    fn renders_one_line_per_result() {
        let response = json!({"results": [
            {"id": "1", "memory": "likes tea", "event": "ADD"},
            {"id": "2", "memory": "lives in Utrecht", "score": 0.81234},
        ]});

        assert_eq!(
            render(&response, "none"),
            "ADD  1  likes tea\n0.812  2  lives in Utrecht"
        );
    }

    #[test]
    fn renders_empty_results_with_fallback() {
        assert_eq!(render(&json!({"results": []}), "none"), "none");
    }

    #[test]
    fn renders_server_message() {
        let response = json!({"message": "Memory deleted successfully"});
        assert_eq!(render(&response, ""), "Memory deleted successfully");
    }

    #[test]
    fn renders_single_memory_as_one_line() {
        let response = json!({"id": "1", "memory": "likes tea", "hash": "abc"});
        assert_eq!(render(&response, ""), "1  likes tea");
    }

    #[test]
    fn renders_unknown_shapes_as_json() {
        assert_eq!(render(&json!({"id": "1"}), ""), "{\n  \"id\": \"1\"\n}");
    }

    #[test]
    fn status_reports_connected_server() {
        let latency = Duration::from_millis(120);
        assert_eq!(
            status("http://mem0", None),
            json!({"connected": true, "backend": "self-hosted", "base_url": "http://mem0"})
        );
        assert_eq!(
            render_status("http://mem0", None, latency),
            "● Connected\nBackend:  self-hosted\nAPI URL:  http://mem0\nLatency:  0.12s"
        );
    }

    #[test]
    fn status_reports_rejected_api_key() {
        let mut server = Server::new();
        server.mock("GET", "/auth/me").with_status(401).create();
        let client = Client::new(&server.url(), None).unwrap();
        let error = client.me().unwrap_err();

        assert_eq!(
            status(&server.url(), Some(&error)),
            json!({"connected": false, "backend": "self-hosted", "base_url": ""})
        );
        assert_eq!(
            render_status(&server.url(), Some(&error), Duration::ZERO),
            "● Disconnected\nBackend:  self-hosted\nError:    server returned 401 Unauthorized\n\
             Run `ferr0 setup` to reconfigure your API key\nLatency:  0.00s"
        );
    }

    #[test]
    fn status_reports_unreachable_server_without_hint() {
        let error = anyhow!("connection refused").context("cannot reach the Mem0 server");

        let text = render_status("http://mem0", Some(&error), Duration::ZERO);

        assert!(text.contains("Error:    cannot reach the Mem0 server: connection refused"));
        assert!(!text.contains("ferr0 setup"), "{text}");
    }

    #[test]
    fn error_includes_server_status_and_detail() {
        let mut server = Server::new();
        server
            .mock("GET", "/memories")
            .with_status(401)
            .with_body(r#"{"detail": "Invalid API key"}"#)
            .create();

        let client = Client::new(&server.url(), None).unwrap();
        let error = client.list(&Scope::default(), None).unwrap_err();

        assert_eq!(
            super::error(&error),
            json!({"error": {
                "message": "server returned 401 Unauthorized: Invalid API key",
                "status": 401,
            }})
        );
    }

    #[test]
    fn error_without_server_status_keeps_the_cause_chain() {
        let error = anyhow!("connection refused").context("cannot reach the Mem0 server");

        assert_eq!(
            super::error(&error),
            json!({"error": {"message": "cannot reach the Mem0 server: connection refused"}})
        );
    }
}
