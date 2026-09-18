use std::process::{Command, Output, Stdio};

use mockito::{Matcher, Mock, Server, ServerGuard};
use serde_json::{Value, json};
use tempfile::TempDir;

struct Run {
    output: Output,
}

impl Run {
    fn stdout(&self) -> String {
        String::from_utf8_lossy(&self.output.stdout).into_owned()
    }

    fn stderr(&self) -> String {
        String::from_utf8_lossy(&self.output.stderr).into_owned()
    }

    fn json(&self) -> Value {
        serde_json::from_str(&self.stdout()).unwrap()
    }
}

fn ferr0(server: &ServerGuard, args: &[&str]) -> Run {
    let config = TempDir::new().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ferr0"))
        .args(["--url", &server.url()])
        .args(args)
        .env("XDG_CONFIG_HOME", config.path())
        .env_remove("FERR0_URL")
        .env_remove("FERR0_API_KEY")
        .env_remove("FERR0_USER_ID")
        .env_remove("FERR0_AGENT_ID")
        .env_remove("FERR0_RUN_ID")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    Run { output }
}

fn never(server: &mut ServerGuard, method: &str) -> Mock {
    server
        .mock(method, Matcher::Any)
        .expect(0)
        .with_body("{}")
        .create()
}

#[test]
fn delete_all_deletes_the_scope_with_force() {
    let mut server = Server::new();
    let delete = server
        .mock("DELETE", "/memories")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("user_id".into(), "alice".into()),
            Matcher::UrlEncoded("agent_id".into(), "claude-code".into()),
        ]))
        .with_body(r#"{"message": "All relevant memories deleted"}"#)
        .create();

    let run = ferr0(
        &server,
        &[
            "delete",
            "--all",
            "--user-id",
            "alice",
            "--agent-id",
            "claude-code",
            "--force",
        ],
    );

    delete.assert();
    assert!(run.output.status.success(), "{}", run.stderr());
    assert_eq!(run.stdout(), "All matching memories deleted\n");
}

#[test]
fn delete_all_without_a_scope_leaves_the_refusal_to_the_server() {
    let mut server = Server::new();
    let delete = server
        .mock("DELETE", "/memories")
        .match_query(Matcher::Exact(String::new()))
        .with_status(400)
        .with_body(r#"{"detail": "At least one identifier is required."}"#)
        .create();

    let run = ferr0(&server, &["delete", "--all", "--user-id", "", "--force"]);

    delete.assert();
    assert!(!run.output.status.success());
    assert!(
        run.stderr()
            .contains("At least one identifier is required.")
    );
}

#[test]
fn destructive_commands_with_json_require_force_even_for_a_dry_run() {
    let mut server = Server::new();
    let delete = never(&mut server, "DELETE");
    let list = never(&mut server, "GET");
    let reset = never(&mut server, "POST");

    for args in [
        &["--json", "delete", "--all", "--user-id", "alice"][..],
        &[
            "--json",
            "delete",
            "--all",
            "--user-id",
            "alice",
            "--dry-run",
        ],
        &["--json", "reset"],
    ] {
        let run = ferr0(&server, args);

        assert!(!run.output.status.success());
        assert_eq!(
            run.json(),
            json!({"error": {
                "message": "Destructive operation requires --force in agent mode.",
            }})
        );
    }
    delete.assert();
    list.assert();
    reset.assert();
}

#[test]
fn delete_rejects_conflicting_modes() {
    let server = Server::new();

    let run = ferr0(&server, &["delete", "abc", "--all", "--user-id", "alice"]);

    assert_eq!(run.output.status.code(), Some(2));
    assert!(run.stderr().contains("cannot be used with"));
}

#[test]
fn delete_all_dry_run_counts_the_scope_without_deleting() {
    let mut server = Server::new();
    let delete = never(&mut server, "DELETE");
    let list = server
        .mock("GET", "/memories")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("user_id".into(), "alice".into()),
            Matcher::UrlEncoded("top_k".into(), "1000".into()),
            Matcher::UrlEncoded("show_expired".into(), "true".into()),
        ]))
        .with_body(r#"{"results": [{"id": "1", "memory": "likes tea"}]}"#)
        .create();

    let run = ferr0(
        &server,
        &["delete", "--all", "--dry-run", "--user-id", "alice"],
    );

    list.assert();
    delete.assert();
    assert!(run.output.status.success(), "{}", run.stderr());
    assert_eq!(
        run.stdout(),
        "Would delete 1 memory.\nNo changes made (dry run).\n"
    );
    assert_eq!(run.stderr(), "");
}

