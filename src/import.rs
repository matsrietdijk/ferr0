use std::{fs, path::Path};

use anyhow::{Context, Result};
use serde_json::{Value, json};

use crate::{
    client::{AddOptions, Client, Message, Scope},
    input, output,
};

pub fn from_file(client: &Client, scope: &Scope, path: &Path) -> Result<Value> {
    let json = fs::read_to_string(path)
        .with_context(|| format!("cannot read memories from {}", path.display()))?;
    let items = match serde_json::from_str(&json).context("the import file must contain JSON")? {
        Value::Array(items) => items,
        item => vec![item],
    };
    Ok(run(client, scope, &items))
}

fn run(client: &Client, resolved: &Scope, items: &[Value]) -> Value {
    let results: Vec<Value> = items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let mut result = match add(client, resolved, item) {
                Ok(response) => json!({"response": response}),
                Err(error) => output::error(&error),
            };
            result["item"] = (index + 1).into();
            result
        })
        .collect();
    let failed = results
        .iter()
        .filter(|result| result.get("error").is_some())
        .count();
    json!({
        "added": results.len() - failed,
        "failed": failed,
        "results": results,
    })
}

fn add(client: &Client, resolved: &Scope, item: &Value) -> Result<Value> {
    let text = match item {
        Value::String(text) => Some(text.as_str()),
        item => ["memory", "text", "content"]
            .into_iter()
            .find_map(|key| item.get(key))
            .and_then(Value::as_str),
    }
    .filter(|text| !text.is_empty())
    .context("no memory, text or content string")?;
    let id = |resolved: &Option<String>, key| {
        let present = |id: &&str| !id.is_empty();
        resolved
            .as_deref()
            .filter(present)
            .or_else(|| item.get(key).and_then(Value::as_str).filter(present))
            .map(str::to_string)
    };
    let scope = Scope {
        user_id: id(&resolved.user_id, "user_id"),
        agent_id: id(&resolved.agent_id, "agent_id"),
        run_id: None,
    };
    let options = AddOptions {
        metadata: item.get("metadata").cloned().and_then(input::non_empty),
        ..AddOptions::default()
    };
    client.add(&[Message::user(text.to_string())], &scope, &options)
}

