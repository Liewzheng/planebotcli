//! Configuration with precedence: CLI flags > env vars > config file.
//!
//! The config file is discovered from several locations, highest priority first:
//! `~/.config/pbot/config.toml` (TOML), `~/.pbot`, `~/.planecli`, then the legacy
//! `~/.plane_api` (all `key=value` lines with lowercase keys). `configure` keeps writing
//! `~/.plane_api` for backward compatibility.

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

/// The path `pbot configure` writes to — the highest-priority existing candidate, else
/// the legacy `~/.plane_api`. Writes therefore land in the file reads actually consult.
pub fn config_file_path() -> PathBuf {
    let home = home_dir();
    active_config_path(&config_file_candidates(), &home.join(CONFIG_FILE))
}

/// Select the first existing candidate, else the legacy path.
fn active_config_path(candidates: &[PathBuf], legacy: &Path) -> PathBuf {
    candidates
        .iter()
        .find(|c| c.is_file())
        .cloned()
        .unwrap_or_else(|| legacy.to_path_buf())
}

fn home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// Candidate config files, highest priority first. The first one that exists is read;
/// a higher-priority file fully shadows the ones below it. Empty when there is no home
/// directory (environment variables still work).
pub fn config_file_candidates() -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    vec![
        home.join(".config/pbot/config.toml"),
        home.join(".pbot"),
        home.join(".planecli"),
        home.join(CONFIG_FILE),
    ]
}

/// Read config pairs from a file — TOML when the extension is `.toml`, otherwise
/// `key=value` lines (lowercase keys, `#` comments).
pub fn read_config_file_from(path: &Path) -> HashMap<String, String> {
    if path.extension().is_some_and(|ext| ext == "toml") {
        read_toml_config(path)
    } else {
        read_key_value_config(path)
    }
}

/// Read `key=value` pairs from a config file (lowercase keys, `#` comments).
fn read_key_value_config(path: &Path) -> HashMap<String, String> {
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

/// Parse a TOML config file: top-level `base_url` / `api_key` / `workspace`,
/// with an optional `[auth]` section as fallback. Integer and boolean values are
/// accepted and converted to their string form.
fn read_toml_config(path: &Path) -> HashMap<String, String> {
    let mut values = HashMap::new();
    let Ok(content) = std::fs::read_to_string(path) else {
        return values;
    };
    let Ok(doc) = content.parse::<toml::Value>() else {
        eprintln!(
            "warning: {} is not valid TOML and was ignored",
            path.display()
        );
        return values;
    };
    for key in ["base_url", "api_key", "workspace"] {
        let value = doc
            .get(key)
            .and_then(toml_string)
            .or_else(|| {
                doc.get("auth")
                    .and_then(|t| t.get(key))
                    .and_then(toml_string)
            });
        if let Some(v) = value {
            values.insert(key.to_string(), v);
        }
    }
    values
}

/// A TOML value as a plain string: strings verbatim, integers and booleans formatted.
fn toml_string(value: &toml::Value) -> Option<String> {
    match value {
        toml::Value::String(s) => Some(s.clone()),
        toml::Value::Integer(i) => Some(i.to_string()),
        toml::Value::Boolean(b) => Some(b.to_string()),
        _ => None,
    }
}

/// First candidate that yields any pairs wins; empty or unreadable files are skipped
/// with a warning so a broken high-priority file cannot shadow the rest.
fn read_first_existing(paths: &[PathBuf]) -> HashMap<String, String> {
    for path in paths {
        if !path.is_file() {
            continue;
        }
        let values = read_config_file_from(path);
        if !values.is_empty() {
            warn_if_world_readable(path);
            return values;
        }
        eprintln!(
            "warning: {} is empty or unreadable; trying the next config location",
            path.display()
        );
    }
    HashMap::new()
}

/// Warn (not fail) when a config file is readable by other users, since it holds an API key.
#[cfg(unix)]
fn warn_if_world_readable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = std::fs::metadata(path) {
        let mode = meta.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            eprintln!(
                "warning: {} is readable by other users (mode {mode:03o}); chmod 600 it to keep the API key private",
                path.display()
            );
        }
    }
}

#[cfg(not(unix))]
fn warn_if_world_readable(_path: &Path) {}

/// Read the first existing config file from the candidate list (highest priority first).
pub fn read_config_file() -> HashMap<String, String> {
    read_first_existing(&config_file_candidates())
}

/// Save config to the active config file (the highest-priority existing candidate, else
/// `~/.plane_api`), in `key=value` or TOML form depending on the target's extension.
/// Writes go through `write_config_file` so tests can target a temp path.
pub fn save_config(base_url: &str, api_key: &str, workspace: &str) -> std::io::Result<()> {
    write_config_file(&config_file_path(), base_url, api_key, workspace)
}

