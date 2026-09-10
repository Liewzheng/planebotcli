//! Configuration with precedence: CLI flags > env vars > `~/.plane_api`.
//!
//! Mirrors the removed Python line's config module: the config file is `key=value` lines with
//! lowercase keys (`base_url`, `api_key`, `workspace`), `#` comments, chmod 600.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::errors::PlaneError;

#[derive(Debug, Clone)]
pub struct Config {
    pub base_url: String,
    pub api_key: String,
    pub workspace: String,
}

const CONFIG_FILE: &str = ".plane_api";

pub fn config_file_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(CONFIG_FILE)
}

/// Read `key=value` pairs from a config file (lowercase keys, `#` comments).
pub fn read_config_file_from(path: &PathBuf) -> HashMap<String, String> {
    let mut values = HashMap::new();
    let Ok(content) = std::fs::read_to_string(path) else {
        return values;
    };
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let trimmed = value
            .trim()
            .trim_matches(|c: char| c == '"' || c == '\'')
            .to_string();
        values.insert(key.trim().to_ascii_lowercase(), trimmed);
    }
    values
}

pub fn read_config_file() -> HashMap<String, String> {
    read_config_file_from(&config_file_path())
}

/// Save config to `~/.plane_api` with `key=value` lines (chmod 600),
/// mirroring `config.py::save_config`. Writes go through `write_config_file`
/// so tests can target a temp path.
pub fn save_config(base_url: &str, api_key: &str, workspace: &str) -> std::io::Result<()> {
    write_config_file(&config_file_path(), base_url, api_key, workspace)
}

fn write_config_file(
    path: &Path,
    base_url: &str,
    api_key: &str,
    workspace: &str,
) -> std::io::Result<()> {
    let content = format!("base_url={base_url}\napi_key={api_key}\nworkspace={workspace}\n");
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;

        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(content.as_bytes())
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, content)
    }
}

/// Load with explicit env + file maps (injectable for tests).
pub fn load_config_with(
    env: &HashMap<String, String>,
    file: &HashMap<String, String>,
) -> Result<Config, PlaneError> {
    let pick = |env_key: &str, file_key: &str| -> Option<String> {
        env.get(env_key)
            .or_else(|| file.get(file_key))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };

    let base_url = pick("PLANE_BASE_URL", "base_url").ok_or_else(|| PlaneError::Auth {
        message: "Missing base URL. Set PLANE_BASE_URL or run 'planebotcli configure'.".into(),
    })?;
    let api_key = pick("PLANE_API_KEY", "api_key").ok_or_else(|| PlaneError::Auth {
        message: "Missing API key. Set PLANE_API_KEY or run 'planebotcli configure'.".into(),
    })?;
    let workspace = pick("PLANE_WORKSPACE", "workspace").ok_or_else(|| PlaneError::Auth {
        message: "Missing workspace slug. Set PLANE_WORKSPACE or run 'planebotcli configure'."
            .into(),
    })?;

    Ok(Config {
        base_url: base_url.trim_end_matches('/').to_string(),
        api_key,
        workspace,
    })
}

pub fn load_config() -> Result<Config, PlaneError> {
    let mut env = HashMap::new();
    for key in ["PLANE_BASE_URL", "PLANE_API_KEY", "PLANE_WORKSPACE"] {
        if let Ok(v) = std::env::var(key) {
            env.insert(key.to_string(), v);
        }
    }
    let file = read_config_file();
    load_config_with(&env, &file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::EXIT_AUTH;

    fn file_map() -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("base_url".into(), "http://100.64.0.8".into());
        m.insert("api_key".into(), "secret".into());
        m.insert("workspace".into(), "isletspace".into());
        m
    }

    #[test]
    fn uses_file_values() {
        let cfg = load_config_with(&HashMap::new(), &file_map()).unwrap();
        assert_eq!(cfg.base_url, "http://100.64.0.8");
        assert_eq!(cfg.workspace, "isletspace");
    }

    #[test]
    fn env_overrides_file() {
        let mut env = HashMap::new();
        env.insert("PLANE_WORKSPACE".into(), "other".into());
        let cfg = load_config_with(&env, &file_map()).unwrap();
        assert_eq!(cfg.workspace, "other");
        assert_eq!(cfg.base_url, "http://100.64.0.8");
    }

    #[test]
    fn missing_fields_is_auth_error() {
        let err = load_config_with(&HashMap::new(), &HashMap::new()).unwrap_err();
        assert_eq!(err.exit_code(), EXIT_AUTH);
    }

    #[test]
    fn strips_trailing_slash_from_base_url() {
        let mut m = file_map();
        m.insert("base_url".into(), "http://100.64.0.8/".into());
        let cfg = load_config_with(&HashMap::new(), &m).unwrap();
        assert_eq!(cfg.base_url, "http://100.64.0.8");
    }

    #[test]
    fn parses_config_file_lines() {
        let mut path = std::env::temp_dir();
        path.push(format!("pbotcli_test_cfg_{}", std::process::id()));
        std::fs::write(&path, "# comment\nbase_url = \"http://x\"\napi_key=abc\n").unwrap();
        let values = read_config_file_from(&path);
        std::fs::remove_file(&path).unwrap();
        assert_eq!(values.get("base_url").map(String::as_str), Some("http://x"));
        assert_eq!(values.get("api_key").map(String::as_str), Some("abc"));
        assert!(!values.contains_key("workspace"));
    }

    #[test]
    fn save_config_writes_key_value_file() {
        let mut path = std::env::temp_dir();
        path.push(format!("pbotcli_save_test_{}", std::process::id()));
        write_config_file(&path, "http://plane.example", "secret", "ws1").unwrap();
        let values = read_config_file_from(&path);
        let permissions = std::fs::metadata(&path).unwrap().permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(permissions.mode() & 0o777, 0o600);
        }
        std::fs::remove_file(&path).unwrap();
        assert_eq!(
            values.get("base_url").map(String::as_str),
            Some("http://plane.example")
        );
        assert_eq!(values.get("api_key").map(String::as_str), Some("secret"));
        assert_eq!(values.get("workspace").map(String::as_str), Some("ws1"));
    }
}
