//! Minimal TTL disk cache: one JSON file per key under a cache directory.
//!
//! Mirrors the Python CLI's per-resource disk cache (`src/planecli/cache.py`):
//! reads are cached with a TTL, and writes invalidate the affected resource.
//! The ops are synchronous file I/O — small files, fast.

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

pub struct Cache {
    dir: PathBuf,
}

impl Default for Cache {
    fn default() -> Self {
        Self::new()
    }
}

impl Cache {
    pub fn new() -> Self {
        let dir = dirs::cache_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("planebotcli");
        Self { dir }
    }

    fn path_for(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{}.json", self.sanitize(key)))
    }

    /// Return the cached value if present and younger than `ttl`.
    pub fn get<T: serde::de::DeserializeOwned>(&self, key: &str, ttl: Duration) -> Option<T> {
        let path = self.path_for(key);
        let modified = std::fs::metadata(&path).ok()?.modified().ok()?;
        if SystemTime::now()
            .duration_since(modified)
            .map(|d| d > ttl)
            .unwrap_or(true)
        {
            return None;
        }
        let data = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&data).ok()
    }

    pub fn set<T: serde::Serialize>(&self, key: &str, value: &T) {
        let _ = std::fs::create_dir_all(&self.dir);
        if let Ok(data) = serde_json::to_string(value) {
            let _ = std::fs::write(self.path_for(key), data);
        }
    }

    /// Remove every entry whose key starts with `key_prefix`.
    pub fn invalidate(&self, key_prefix: &str) {
        let safe_prefix = self.sanitize(key_prefix);
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(&safe_prefix) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    fn sanitize(&self, key: &str) -> String {
        key.chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    }

    pub fn clear(&self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_cache(name: &str) -> Cache {
        Cache {
            dir: std::env::temp_dir().join(format!(
                "planebotcli_cache_test_{}_{name}",
                std::process::id()
            )),
        }
    }

    #[test]
    fn set_and_get_roundtrip() {
        let cache = test_cache("rt");
        cache.clear();
        cache.set("projects:ws", &vec![1u32, 2, 3]);
        let got: Option<Vec<u32>> = cache.get("projects:ws", Duration::from_secs(60));
        assert_eq!(got, Some(vec![1, 2, 3]));
        cache.clear();
    }

    #[test]
    fn ttl_expiry_returns_none() {
        let cache = test_cache("ttl");
        cache.clear();
        cache.set("k", &42u32);
        // TTL of 50ms is long enough that the write is never "already expired".
        let got: Option<u32> = cache.get("k", Duration::from_millis(50));
        std::thread::sleep(Duration::from_millis(80));
        let expired: Option<u32> = cache.get("k", Duration::from_millis(50));
        assert_eq!(got, Some(42));
        assert_eq!(expired, None);
        cache.clear();
    }
}
