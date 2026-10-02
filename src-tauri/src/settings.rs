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
    dirs::config_dir().unwrap_or_else(std::env::temp_dir).join("islet")
}

pub fn settings_path() -> PathBuf {
    app_config_dir().join("settings.json")
}

/// Copies settings from the pre-rename `clawed` config directory the first
/// time islet runs. Does nothing once islet has its own file.
pub fn migrate_from_clawed(new: &Path) {
    let Some(old) = new.parent().and_then(Path::parent).map(|d| d.join("clawed").join("settings.json")) else {
        return;
    };
    if new.exists() || !old.exists() {
        return;
    }
    if let Some(dir) = new.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match std::fs::copy(&old, new) {
        Ok(_) => log::info!("copied settings from {}", old.display()),
        Err(e) => log::warn!("copy settings from {}: {e}", old.display()),
    }
}

impl Settings {
    /// Missing or unreadable files give defaults. A file that exists but does
    /// not parse is renamed to `settings.json.corrupt` first, so the next
    /// save does not silently destroy it.
    pub fn load(path: &Path) -> Self {
        let s: Self = match std::fs::read(path) {
            Ok(b) => serde_json::from_slice(&b).unwrap_or_else(|e| {
                log::warn!("{}: {e}; using defaults", path.display());
                let _ = std::fs::rename(path, path.with_extension("json.corrupt"));
                Self::default()
            }),
            Err(_) => Self::default(),
        };
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
    fn migrates_clawed_settings_once() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("clawed").join("settings.json");
        let new = dir.path().join("islet").join("settings.json");
        std::fs::create_dir_all(old.parent().unwrap()).unwrap();
        std::fs::write(&old, r#"{"idle_minutes": 9}"#).unwrap();
        migrate_from_clawed(&new);
        assert_eq!(Settings::load(&new).idle_minutes, 9);
        // islet's own file wins afterwards.
        std::fs::write(&old, r#"{"idle_minutes": 1}"#).unwrap();
        migrate_from_clawed(&new);
        assert_eq!(Settings::load(&new).idle_minutes, 9);
    }

    #[test]
    fn corrupt_file_kept_aside() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("settings.json");
        std::fs::write(&p, r#"{"low_memory": fals"#).unwrap();
        assert_eq!(Settings::load(&p), Settings::default());
        assert_eq!(std::fs::read_to_string(dir.path().join("settings.json.corrupt")).unwrap(), r#"{"low_memory": fals"#);
        // Wrong types are corrupt too, not a crash.
        std::fs::write(&p, r#"{"idle_minutes": -5, "colors": 3}"#).unwrap();
        assert_eq!(Settings::load(&p), Settings::default());
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