pub fn render(summary: &Value) -> String {
    let mut lines = vec![format!("Imported {} memories.", summary["added"])];
    if summary["failed"].as_u64().is_some_and(|failed| failed > 0) {
        lines.push(format!("{} memories failed to import.", summary["failed"]));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use mockito::{Matcher, Server};

    use super::*;

    fn post(server: &mut Server, body: Value) -> mockito::Mock {
        server
            .mock("POST", "/memories")
            .match_body(Matcher::Json(body))
            .with_body(r#"{"results": []}"#)
            .create()
    }

    #[test]
    fn adds_strings_and_objects_with_their_text_key_and_metadata() {
        let mut server = Server::new();
        let mocks = [
            post(
                &mut server,
                json!({"messages": [{"role": "user", "content": "likes tea"}]}),
            ),
            post(
                &mut server,
                json!({"messages": [{"role": "user", "content": "uses pnpm"}], "metadata": {"topic": "tools"}}),
            ),
            post(
                &mut server,
                json!({"messages": [{"role": "user", "content": "lives in Utrecht"}]}),
            ),
            post(
                &mut server,
                json!({"messages": [{"role": "user", "content": "works remotely"}]}),
            ),
        ];

        let items = json!([
            "likes tea",
            {"memory": "uses pnpm", "metadata": {"topic": "tools"}},
            {"text": "lives in Utrecht", "metadata": {}},
            {"content": "works remotely", "metadata": null},
        ]);
        let client = Client::new(&server.url(), None).unwrap();
        let summary = run(&client, &Scope::default(), items.as_array().unwrap());

        for mock in mocks {
            mock.assert();
        }
        assert_eq!(
            (summary["added"].clone(), summary["failed"].clone()),
            (json!(4), json!(0))
        );
    }

    #[test]
    fn resolved_ids_take_precedence_and_item_ids_fill_gaps() {
        let mut server = Server::new();
        let mock = post(
            &mut server,
            json!({
                "messages": [{"role": "user", "content": "likes tea"}],
                "user_id": "alice",
                "agent_id": "codex",
            }),
        );

        let resolved = Scope {
            user_id: Some("alice".into()),
            agent_id: None,
            run_id: Some("r1".into()),
        };
        let items =
            json!([{"memory": "likes tea", "user_id": "bob", "agent_id": "codex", "run_id": "r2"}]);
        let client = Client::new(&server.url(), None).unwrap();
        run(&client, &resolved, items.as_array().unwrap());

        mock.assert();
    }

    #[test]
    fn empty_ids_count_as_unset() {
        let mut server = Server::new();
        let mock = post(
            &mut server,
            json!({
                "messages": [{"role": "user", "content": "likes tea"}],
                "user_id": "bob",
            }),
        );

        let resolved = Scope {
            user_id: Some(String::new()),
            agent_id: Some(String::new()),
            run_id: None,
        };
        let items = json!([{"memory": "likes tea", "user_id": "bob", "agent_id": ""}]);
        let client = Client::new(&server.url(), None).unwrap();
        run(&client, &resolved, items.as_array().unwrap());

        mock.assert();
    }

    #[test]
    fn imports_a_single_object_file_as_one_item() {
        let mut server = Server::new();
        let mock = post(
            &mut server,
            json!({"messages": [{"role": "user", "content": "likes tea"}]}),
        );
        let file = tempfile::NamedTempFile::new().unwrap();
        fs::write(file.path(), r#"{"memory": "likes tea"}"#).unwrap();

        let client = Client::new(&server.url(), None).unwrap();
        let summary = from_file(&client, &Scope::default(), file.path()).unwrap();

        mock.assert();
        assert_eq!(summary["added"], 1);
    }

    #[test]
    fn rejects_unreadable_and_malformed_files() {
        let client = Client::new("http://127.0.0.1:9", None).unwrap();
        let missing = from_file(
            &client,
            &Scope::default(),
            Path::new("/nonexistent/ferr0.json"),
        );
        assert!(
            missing
                .unwrap_err()
                .to_string()
                .starts_with("cannot read memories from")
        );

        let file = tempfile::NamedTempFile::new().unwrap();
        fs::write(file.path(), "[\"likes tea\"").unwrap();
        let malformed = from_file(&client, &Scope::default(), file.path()).unwrap_err();
        assert_eq!(malformed.to_string(), "the import file must contain JSON");
    }

    #[test]
    fn counts_failed_items_and_continues() {
        let mut server = Server::new();
        let failing = server
            .mock("POST", "/memories")
            .match_body(Matcher::PartialJson(
                json!({"messages": [{"content": "fails"}]}),
            ))
            .with_status(500)
            .with_body(r#"{"detail": "boom"}"#)
            .create();
        let added = post(
            &mut server,
            json!({"messages": [{"role": "user", "content": "likes tea"}]}),
        );

        let items = json!(["fails", ["x", null, null, null, null], {"memory": ""}, 1, "likes tea"]);
        let client = Client::new(&server.url(), None).unwrap();
        let summary = run(&client, &Scope::default(), items.as_array().unwrap());

        failing.assert();
        added.assert();
        let missing = json!({"message": "no memory, text or content string"});
        assert_eq!(
            summary,
            json!({
                "added": 1,
                "failed": 4,
                "results": [
                    {"item": 1, "error": {"message": "server returned 500 Internal Server Error: boom", "status": 500}},
                    {"item": 2, "error": missing},
                    {"item": 3, "error": missing},
                    {"item": 4, "error": missing},
                    {"item": 5, "response": {"results": []}},
                ],
            })
        );
        assert_eq!(
            render(&summary),
            "Imported 1 memories.\n4 memories failed to import."
        );
    }

    #[test]
    fn renders_only_the_added_count_without_failures() {
        let summary = json!({"added": 2, "failed": 0, "results": []});
        assert_eq!(render(&summary), "Imported 2 memories.");
    }
}
