//! Merges islet's hook handlers into Claude Code's `settings.json`.
//!
//! Our handlers are recognised by their command file name (`islet-hook`),
//! so install is idempotent and uninstall never touches anything else.

use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

pub const EVENTS: &[&str] = &[
    "SessionStart",
    "SessionEnd",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "Notification",
    "Stop",
    "StopFailure",
    "SubagentStop",
    "PreCompact",
    "PermissionRequest",
    "TaskCreated",
    "TaskCompleted",
];

pub const APPROVAL_TIMEOUT_SECS: u64 = 65;

pub fn claude_settings_path() -> PathBuf {
    let dir = std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".claude"));
    dir.join("settings.json")
}

fn is_hook_command(cmd: &str) -> bool {
    let cmd = cmd.trim();
    let path = match cmd.strip_prefix(['"', '\'']) {
        Some(rest) => rest.split(['"', '\'']).next().unwrap_or(rest),
        None => cmd.split_whitespace().next().unwrap_or(cmd),
    };
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path).to_ascii_lowercase();
    let stem = name.strip_suffix(".exe").unwrap_or(&name);
    // `clawed-hook` is the name before the rename to islet; recognising it
    // lets Install replace and Uninstall remove entries from those versions.
    stem == "islet-hook" || stem == "clawed-hook"
}

/// True for a hook handler or status line object that points at islet-hook
/// (or the pre-rename clawed-hook).
pub fn is_ours(handler: &Value) -> bool {
    handler.get("command").and_then(Value::as_str).is_some_and(is_hook_command)
}

fn handler(event: &str, hook: &str) -> Value {
    let mut h = json!({ "type": "command", "command": hook, "args": [] });
    if event == "PermissionRequest" {
        h["timeout"] = json!(APPROVAL_TIMEOUT_SECS);
    } else {
        h["async"] = json!(true);
    }
    h
}

/// Shell command for the status line. Claude Code runs it through a shell,
/// so use forward slashes and quote only when needed.
fn statusline_command(hook: &str) -> String {
    let p = hook.replace('\\', "/");
    if p.contains(' ') {
        format!("\"{p}\" statusline")
    } else {
        format!("{p} statusline")
    }
}

pub fn is_installed(settings: &Value) -> bool {
    let Some(hooks) = settings.get("hooks").and_then(Value::as_object) else { return false };
    EVENTS.iter().all(|e| {
        hooks.get(*e).and_then(Value::as_array).is_some_and(|groups| {
            groups.iter().any(|g| {
                g.get("hooks").and_then(Value::as_array).is_some_and(|hs| hs.iter().any(is_ours))
            })
        })
    })
}

/// Adds our handlers. Refuses, instead of overwriting, when `hooks` or one
/// of our events holds something Claude Code would not accept either.
pub fn install(settings: &Value, hook: &str) -> Result<Value, String> {
    let mut out = uninstall(settings);
    let root = ensure_object(&mut out);
    let hooks = root.entry("hooks").or_insert_with(|| json!({}));
    let Some(hooks) = hooks.as_object_mut() else {
        return Err("\"hooks\" in settings.json is not an object; fix it by hand first".into());
    };
    for event in EVENTS {
        let groups = hooks.entry(*event).or_insert_with(|| json!([]));
        let Some(groups) = groups.as_array_mut() else {
            return Err(format!("\"hooks.{event}\" in settings.json is not a list; fix it by hand first"));
        };
        groups.push(json!({ "hooks": [handler(event, hook)] }));
    }
    if !root.contains_key("statusLine") {
        root.insert(
            "statusLine".into(),
            json!({ "type": "command", "command": statusline_command(hook), "padding": 0 }),
        );
    }
    Ok(out)
}

pub fn uninstall(settings: &Value) -> Value {
    let mut out = settings.clone();
    let root = ensure_object(&mut out);
    if root.get("statusLine").is_some_and(is_ours) {
        root.remove("statusLine");
    }
    if let Some(hooks) = root.get_mut("hooks").and_then(Value::as_object_mut) {
        for groups in hooks.values_mut() {
            let Some(arr) = groups.as_array_mut() else { continue };
            for g in arr.iter_mut() {
                if let Some(hs) = g.get_mut("hooks").and_then(Value::as_array_mut) {
                    hs.retain(|h| !is_ours(h));
                }
            }
            arr.retain(|g| g.get("hooks").and_then(Value::as_array).is_none_or(|hs| !hs.is_empty()));
        }
        hooks.retain(|_, groups| groups.as_array().is_none_or(|a| !a.is_empty()));
    }
    out
}

