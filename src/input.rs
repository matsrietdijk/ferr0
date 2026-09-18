use std::{
    fs,
    io::{self, IsTerminal, Read},
    path::Path,
};

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value};

use crate::client::Message;

pub fn from_arg_or_stdin(arg: Option<String>, name: &str) -> Result<String> {
    let stdin = io::stdin();
    let piped = (!stdin.is_terminal()).then(|| stdin.lock());
    resolve(arg, piped, name)
}

fn resolve(arg: Option<String>, piped: Option<impl Read>, name: &str) -> Result<String> {
    if let Some(arg) = arg {
        return Ok(arg);
    }
    let mut text = String::new();
    if let Some(mut piped) = piped {
        piped
            .read_to_string(&mut text)
            .context("cannot read from stdin")?;
    }
    let text = text.trim();
    if text.is_empty() {
        bail!("no {name} given: pass it as an argument or pipe it on stdin");
    }
    Ok(text.to_string())
}

pub fn messages_from_file(path: &Path) -> Result<Vec<Message>> {
    let json = fs::read_to_string(path)
        .with_context(|| format!("cannot read messages from {}", path.display()))?;
    parse_messages(&json)
}

pub fn parse_messages(json: &str) -> Result<Vec<Message>> {
    let messages: Vec<Message> = serde_json::from_str(json)
        .context("messages must be a JSON array of objects with string role and content fields")?;
    if messages.is_empty() {
        bail!("no messages given: the messages array is empty");
    }
    Ok(messages)
}

pub fn parse_object(json: &str, name: &str) -> Result<Map<String, Value>> {
    serde_json::from_str(json).with_context(|| format!("invalid JSON in {name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_the_argument_over_piped_input() {
        let text = resolve(Some("x".into()), Some("piped".as_bytes()), "text").unwrap();
        assert_eq!(text, "x");
    }

    #[test]
    fn reads_trimmed_piped_input_without_an_argument() {
        let text = resolve(None, Some("likes tea\n".as_bytes()), "text").unwrap();
        assert_eq!(text, "likes tea");
    }

    #[test]
    fn rejects_empty_piped_input() {
        let error = resolve(None, Some(" \n".as_bytes()), "query").unwrap_err();
        assert_eq!(
            error.to_string(),
            "no query given: pass it as an argument or pipe it on stdin"
        );
    }

    #[test]
    fn rejects_a_missing_argument_on_a_terminal() {
        assert!(resolve(None, None::<&[u8]>, "text").is_err());
    }

    #[test]
    fn parses_messages_with_their_roles() {
        let messages = parse_messages(
            r#"[{"role": "user", "content": "use pnpm"}, {"role": "assistant", "content": "switched to pnpm"}]"#,
        )
        .unwrap();
        assert_eq!(
            messages,
            [
                Message::user("use pnpm".into()),
                Message {
                    role: "assistant".into(),
                    content: "switched to pnpm".into(),
                },
            ]
        );
    }

    #[test]
    fn rejects_messages_that_are_not_role_and_content_objects() {
        assert!(parse_messages(r#"["likes tea"]"#).is_err());
        assert!(parse_messages(r#"[{"role": "user"}]"#).is_err());
    }

    #[test]
    fn rejects_an_empty_messages_array() {
        let error = parse_messages("[]").unwrap_err();
        assert_eq!(
            error.to_string(),
            "no messages given: the messages array is empty"
        );
    }

    #[test]
    fn parses_a_json_object() {
        let object = parse_object(r#"{"category": "food"}"#, "--filter").unwrap();
        assert_eq!(object["category"], "food");
    }

    #[test]
    fn rejects_json_that_is_not_an_object() {
        for json in ["[]", r#""food""#, "{", ""] {
            let error = parse_object(json, "--filter").unwrap_err();
            assert_eq!(error.to_string(), "invalid JSON in --filter");
        }
    }
}
