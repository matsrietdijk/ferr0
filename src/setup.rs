use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use dialoguer::{Confirm, Input, MultiSelect, Password, Select, theme::ColorfulTheme};

use reqwest::StatusCode;

use crate::{
    client::{Client, Scope, ServerError},
    config::{self, FileConfig},
};

const SKILL: &str = include_str!("../skills/ferr0/SKILL.md");
const DEFAULT_URL: &str = "http://localhost:8888";

const SKILL_DIRS: [(&str, Option<&str>, &str, bool); 8] = [
    (
        "shared: Codex, Gemini CLI, Cursor, Copilot, OpenCode, Windsurf, Amp, Goose",
        None,
        ".agents/skills",
        true,
    ),
    ("Claude Code", Some(".claude"), ".claude/skills", true),
    ("Gemini CLI", Some(".gemini"), ".gemini/skills", false),
    ("Cursor", Some(".cursor"), ".cursor/skills", false),
    ("GitHub Copilot", Some(".copilot"), ".copilot/skills", false),
    (
        "OpenCode",
        Some(".config/opencode"),
        ".config/opencode/skills",
        false,
    ),
    (
        "Windsurf",
        Some(".codeium/windsurf"),
        ".codeium/windsurf/skills",
        false,
    ),
    ("Amp", Some(".config/amp"), ".config/amp/skills", false),
];

struct SkillTarget {
    label: String,
    path: PathBuf,
    preselected: bool,
}

#[derive(Debug, PartialEq)]
enum SkillState {
    Missing,
    Current,
    Different,
}

pub fn run(config_path: &Path) -> Result<()> {
    let theme = ColorfulTheme::default();
    let config = prompt_config(&theme, config::load(config_path)?)?;
    config::save(config_path, &config)?;
    println!("Saved to {}", config_path.display());

    let home = env::home_dir().context("cannot determine home directory")?;
    install_skills(&theme, &skill_targets(&home))
}

fn prompt_config(theme: &ColorfulTheme, mut config: FileConfig) -> Result<FileConfig> {
    loop {
        let url = Input::<String>::with_theme(theme)
            .with_prompt("Mem0 server URL")
            .default(config.url.clone().unwrap_or(DEFAULT_URL.into()))
            .interact_text()?;

        let key_prompt = match config.api_key {
            Some(_) => "API key (leave empty to keep the current key)",
            None => "API key (leave empty if the server has no authentication)",
        };
        let api_key = Password::with_theme(theme)
            .with_prompt(key_prompt)
            .allow_empty_password(true)
            .interact()?;

        let mut user_input = Input::<String>::with_theme(theme).with_prompt("User id");
        if let Some(user_id) = config.user_id.clone().or_else(shell_user) {
            user_input = user_input.default(user_id);
        }
        let user_id = user_input.interact_text()?;

        config = FileConfig {
            url: Some(url),
            api_key: Some(api_key)
                .filter(|key| !key.is_empty())
                .or(config.api_key),
            user_id: Some(user_id),
            ..config
        };

        match verify(&config) {
            Ok(()) => {
                println!("Connected to the Mem0 server.");
                return Ok(config);
            }
            Err(error) => {
                eprintln!("Could not verify the connection: {error:#}");
                let choice = Select::with_theme(theme)
                    .with_prompt("What now?")
                    .items(["Re-enter the settings", "Save anyway"])
                    .default(0)
                    .interact()?;
                if choice == 1 {
                    return Ok(config);
                }
            }
        }
    }
}

fn verify(config: &FileConfig) -> Result<()> {
    let client = Client::new(
        config.url.as_deref().unwrap_or_default(),
        config.api_key.clone(),
    )?;
    let scope = Scope {
        user_id: config.user_id.clone(),
        ..Scope::default()
    };
    match client.me() {
        Err(error)
            if error
                .downcast_ref::<ServerError>()
                .is_some_and(|error| error.status == StatusCode::NOT_FOUND) =>
        {
            client.list(&scope, Some(1)).map(drop)
        }
        result => result.map(drop),
    }
}

fn shell_user() -> Option<String> {
    ["USER", "USERNAME"]
        .into_iter()
        .filter_map(|name| env::var(name).ok())
        .find(|user| !user.is_empty())
}