/// `read_settings` only returns objects, so this never replaces content.
fn ensure_object(v: &mut Value) -> &mut Map<String, Value> {
    if !v.is_object() {
        *v = json!({});
    }
    match v {
        Value::Object(m) => m,
        _ => unreachable!("replaced with an object above"),
    }
}

/// Reads Claude Code's settings. Missing or blank is `{}`; invalid JSON or a
/// non-object root is an error, so the file is never overwritten.
pub fn read_settings(path: &Path) -> std::io::Result<Value> {
    let invalid = |e: String| std::io::Error::new(std::io::ErrorKind::InvalidData, e);
    match std::fs::read(path) {
        Ok(b) if b.iter().all(u8::is_ascii_whitespace) => Ok(json!({})),
        Ok(b) => match serde_json::from_slice(&b) {
            Ok(v @ Value::Object(_)) => Ok(v),
            Ok(_) => Err(invalid("settings.json is not a JSON object".into())),
            Err(e) => Err(invalid(format!("settings.json is not valid JSON: {e}"))),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(e) => Err(e),
    }
}

pub fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_default() + "\n"
}

pub fn diff(before: &Value, after: &Value) -> String {
    let (a, b) = (pretty(before), pretty(after));
    similar::TextDiff::from_lines(&a, &b)
        .unified_diff()
        .context_radius(2)
        .header("settings.json", "settings.json (new)")
        .to_string()
}

/// Writes `new` after backing up the current file. Returns the backup path.
///
/// A symlinked settings.json (dotfile managers) is written through to its
/// target. A read-only file is refused rather than replaced. On failure the
/// original is untouched and no temp or backup file is left behind.
pub fn write_with_backup(path: &Path, new: &Value, ts: u64) -> std::io::Result<Option<PathBuf>> {
    let is_link = std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink());
    let path = if is_link { std::fs::canonicalize(path)? } else { path.to_path_buf() };
    let backup = match std::fs::metadata(&path) {
        Ok(m) if m.permissions().readonly() => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!("{} is read-only", path.display()),
            ));
        }
        Ok(_) => {
            let b = path.with_file_name(format!("settings.json.islet-backup-{ts}"));
            std::fs::copy(&path, &b)?;
            Some(b)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            None
        }
        Err(e) => return Err(e),
    };
    let tmp = path.with_extension("json.islet-tmp");
    let written = std::fs::write(&tmp, pretty(new)).and_then(|()| std::fs::rename(&tmp, &path));
    if let Err(e) = written {
        let _ = std::fs::remove_file(&tmp);
        if let Some(b) = &backup {
            let _ = std::fs::remove_file(b);
        }
        return Err(e);
    }
    Ok(backup)
}

/// Copies the hook binary next to the app's data so the registered path
/// stays valid across app updates. Returns the installed path.
pub fn install_hook_binary(src: &Path, data_dir: &Path) -> std::io::Result<PathBuf> {
    let name = if cfg!(windows) { "islet-hook.exe" } else { "islet-hook" };
    let dir = data_dir.join("bin");
    std::fs::create_dir_all(&dir)?;
    let dst = dir.join(name);
    // A running hook (an approval waits up to 60 s) can't be overwritten or
    // deleted on Windows, but it can be renamed: move it aside, then swap.
    let tmp = dir.join(format!("{name}.new"));
    let old = dir.join(format!("{name}.old"));
    std::fs::copy(src, &tmp)?;
    if std::fs::rename(&tmp, &dst).is_err() {
        let _ = std::fs::remove_file(&old);
        let _ = std::fs::rename(&dst, &old);
        if let Err(e) = std::fs::rename(&tmp, &dst) {
            let _ = std::fs::rename(&old, &dst);
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        }
    }
    let _ = std::fs::remove_file(&old);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dst, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(dst)
}

