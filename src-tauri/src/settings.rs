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
    /// Ask GitHub once a day whether a newer release exists.
    pub check_updates: bool,
    pub colors: Colors,
}

/// Island colors as `#rrggbb`. Ring and bar colors apply below the warn
/// threshold; `warn` and `crit` replace them above it, for every meter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Colors {
    pub working: String,
    pub done: String,
    pub error: String,
    /// Approval requests.
    pub request: String,
    pub five_hour: String,
    pub seven_day: String,
    pub cpu: String,
    pub ram: String,
    pub gpu: String,
    pub ctx: String,
    pub warn: String,
    pub crit: String,
}

impl Default for Colors {
    fn default() -> Self {
        let c = |s: &str| s.to_string();
        Self {
            working: c("#4c8dff"),
            done: c("#34c759"),
            error: c("#ff453a"),
            request: c("#a970ff"),
            five_hour: c("#e5e5ea"),
            seven_day: c("#e5e5ea"),
            cpu: c("#64d2ff"),
            ram: c("#5e5ce6"),
            gpu: c("#66d4cf"),
            ctx: c("#e5e5ea"),
            warn: c("#ffb340"),
            crit: c("#ff453a"),
        }
    }
}

fn is_hex_color(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

impl Colors {
    /// Replaces anything that is not `#rrggbb` with its default, so values
    /// written into CSS can never carry anything but a color.
    pub fn sanitized(self) -> Self {
        let d = Colors::default();
        let pick = |v: String, d: String| if is_hex_color(&v) { v.to_ascii_lowercase() } else { d };
        Self {
            working: pick(self.working, d.working),
            done: pick(self.done, d.done),
            error: pick(self.error, d.error),
            request: pick(self.request, d.request),
            five_hour: pick(self.five_hour, d.five_hour),
            seven_day: pick(self.seven_day, d.seven_day),
            cpu: pick(self.cpu, d.cpu),
            ram: pick(self.ram, d.ram),
            gpu: pick(self.gpu, d.gpu),
            ctx: pick(self.ctx, d.ctx),
            warn: pick(self.warn, d.warn),
            crit: pick(self.crit, d.crit),
        }
    }
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
            check_updates: true,
            colors: Colors::default(),
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
        let s: Self = std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Self { colors: s.colors.clone().sanitized(), ..s }
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
        assert!(s.check_updates);
        let s2 = Settings { idle_minutes: 7, ..s };
        s2.save(&p).unwrap();
        assert_eq!(Settings::load(&p), s2);
    }

    #[test]
    fn bad_colors_fall_back() {
        let c = Colors { cpu: "#ABCDEF".into(), ram: "red;}".into(), gpu: "#12345".into(), ..Colors::default() };
        let c = c.sanitized();
        assert_eq!(c.cpu, "#abcdef");
        assert_eq!(c.ram, Colors::default().ram);
        assert_eq!(c.gpu, Colors::default().gpu);
    }

    #[test]
    fn partial_file_fills_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("settings.json");
        std::fs::write(&p, r#"{"low_memory": false}"#).unwrap();
        let s = Settings::load(&p);
        assert!(!s.low_memory);
        assert_eq!(s.thresholds.warn, 70.0);
        assert_eq!(s.colors, Colors::default());
    }
}
