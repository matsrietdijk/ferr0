use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::{
    cli::{ConfigKey, GlobalArgs},
    client::Scope,
};

#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FileConfig {
    pub url: Option<String>,
    pub api_key: Option<String>,
    pub user_id: Option<String>,
    pub agent_id: Option<String>,
    pub run_id: Option<String>,
}

impl FileConfig {
    pub fn set(&mut self, key: ConfigKey, value: String) {
        let slot = match key {
            ConfigKey::Url => &mut self.url,
            ConfigKey::ApiKey => &mut self.api_key,
            ConfigKey::UserId => &mut self.user_id,
            ConfigKey::AgentId => &mut self.agent_id,
            ConfigKey::RunId => &mut self.run_id,
        };
        *slot = Some(value);
    }

    pub fn get(&self, key: ConfigKey, env: impl Fn(&str) -> Option<String>) -> String {
        let (name, stored) = match key {
            ConfigKey::Url => ("FERR0_URL", &self.url),
            ConfigKey::ApiKey => ("FERR0_API_KEY", &self.api_key),
            ConfigKey::UserId => ("FERR0_USER_ID", &self.user_id),
            ConfigKey::AgentId => ("FERR0_AGENT_ID", &self.agent_id),
            ConfigKey::RunId => ("FERR0_RUN_ID", &self.run_id),
        };
        let value = present(env(name))
            .or_else(|| stored.clone())
            .unwrap_or_default();
        match key {
            ConfigKey::ApiKey => redact(&value),
            _ => value,
        }
    }

    pub fn describe(&self) -> String {
        let unset = "(unset)";
        format!(
            "url = {}\napi_key = {}\nuser_id = {}\nagent_id = {}\nrun_id = {}",
            self.url.as_deref().unwrap_or(unset),
            if self.api_key.is_some() {
                "(set)"
            } else {
                unset
            },
            self.user_id.as_deref().unwrap_or(unset),
            self.agent_id.as_deref().unwrap_or(unset),
            self.run_id.as_deref().unwrap_or(unset),
        )
    }
}

fn redact(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    match chars.len() {
        0 => "(not set)".into(),
        1..=8 => chars.iter().take(2).chain(&['*'; 3]).collect(),
        len => {
            let (head, tail) = (&chars[..4], &chars[len - 4..]);
            format!("{}...{}", String::from_iter(head), String::from_iter(tail))
        }
    }
}

pub struct Settings {
    pub url: String,
    pub api_key: Option<String>,
    pub scope: Scope,
}

pub fn default_path() -> Result<PathBuf> {
    let base = match env::var_os("XDG_CONFIG_HOME").filter(|dir| !dir.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => env::home_dir()
            .context("cannot determine home directory")?
            .join(".config"),
    };
    Ok(base.join("ferr0").join("config.toml"))
}

pub fn load(path: &Path) -> Result<FileConfig> {
    if !path.exists() {
        return Ok(FileConfig::default());
    }
    let contents =
        fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    toml::from_str(&contents).with_context(|| format!("invalid config in {}", path.display()))
}

pub fn save(path: &Path, config: &FileConfig) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options
        .open(path)
        .with_context(|| format!("cannot write {}", path.display()))?;
    file.write_all(toml::to_string(config)?.as_bytes())
        .with_context(|| format!("cannot write {}", path.display()))
}

pub fn resolve(args: GlobalArgs, mut file: FileConfig) -> Result<Settings> {
    let url = args
        .url
        .or(file.url.take())
        .context("no server URL: pass --url, set FERR0_URL, or run `ferr0 config set url <url>`")?;
    let api_key = args.api_key.or(file.api_key.take());
    Ok(Settings {
        url,
        api_key,
        scope: scope(args.user_id, args.agent_id, args.run_id, file, |name| {
            env::var(name).ok()
        }),
    })
}

fn present(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.is_empty())
}

