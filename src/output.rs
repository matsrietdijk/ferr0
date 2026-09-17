use serde_json::Value;

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
    if let Some(message) = response.get("message").and_then(Value::as_str) {
        return message.to_string();
    }
    pretty(response)
}

pub fn pretty(response: &Value) -> String {
    serde_json::to_string_pretty(response).unwrap_or_else(|_| response.to_string())
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
    use serde_json::json;

    use super::*;

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
    fn renders_unknown_shapes_as_json() {
        assert_eq!(render(&json!({"id": "1"}), ""), "{\n  \"id\": \"1\"\n}");
    }
}
