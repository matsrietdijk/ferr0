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
}

impl FileConfig {
    pub fn set(&mut self, key: ConfigKey, value: String) {
        let slot = match key {
            ConfigKey::Url => &mut self.url,
            ConfigKey::ApiKey => &mut self.api_key,
            ConfigKey::UserId => &mut self.user_id,
        };
        *slot = Some(value);
    }

    pub fn describe(&self) -> String {
        let unset = "(unset)";
        format!(
            "url = {}\napi_key = {}\nuser_id = {}",
            self.url.as_deref().unwrap_or(unset),
            if self.api_key.is_some() {
                "(set)"
            } else {
                unset
            },
            self.user_id.as_deref().unwrap_or(unset),
        )
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

pub fn resolve(args: GlobalArgs, file: FileConfig) -> Result<Settings> {
    let url = args
        .url
        .or(file.url)
        .context("no server URL: pass --url, set FERR0_URL, or run `ferr0 config set url <url>`")?;
    Ok(Settings {
        url,
        api_key: args.api_key.or(file.api_key),
        scope: Scope {
            user_id: args.user_id.or(file.user_id),
            agent_id: args.agent_id,
            run_id: args.run_id,
        },
    })
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
            user_id: Some("file-user".into()),
        };
        let settings = resolve(
            GlobalArgs {
                url: Some("http://arg".into()),
                user_id: Some("arg-user".into()),
                ..args()
            },
            file,
        )
        .unwrap();

        assert_eq!(settings.url, "http://arg");
        assert_eq!(settings.api_key.as_deref(), Some("file-key"));
        assert_eq!(settings.scope.user_id.as_deref(), Some("arg-user"));
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
}