fn scope(
    user_id: Option<String>,
    agent_id: Option<String>,
    run_id: Option<String>,
    file: FileConfig,
    env: impl Fn(&str) -> Option<String>,
) -> Scope {
    let (user_id, agent_id, run_id) = (present(user_id), present(agent_id), present(run_id));
    if user_id.is_some() || agent_id.is_some() || run_id.is_some() {
        return Scope {
            user_id,
            agent_id,
            run_id,
        };
    }
    let default = |name, stored| present(env(name)).or(present(stored));
    Scope {
        user_id: default("FERR0_USER_ID", file.user_id),
        agent_id: default("FERR0_AGENT_ID", file.agent_id),
        run_id: default("FERR0_RUN_ID", file.run_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args() -> GlobalArgs {
        GlobalArgs {
            url: None,
            api_key: None,
            user_id: None,
            agent_id: None,
            run_id: None,
            json: false,
        }
    }

    #[test]
    fn missing_file_loads_empty_config() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            load(&dir.path().join("config.toml")).unwrap(),
            FileConfig::default()
        );
    }

    #[test]
    fn saved_config_loads_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("config.toml");
        let mut config = FileConfig::default();
        config.set(ConfigKey::Url, "http://localhost:8888".into());
        config.set(ConfigKey::ApiKey, "secret".into());

        save(&path, &config).unwrap();

        assert_eq!(load(&path).unwrap(), config);
    }

    #[cfg(unix)]
    #[test]
    fn saved_config_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");

        save(&path, &FileConfig::default()).unwrap();

        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn arguments_override_file_values() {
        let file = FileConfig {
            url: Some("http://file".into()),
            api_key: Some("file-key".into()),
            ..FileConfig::default()
        };
        let settings = resolve(
            GlobalArgs {
                url: Some("http://arg".into()),
                ..args()
            },
            file,
        )
        .unwrap();

        assert_eq!(settings.url, "http://arg");
        assert_eq!(settings.api_key.as_deref(), Some("file-key"));
    }

    fn stored_scope() -> FileConfig {
        FileConfig {
            user_id: Some("file-user".into()),
            agent_id: Some("file-agent".into()),
            run_id: Some("file-run".into()),
            ..FileConfig::default()
        }
    }

    fn ids(scope: Scope) -> [Option<String>; 3] {
        [scope.user_id, scope.agent_id, scope.run_id]
    }

    #[test]
    fn scope_falls_back_per_id_to_env_then_file() {
        let env = |name: &str| (name == "FERR0_AGENT_ID").then(|| "env-agent".to_string());

        let scope = scope(None, None, None, stored_scope(), env);

        assert_eq!(
            ids(scope),
            [
                Some("file-user".into()),
                Some("env-agent".into()),
                Some("file-run".into())
            ]
        );
    }

    #[test]
    fn scope_ignores_empty_env_values() {
        let scope = scope(None, None, None, stored_scope(), |_| Some(String::new()));

        assert_eq!(scope.user_id.as_deref(), Some("file-user"));
    }

    #[test]
    fn scope_treats_empty_ids_as_unset() {
        let file = FileConfig {
            agent_id: Some(String::new()),
            ..stored_scope()
        };

        let scope = scope(Some(String::new()), None, None, file, |_| None);

        assert_eq!(
            ids(scope),
            [Some("file-user".into()), None, Some("file-run".into())]
        );
    }

    #[test]
    fn any_scope_flag_replaces_env_and_stored_ids() {
        let env = |_: &str| Some("env-id".to_string());

        let scope = scope(None, Some("arg-agent".into()), None, stored_scope(), env);

        assert_eq!(ids(scope), [None, Some("arg-agent".into()), None]);
    }

    #[test]
    fn missing_url_is_an_error() {
        let error = resolve(args(), FileConfig::default())
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains("FERR0_URL"), "{error}");
    }

    #[test]
    fn describe_hides_api_key() {
        let config = FileConfig {
            api_key: Some("secret".into()),
            ..FileConfig::default()
        };
        let text = config.describe();
        assert!(!text.contains("secret"));
        assert!(text.contains("api_key = (set)"));
    }

    #[test]
    fn describe_shows_agent_and_run_ids() {
        let mut config = FileConfig::default();
        config.set(ConfigKey::AgentId, "claude-code".into());

        let text = config.describe();

        assert!(text.contains("agent_id = claude-code"), "{text}");
        assert!(text.contains("run_id = (unset)"), "{text}");
    }

    #[test]
    fn get_returns_a_single_value_or_empty() {
        let mut config = FileConfig::default();
        config.set(ConfigKey::RunId, "run-1".into());

        assert_eq!(config.get(ConfigKey::RunId, |_| None), "run-1");
        assert_eq!(config.get(ConfigKey::UserId, |_| None), "");
    }

    #[test]
    fn get_prefers_non_empty_env_values() {
        let mut config = FileConfig::default();
        config.set(ConfigKey::UserId, "file-user".into());
        let env = |name: &str| (name == "FERR0_USER_ID").then(|| "env-user".to_string());

        assert_eq!(config.get(ConfigKey::UserId, env), "env-user");
        assert_eq!(
            config.get(ConfigKey::UserId, |_| Some(String::new())),
            "file-user"
        );
    }

    #[test]
    fn get_redacts_api_key() {
        let mut config = FileConfig::default();
        assert_eq!(config.get(ConfigKey::ApiKey, |_| None), "(not set)");

        config.set(ConfigKey::ApiKey, "m0-abcdefghijkl".into());
        assert_eq!(config.get(ConfigKey::ApiKey, |_| None), "m0-a...ijkl");

        config.set(ConfigKey::ApiKey, "short".into());
        assert_eq!(config.get(ConfigKey::ApiKey, |_| None), "sh***");
    }
}
