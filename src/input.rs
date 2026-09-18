use std::{
    fs,
    io::{self, IsTerminal, Read},
    path::Path,
};

use anyhow::{Context, Result, bail};
use chrono::{Local, NaiveDate};
use serde_json::Value;

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

pub fn parse_metadata(json: Option<&str>) -> Result<Option<Value>> {
    let Some(json) = json.filter(|json| !json.is_empty()) else {
        return Ok(None);
    };
    let metadata = serde_json::from_str(json).context("invalid JSON in --metadata")?;
    Ok(non_empty(metadata))
}

pub fn non_empty(value: Value) -> Option<Value> {
    let empty = match &value {
        Value::Null => true,
        Value::Bool(value) => !value,
        Value::Number(value) => value.as_f64() == Some(0.0),
        Value::String(value) => value.is_empty(),
        Value::Array(value) => value.is_empty(),
        Value::Object(value) => value.is_empty(),
    };
    (!empty).then_some(value)
}

pub fn parse_expires(date: Option<&str>) -> Result<Option<String>> {
    let Some(date) = date.filter(|date| !date.is_empty()) else {
        return Ok(None);
    };
    validate_expires(date, Local::now().date_naive())?;
    Ok(Some(date.to_string()))
}

fn validate_expires(date: &str, today: NaiveDate) -> Result<()> {
    let digits = date.len() == 10
        && date.bytes().enumerate().all(|(index, byte)| {
            matches!(index, 4 | 7) == (byte == b'-') && (byte == b'-' || byte.is_ascii_digit())
        });
    let date = digits
        .then(|| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
        .flatten()
        .context("invalid date format for --expires: use YYYY-MM-DD (e.g. 2025-12-31)")?;
    if date <= today {
        bail!("--expires date must be in the future");
    }
    Ok(())
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
    fn parses_any_json_metadata() {
        assert_eq!(
            parse_metadata(Some(r#"{"topic": "tools"}"#)).unwrap(),
            Some(serde_json::json!({"topic": "tools"}))
        );
        assert_eq!(
            parse_metadata(Some("[1]")).unwrap(),
            Some(serde_json::json!([1]))
        );
    }

    #[test]
    fn drops_empty_metadata() {
        for json in ["{}", "[]", "\"\"", "null", "false", "0"] {
            assert_eq!(parse_metadata(Some(json)).unwrap(), None, "{json}");
        }
    }

    #[test]
    fn omits_empty_flag_values() {
        assert_eq!(parse_metadata(Some("")).unwrap(), None);
        assert_eq!(parse_expires(Some("")).unwrap(), None);
    }

    #[test]
    fn rejects_malformed_metadata() {
        let error = parse_metadata(Some("{topic: tools}")).unwrap_err();
        assert_eq!(error.to_string(), "invalid JSON in --metadata");
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, 18).unwrap()
    }

    #[test]
    fn accepts_future_calendar_dates() {
        for date in ["2026-09-19", "2028-02-29"] {
            assert!(validate_expires(date, today()).is_ok(), "{date}");
        }
    }

    #[test]
    fn rejects_today_and_past_dates() {
        for date in ["2026-09-18", "2026-09-17"] {
            let error = validate_expires(date, today()).unwrap_err();
            assert_eq!(error.to_string(), "--expires date must be in the future");
        }
    }

    #[test]
    fn rejects_dates_that_are_not_yyyy_mm_dd_calendar_dates() {
        for date in [
            "2027-02-29",
            "2027-04-31",
            "2027-13-01",
            "2027-1-01",
            "2027-01-1x",
            "+2027-01-01",
            "2027-01-01T00:00:00",
            "tomorrow",
        ] {
            let error = validate_expires(date, today()).unwrap_err();
            assert_eq!(
                error.to_string(),
                "invalid date format for --expires: use YYYY-MM-DD (e.g. 2025-12-31)",
                "{date}"
            );
        }
    }
}
