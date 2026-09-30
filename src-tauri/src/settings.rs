//! App settings, stored as JSON in the OS config dir.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::usage::{Caps, Thresholds};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Destroy the island window while idle.
    pub low_memory: bool,
    /// Minutes without an active session before the island is torn down.
    pub idle_minutes: u64,
    pub pause_approvals: bool,
    pub thresholds: Thresholds,
    pub caps: Caps,
    /// Substring of a model id mapped to its context window size.
    pub context_overrides: HashMap<String, u64>,
    /// Set after the first launch enabled autostart.
    pub first_run_done: bool,
    pub hooks_notice_shown: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            low_memory: true,
            idle_minutes: 3,
            pause_approvals: false,
            thresholds: Thresholds::default(),
            caps: Caps::default(),
            context_overrides: HashMap::new(),
            first_run_done: false,
            hooks_notice_shown: false,
        }
    }
}

pub fn app_config_dir() -> PathBuf {
    dirs::config_dir().unwrap_or_else(std::env::temp_dir).join("clawed")
}

pub fn settings_path() -> PathBuf {
    app_config_dir().join("settings.json")
}

impl Settings {
    /// Missing or unreadable files give defaults.
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("sub/settings.json");
        let s = Settings::load(&p);
        assert!(s.low_memory);
        assert_eq!(s.idle_minutes, 3);
        let s2 = Settings { idle_minutes: 7, ..s };
        s2.save(&p).unwrap();
        assert_eq!(Settings::load(&p), s2);
    }

    #[test]
    fn partial_file_fills_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("settings.json");
        std::fs::write(&p, r#"{"low_memory": false}"#).unwrap();
        let s = Settings::load(&p);
        assert!(!s.low_memory);
        assert_eq!(s.thresholds.warn, 70.0);
    }
}
