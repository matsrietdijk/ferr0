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

pub fn render_entities(entities: &[Value], kind: &str) -> String {
    if entities.is_empty() {
        return format!("No {kind}s found.");
    }
    entities
        .iter()
        .map(render_entity)
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn pretty(response: &Value) -> String {
    serde_json::to_string_pretty(response).unwrap_or_else(|_| response.to_string())
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

fn render_entity(entity: &Value) -> String {
    let text = |key| entity.get(key).and_then(Value::as_str);
    let created = text("created_at").map_or("—", |created| created.get(..10).unwrap_or(created));
    format!("{}  {created}", text("id").unwrap_or("—"))
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
    fn renders_one_line_per_entity_with_its_created_date() {
        let entities = [
            json!({"id": "alice", "type": "user", "created_at": "2026-09-01T10:00:00Z"}),
            json!({"id": "bob", "type": "user", "created_at": null}),
        ];

        assert_eq!(
            render_entities(&entities, "user"),
            "alice  2026-09-01\nbob  —"
        );
    }

    #[test]
    fn renders_no_entities_with_the_type() {
        assert_eq!(render_entities(&[], "agent"), "No agents found.");
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