/// Re-copies the hook when an installed copy exists at `dst` and differs from
/// `src`. Returns whether it was replaced.
pub fn refresh_hook_binary(src: &Path, dst: &Path) -> std::io::Result<bool> {
    if !dst.exists() || std::fs::read(src)? == std::fs::read(dst)? {
        return Ok(false);
    }
    let data_dir = dst.parent().and_then(Path::parent).ok_or(std::io::ErrorKind::InvalidInput)?;
    install_hook_binary(src, data_dir)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_only_replaces_stale_installed_hook() {
        let dir = tempfile::tempdir().unwrap();
        let name = if cfg!(windows) { "islet-hook.exe" } else { "islet-hook" };
        let src = dir.path().join("bundled");
        let dst = dir.path().join("data").join("bin").join(name);
        std::fs::write(&src, b"v2").unwrap();
        // Never installed: nothing is created.
        assert!(!refresh_hook_binary(&src, &dst).unwrap());
        assert!(!dst.exists());
        // Stale copy from an older version: replaced.
        std::fs::create_dir_all(dst.parent().unwrap()).unwrap();
        std::fs::write(&dst, b"v1").unwrap();
        assert!(refresh_hook_binary(&src, &dst).unwrap());
        assert_eq!(std::fs::read(&dst).unwrap(), b"v2");
        // Already current: left alone.
        assert!(!refresh_hook_binary(&src, &dst).unwrap());
    }

    const HOOK: &str = "C:\\Users\\me\\AppData\\Local\\islet\\bin\\islet-hook.exe";

    fn count_ours(v: &Value) -> usize {
        v["hooks"]
            .as_object()
            .map(|h| {
                h.values()
                    .flat_map(|g| g.as_array().unwrap())
                    .flat_map(|g| g["hooks"].as_array().unwrap())
                    .filter(|h| is_ours(h))
                    .count()
            })
            .unwrap_or(0)
    }

    #[test]
    fn install_into_empty() {
        let out = install(&json!({}), HOOK).unwrap();
        assert!(is_installed(&out));
        assert_eq!(count_ours(&out), EVENTS.len());
        assert_eq!(out["statusLine"]["command"], "C:/Users/me/AppData/Local/islet/bin/islet-hook.exe statusline");
    }

    #[test]
    fn install_preserves_other_hooks() {
        let before = json!({
            "model": "opus",
            "hooks": { "PreToolUse": [ { "matcher": "Bash", "hooks": [ { "type": "command", "command": "rtk hook" } ] } ] }
        });
        let out = install(&before, HOOK).unwrap();
        let pre = out["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(pre.len(), 2);
        assert_eq!(pre[0]["hooks"][0]["command"], "rtk hook");
        assert_eq!(out["model"], "opus");
    }

    #[test]
    fn install_idempotent() {
        let once = install(&json!({}), HOOK).unwrap();
        let twice = install(&once, HOOK).unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn uninstall_removes_only_ours() {
        let before = json!({
            "hooks": { "PreToolUse": [ { "matcher": "Bash", "hooks": [ { "type": "command", "command": "rtk hook" } ] } ] }
        });
        let out = uninstall(&install(&before, HOOK).unwrap());
        assert_eq!(out, before);
    }

    #[test]
    fn uninstall_mixed_group_keeps_foreign_handler() {
        let before = json!({ "hooks": { "Stop": [ { "hooks": [
            { "type": "command", "command": "notify.sh" },
            { "type": "command", "command": HOOK }
        ] } ] } });
        let out = uninstall(&before);
        assert_eq!(out["hooks"]["Stop"][0]["hooks"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn statusline_kept_when_foreign() {
        let before = json!({ "statusLine": { "type": "command", "command": "~/bin/my-status" } });
        let out = install(&before, HOOK).unwrap();
        assert_eq!(out["statusLine"]["command"], "~/bin/my-status");
        assert_eq!(uninstall(&out)["statusLine"]["command"], "~/bin/my-status");
    }

    #[test]
    fn uninstall_removes_our_statusline_only() {
        let out = uninstall(&install(&json!({}), HOOK).unwrap());
        assert!(out.get("statusLine").is_none());
    }

    #[test]
    fn statusline_quoted_with_spaces() {
        let out = install(&json!({}), "/Users/a b/islet-hook").unwrap();
        assert_eq!(out["statusLine"]["command"], "\"/Users/a b/islet-hook\" statusline");
        assert!(is_ours(&out["statusLine"]));
    }

    #[test]
    fn permission_request_has_timeout_65_and_not_async() {
        let out = install(&json!({}), HOOK).unwrap();
        let h = &out["hooks"]["PermissionRequest"][0]["hooks"][0];
        assert_eq!(h["timeout"], 65);
        assert!(h.get("async").is_none());
        assert_eq!(out["hooks"]["PreToolUse"][0]["hooks"][0]["async"], true);
        assert!(out["hooks"]["PreToolUse"][0].get("matcher").is_none());
    }

    #[test]
    fn recognises_our_commands() {
        assert!(is_hook_command("/usr/local/bin/islet-hook"));
        assert!(is_hook_command("C:\\x\\ISLET-HOOK.EXE"));
        assert!(is_hook_command("\"C:/a b/islet-hook.exe\" statusline"));
        assert!(is_hook_command("/opt/old/clawed-hook statusline"));
        assert!(is_hook_command("C:/old/CLAWED-HOOK.EXE"));
        assert!(!is_hook_command("islet-hooker"));
        assert!(!is_hook_command("rtk hook"));
    }

    #[test]
    fn backup_written() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("settings.json");
        std::fs::write(&p, "{\"a\": 1}").unwrap();
        let new = install(&read_settings(&p).unwrap(), HOOK).unwrap();
        let backup = write_with_backup(&p, &new, 123).unwrap().unwrap();
        assert_eq!(std::fs::read_to_string(backup).unwrap(), "{\"a\": 1}");
        assert!(is_installed(&read_settings(&p).unwrap()));
    }

    #[test]
    fn missing_or_empty_settings_is_empty_object() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_settings(&dir.path().join("nope.json")).unwrap(), json!({}));
        let p = dir.path().join("empty.json");
        std::fs::write(&p, "  \n").unwrap();
        assert_eq!(read_settings(&p).unwrap(), json!({}));
    }

    #[test]
    fn diff_shows_changes() {
        let d = diff(&json!({}), &install(&json!({}), HOOK).unwrap());
        assert!(d.contains("+") && d.contains("PermissionRequest"));
    }
    #[test]
    fn invalid_or_non_object_settings_are_errors() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("settings.json");
        for bad in ["{\"hooks\": {", "[1, 2]", "\"text\"", "null"] {
            std::fs::write(&p, bad).unwrap();
            let e = read_settings(&p).unwrap_err();
            assert_eq!(e.kind(), std::io::ErrorKind::InvalidData, "{bad}");
        }
    }

    #[test]
    fn install_refuses_malformed_hooks_instead_of_overwriting() {
        assert!(install(&json!({ "hooks": [] }), HOOK).is_err());
        assert!(install(&json!({ "hooks": { "Stop": "x" } }), HOOK).is_err());
        // Uninstall leaves values it does not understand alone.
        let odd = json!({ "hooks": { "Stop": "x", "Other": 3 } });
        assert_eq!(uninstall(&odd), odd);
    }

    #[test]
    fn read_only_settings_left_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("settings.json");
        std::fs::write(&p, "{\"a\": 1}").unwrap();
        let mut perms = std::fs::metadata(&p).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&p, perms.clone()).unwrap();
        let new = install(&read_settings(&p).unwrap(), HOOK).unwrap();
        let e = write_with_backup(&p, &new, 1).unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::PermissionDenied);
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "{\"a\": 1}");
        let names: Vec<_> = std::fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names.len(), 1, "left behind: {names:?}");
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        std::fs::set_permissions(&p, perms).unwrap();
    }

    #[test]
    fn missing_settings_dir_is_created() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("no").join("such").join("settings.json");
        let new = install(&read_settings(&p).unwrap(), HOOK).unwrap();
        assert_eq!(write_with_backup(&p, &new, 1).unwrap(), None);
        assert!(is_installed(&read_settings(&p).unwrap()));
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_settings_written_through() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("dotfiles-settings.json");
        let link = dir.path().join("settings.json");
        std::fs::write(&real, "{}").unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let new = install(&read_settings(&link).unwrap(), HOOK).unwrap();
        write_with_backup(&link, &new, 1).unwrap();
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert!(is_installed(&read_settings(&real).unwrap()));
    }
}