#[test]
fn delete_all_dry_run_warns_when_the_listing_hits_the_server_limit() {
    let mut server = Server::new();
    let results: Vec<_> = (0..1000)
        .map(|id| json!({"id": id.to_string(), "memory": "likes tea"}))
        .collect();
    server
        .mock("GET", "/memories")
        .match_query(Matcher::Any)
        .with_body(json!({ "results": results }).to_string())
        .create();

    let run = ferr0(
        &server,
        &["delete", "--all", "--dry-run", "--user-id", "alice"],
    );

    assert!(run.output.status.success(), "{}", run.stderr());
    assert!(run.stdout().starts_with("Would delete 1000 memories."));
    assert!(run.stderr().contains("Counted the first 1000 memories"));
}

#[test]
fn delete_dry_run_shows_the_memory_without_deleting() {
    let mut server = Server::new();
    let delete = never(&mut server, "DELETE");
    let get = server
        .mock("GET", "/memories/abc")
        .with_body(r#"{"id": "abc", "memory": "likes tea"}"#)
        .create();

    let run = ferr0(&server, &["delete", "abc", "--dry-run"]);

    get.assert();
    delete.assert();
    assert_eq!(run.stdout(), "abc  likes tea\nNo changes made (dry run).\n");
}

#[test]
fn reset_deletes_every_memory_with_force() {
    let mut server = Server::new();
    let reset = server
        .mock("POST", "/reset")
        .with_body(r#"{"message": "All memories reset"}"#)
        .create();

    let run = ferr0(&server, &["reset", "--force"]);

    reset.assert();
    assert_eq!(run.stdout(), "All memories deleted\n");
}

#[test]
fn reset_rejects_scope_flags() {
    let mut server = Server::new();
    let reset = never(&mut server, "POST");

    let run = ferr0(&server, &["reset", "--user-id", "alice", "--force"]);

    reset.assert();
    assert!(!run.output.status.success());
    assert!(run.stderr().contains("takes no --user-id"));
}

#[test]
fn destructive_commands_without_force_fail_when_stdin_is_not_a_terminal() {
    let mut server = Server::new();
    let delete = never(&mut server, "DELETE");
    let reset = never(&mut server, "POST");

    for args in [
        &["delete", "--all", "--user-id", "alice"][..],
        &["reset"],
        &["entity", "delete", "--user-id", "alice"],
    ] {
        let run = ferr0(&server, args);

        assert!(!run.output.status.success());
        assert!(run.stderr().contains("pass --force to confirm"));
    }
    delete.assert();
    reset.assert();
}

#[test]
fn entity_list_filters_by_type() {
    let mut server = Server::new();
    server
        .mock("GET", "/entities")
        .with_body(
            json!([
                {"id": "alice", "type": "user", "total_memories": 3},
                {"id": "claude-code", "type": "agent", "total_memories": 1},
            ])
            .to_string(),
        )
        .create();

    let run = ferr0(&server, &["--json", "entity", "list", "agents"]);

    assert!(run.output.status.success(), "{}", run.stderr());
    assert_eq!(
        run.json(),
        json!([{"id": "claude-code", "type": "agent", "total_memories": 1}])
    );
}

#[test]
fn entity_delete_deletes_each_given_entity_with_force() {
    let mut server = Server::new();
    let user = server
        .mock("DELETE", "/entities/user/alice")
        .with_body(r#"{"message": "Entity deleted"}"#)
        .create();
    let agent = server
        .mock("DELETE", "/entities/agent/claude-code")
        .with_body(r#"{"message": "Entity deleted"}"#)
        .create();

    let run = ferr0(
        &server,
        &[
            "--json",
            "entity",
            "delete",
            "--user-id",
            "alice",
            "--agent-id",
            "claude-code",
            "--force",
        ],
    );

    user.assert();
    agent.assert();
    assert_eq!(
        run.json(),
        json!({
            "user": {"message": "Entity deleted"},
            "agent": {"message": "Entity deleted"},
        })
    );
}

#[test]
fn entity_delete_needs_an_entity_flag() {
    let mut server = Server::new();
    let delete = never(&mut server, "DELETE");

    let run = ferr0(&server, &["entity", "delete", "--force"]);

    delete.assert();
    assert!(!run.output.status.success());
    assert!(
        run.stderr()
            .contains("Provide at least one of --user-id, --agent-id, --run-id.")
    );
}

#[test]
fn entity_delete_dry_run_makes_no_request() {
    let mut server = Server::new();
    let delete = never(&mut server, "DELETE");

    let run = ferr0(
        &server,
        &["entity", "delete", "--run-id", "r1", "--dry-run"],
    );

    delete.assert();
    assert_eq!(
        run.stdout(),
        "Would delete entity run=r1 and all its memories.\nNo changes made (dry run).\n"
    );
}

#[test]
fn entity_delete_dry_run_prints_json_with_json() {
    let mut server = Server::new();
    let delete = never(&mut server, "DELETE");

    let run = ferr0(
        &server,
        &[
            "--json",
            "entity",
            "delete",
            "--user-id",
            "alice",
            "--dry-run",
            "--force",
        ],
    );

    delete.assert();
    assert_eq!(
        run.json(),
        json!({"message": "Would delete entity user=alice and all its memories."})
    );
}