fn write_config_file(
    path: &Path,
    base_url: &str,
    api_key: &str,
    workspace: &str,
) -> std::io::Result<()> {
    let content = if path.extension().is_some_and(|ext| ext == "toml") {
        format!(
            "base_url = \"{base_url}\"\napi_key = \"{api_key}\"\nworkspace = \"{workspace}\"\n"
        )
    } else {
        format!("base_url={base_url}\napi_key={api_key}\nworkspace={workspace}\n")
    };
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
        path.push(format!("planebotcli_test_cfg_{}", std::process::id()));
        std::fs::write(&path, "# comment\nbase_url = \"http://x\"\napi_key=abc\n").unwrap();
        let values = read_config_file_from(&path);
        std::fs::remove_file(&path).unwrap();
        assert_eq!(values.get("base_url").map(String::as_str), Some("http://x"));
        assert_eq!(values.get("api_key").map(String::as_str), Some("abc"));
        assert!(!values.contains_key("workspace"));
    }

    #[test]
    fn parses_toml_config_top_level() {
        let mut path = std::env::temp_dir();
        path.push(format!("planebotcli_test_toml_{}.toml", std::process::id()));
        std::fs::write(
            &path,
            "base_url = \"http://x\"\napi_key = \"abc\"\nworkspace = \"ws\"\n",
        )
        .unwrap();
        let values = read_config_file_from(&path);
        std::fs::remove_file(&path).unwrap();
        assert_eq!(values.get("base_url").map(String::as_str), Some("http://x"));
        assert_eq!(values.get("api_key").map(String::as_str), Some("abc"));
        assert_eq!(values.get("workspace").map(String::as_str), Some("ws"));
    }

    #[test]
    fn parses_toml_config_auth_section() {
        let mut path = std::env::temp_dir();
        path.push(format!("planebotcli_test_toml_auth_{}.toml", std::process::id()));
        std::fs::write(
            &path,
            "[auth]\nbase_url = \"http://y\"\napi_key = \"def\"\nworkspace = \"ws2\"\n",
        )
        .unwrap();
        let values = read_config_file_from(&path);
        std::fs::remove_file(&path).unwrap();
        assert_eq!(values.get("base_url").map(String::as_str), Some("http://y"));
        assert_eq!(values.get("api_key").map(String::as_str), Some("def"));
        assert_eq!(values.get("workspace").map(String::as_str), Some("ws2"));
    }

    #[test]
    fn toml_top_level_wins_over_auth_section() {
        let mut path = std::env::temp_dir();
        path.push(format!("planebotcli_test_toml_prio_{}.toml", std::process::id()));
        std::fs::write(
            &path,
            "base_url = \"http://top\"\n[auth]\nbase_url = \"http://auth\"\n",
        )
        .unwrap();
        let values = read_config_file_from(&path);
        std::fs::remove_file(&path).unwrap();
        assert_eq!(values.get("base_url").map(String::as_str), Some("http://top"));
    }

    #[test]
    fn first_existing_candidate_wins() {
        let dir = std::env::temp_dir().join(format!("planebotcli_cand_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let high = dir.join("high");
        let low = dir.join("low");
        std::fs::write(&high, "base_url=http://high\n").unwrap();
        std::fs::write(&low, "base_url=http://low\n").unwrap();
        let values = read_first_existing(&[high.clone(), low.clone()]);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(values.get("base_url").map(String::as_str), Some("http://high"));
    }

    #[test]
    fn missing_file_reads_nothing() {
        let values = read_config_file_from(&PathBuf::from("/nonexistent/planebotcli-config"));
        assert!(values.is_empty());
    }

    #[test]
    fn candidates_order_is_high_to_low() {
        let home = dirs::home_dir().unwrap();
        let c = config_file_candidates();
        assert_eq!(c[0], home.join(".config/pbot/config.toml"));
        assert!(c.contains(&home.join(".pbot")));
        assert!(c.contains(&home.join(".planecli")));
        assert!(c.contains(&home.join(".plane_api")));
    }

    #[test]
    fn save_config_writes_key_value_file() {
        let mut path = std::env::temp_dir();
        path.push(format!("planebotcli_save_test_{}", std::process::id()));
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

    #[test]
    fn save_config_writes_toml_for_toml_path() {
        let mut path = std::env::temp_dir();
        path.push(format!("planebotcli_save_toml_{}.toml", std::process::id()));
        write_config_file(&path, "http://plane.example", "secret", "ws1").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!(content.contains("base_url = \"http://plane.example\""));
        assert!(content.contains("api_key = \"secret\""));
        assert!(content.contains("workspace = \"ws1\""));
    }

    #[test]
    fn active_path_prefers_existing_candidate() {
        let dir = std::env::temp_dir().join(format!("planebotcli_ap_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let existing = dir.join("config.toml");
        std::fs::write(&existing, "base_url = \"http://x\"\n").unwrap();
        let legacy = dir.join("legacy");
        let chosen = active_config_path(
            &[existing.clone(), dir.join("missing")],
            &legacy,
        );
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(chosen, existing);
    }

    #[test]
    fn active_path_falls_back_to_legacy_when_none_exist() {
        let dir = std::env::temp_dir().join(format!("planebotcli_ap2_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let legacy = dir.join("legacy");
        let chosen = active_config_path(&[dir.join("a"), dir.join("b")], &legacy);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(chosen, legacy);
    }

    #[test]
    fn empty_file_is_skipped_for_next_candidate() {
        let dir = std::env::temp_dir().join(format!("planebotcli_skip_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let empty = dir.join("empty");
        let filled = dir.join("filled");
        std::fs::write(&empty, "").unwrap();
        std::fs::write(&filled, "base_url=http://ok\n").unwrap();
        let values = read_first_existing(&[empty.clone(), filled.clone()]);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(values.get("base_url").map(String::as_str), Some("http://ok"));
    }

    #[test]
    fn toml_accepts_integer_and_boolean_values() {
        let mut path = std::env::temp_dir();
        path.push(format!("planebotcli_toml_types_{}.toml", std::process::id()));
        std::fs::write(
            &path,
            "base_url = \"http://x\"\napi_key = 12345\nworkspace = true\n",
        )
        .unwrap();
        let values = read_config_file_from(&path);
        std::fs::remove_file(&path).unwrap();
        assert_eq!(values.get("api_key").map(String::as_str), Some("12345"));
        assert_eq!(values.get("workspace").map(String::as_str), Some("true"));
    }
}
