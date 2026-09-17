use std::io::{self, IsTerminal, Read};

use anyhow::{Context, Result, bail};

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
}