fn install_skills(theme: &ColorfulTheme, targets: &[SkillTarget]) -> Result<()> {
    let mut current = Vec::new();
    let mut pending = Vec::new();
    for target in targets {
        match skill_state(&target.path)? {
            SkillState::Current => current.push(&target.label),
            state => pending.push((target, state)),
        }
    }
    if !current.is_empty() {
        println!("The ferr0 agent skill is already installed and up to date in:");
        for label in current {
            println!("  {label}");
        }
    }
    if pending.is_empty() {
        return Ok(());
    }

    let labels: Vec<_> = pending
        .iter()
        .map(|(target, state)| match state {
            SkillState::Different => format!("{} [differs from this version]", target.label),
            _ => target.label.clone(),
        })
        .collect();
    let defaults: Vec<_> = pending
        .iter()
        .map(|(target, _)| target.preselected)
        .collect();
    let chosen = MultiSelect::with_theme(theme)
        .with_prompt("Install the ferr0 agent skill (space to toggle, enter to confirm)")
        .items(&labels)
        .defaults(&defaults)
        .interact()?;
    if chosen.is_empty() {
        println!("Skipped installing the agent skill.");
    }

    for (target, state) in chosen.into_iter().map(|index| &pending[index]) {
        let path = &target.path;
        let replace = *state != SkillState::Different
            || Confirm::with_theme(theme)
                .with_prompt(format!("Replace the existing skill at {}?", path.display()))
                .default(false)
                .interact()?;
        if replace {
            write_skill(path)?;
            println!("Installed {}", path.display());
        } else {
            println!("Kept {}", path.display());
        }
    }
    Ok(())
}

fn skill_targets(home: &Path) -> Vec<SkillTarget> {
    SKILL_DIRS
        .into_iter()
        .filter(|(_, app_dir, _, _)| app_dir.is_none_or(|dir| home.join(dir).is_dir()))
        .map(|(agents, _, skills_dir, preselected)| SkillTarget {
            label: format!("~/{skills_dir} ({agents})"),
            path: home.join(skills_dir).join("ferr0").join("SKILL.md"),
            preselected,
        })
        .collect()
}

fn skill_state(path: &Path) -> Result<SkillState> {
    match fs::read_to_string(path) {
        Ok(contents) if contents == SKILL => Ok(SkillState::Current),
        Ok(_) => Ok(SkillState::Different),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(SkillState::Missing),
        Err(error) => Err(error).with_context(|| format!("cannot read {}", path.display())),
    }
}

fn write_skill(path: &Path) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    }
    fs::write(path, SKILL).with_context(|| format!("cannot write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offers_shared_folder_and_detected_agents_only() {
        let home = tempfile::tempdir().unwrap();
        fs::create_dir(home.path().join(".claude")).unwrap();
        fs::create_dir_all(home.path().join(".config/opencode")).unwrap();

        let targets = skill_targets(home.path());

        let offered: Vec<_> = targets
            .iter()
            .map(|target| {
                (
                    target.path.strip_prefix(home.path()).unwrap(),
                    target.preselected,
                )
            })
            .collect();
        assert_eq!(
            offered,
            [
                (Path::new(".agents/skills/ferr0/SKILL.md"), true),
                (Path::new(".claude/skills/ferr0/SKILL.md"), true),
                (Path::new(".config/opencode/skills/ferr0/SKILL.md"), false),
            ]
        );
    }

    #[test]
    fn writes_skill_and_reports_its_state() {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join(".agents/skills/ferr0/SKILL.md");
        assert_eq!(skill_state(&path).unwrap(), SkillState::Missing);

        write_skill(&path).unwrap();
        assert_eq!(skill_state(&path).unwrap(), SkillState::Current);

        fs::write(&path, "older skill").unwrap();
        assert_eq!(skill_state(&path).unwrap(), SkillState::Different);
    }

    fn alice(server: &mockito::Server) -> FileConfig {
        FileConfig {
            url: Some(server.url()),
            api_key: Some("secret".into()),
            user_id: Some("alice".into()),
            ..FileConfig::default()
        }
    }

    #[test]
    fn verify_checks_the_api_key_with_auth_me() {
        let mut server = mockito::Server::new();
        let me = server
            .mock("GET", "/auth/me")
            .match_header("x-api-key", "secret")
            .with_body(r#"{"id": "1"}"#)
            .create();
        let list = server.mock("GET", "/memories").expect(0).create();

        verify(&alice(&server)).unwrap();

        me.assert();
        list.assert();
    }

    #[test]
    fn verify_reports_rejected_api_key() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/auth/me").with_status(401).create();

        let error = verify(&alice(&server)).unwrap_err();

        assert!(
            error.to_string().starts_with("server returned 401"),
            "{error}"
        );
    }

    #[test]
    fn verify_falls_back_to_listing_on_servers_without_auth_me() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/auth/me").with_status(404).create();
        let mock = server
            .mock("GET", "/memories")
            .match_header("x-api-key", "secret")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("user_id".into(), "alice".into()),
                mockito::Matcher::UrlEncoded("top_k".into(), "1".into()),
            ]))
            .with_body(r#"{"results": []}"#)
            .create();

        verify(&alice(&server)).unwrap();

        mock.assert();
    }
}
