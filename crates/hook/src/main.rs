//! `clawed-hook`: run by Claude Code on hook events and as the status line.
//!
//! Guarantee: if the app is not running or anything goes wrong, exit 0 with
//! no output, so Claude Code carries on exactly as if the hook were absent.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::time::Duration;

use clawed_proto::{
    now_ms, peer, pipe, strip_event, strip_status, AppMsg, Behavior, HookMsg, StatusLine,
};
use interprocess::local_socket::{prelude::*, Stream};
use serde_json::Value;

mod ppid;

const STDIN_MAX: u64 = 4 * 1024 * 1024;
const EVENT_DEADLINE: Duration = Duration::from_secs(3);
const APPROVAL_TIMEOUT: Duration = Duration::from_secs(60);

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let statusline = args.first().map(String::as_str) == Some("statusline");
    let capture = args
        .iter()
        .position(|a| a == "--capture")
        .and_then(|i| args.get(i + 1).cloned());

    let raw = match read_stdin() {
        Some(v) => v,
        None => return,
    };
    if let Some(path) = capture {
        let _ = append_capture(&path, &raw);
    }

    if statusline {
        run_statusline(&raw);
    } else {
        run_event(&raw);
    }
}

fn read_stdin() -> Option<Value> {
    let mut buf = Vec::new();
    io::stdin().take(STDIN_MAX).read_to_end(&mut buf).ok()?;
    serde_json::from_slice(&buf).ok()
}

fn append_capture(path: &str, raw: &Value) -> io::Result<()> {
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    let line = serde_json::json!({ "ts": now_ms(), "payload": raw });
    writeln!(f, "{line}")
}

/// Exits the process after `after`, whatever the main thread is doing.
fn watchdog(after: Duration) {
    std::thread::spawn(move || {
        std::thread::sleep(after);
        std::process::exit(0);
    });
}

/// Connects to the app. `None` if the server runs as another user (someone
/// squatting the socket name), so events and approvals never reach it.
fn connect() -> Option<Stream> {
    let name = pipe::name().ok()?;
    let conn = Stream::connect(name).ok()?;
    let creds = conn.peer_creds().ok()?;
    peer::is_same_user(&creds).then_some(conn)
}

fn send(conn: &mut Stream, msg: &HookMsg) -> Option<()> {
    let mut line = serde_json::to_vec(msg).ok()?;
    line.push(b'\n');
    conn.write_all(&line).ok()?;
    conn.flush().ok()
}

fn run_event(raw: &Value) {
    let ts = now_ms();
    let Some(event) = strip_event(raw, ppid::parent_pid(), ts) else { return };

    if event.kind != "PermissionRequest" {
        watchdog(EVENT_DEADLINE);
        if let Some(mut conn) = connect() {
            let _ = send(&mut conn, &HookMsg::Event(event));
        }
        return;
    }

    watchdog(approval_timeout());
    let Some(mut conn) = connect() else { return };
    let id = format!("{}-{ts}", std::process::id());
    if send(&mut conn, &HookMsg::Approval { id: id.clone(), event }).is_none() {
        return;
    }
    let mut line = String::new();
    if BufReader::new(&mut conn).read_line(&mut line).unwrap_or(0) == 0 {
        return;
    }
    if let Ok(AppMsg::Decision { id: got, behavior }) = serde_json::from_str(&line) {
        if got == id {
            print_decision(behavior);
        }
    }
}

fn approval_timeout() -> Duration {
    std::env::var("CLAWED_APPROVAL_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .map(Duration::from_millis)
        .unwrap_or(APPROVAL_TIMEOUT)
}

fn print_decision(behavior: Behavior) {
    let decision = match behavior {
        Behavior::Allow => serde_json::json!({ "behavior": "allow" }),
        Behavior::Deny => serde_json::json!({ "behavior": "deny", "message": "Denied from clawed" }),
    };
    let out = serde_json::json!({
        "hookSpecificOutput": { "hookEventName": "PermissionRequest", "decision": decision }
    });
    let mut stdout = io::stdout().lock();
    let _ = writeln!(stdout, "{out}");
    let _ = stdout.flush();
}

fn run_statusline(raw: &Value) {
    watchdog(EVENT_DEADLINE);
    let Some(status) = strip_status(raw, ppid::parent_pid(), now_ms()) else { return };
    let line = status_text(&status);
    if let Some(mut conn) = connect() {
        let _ = send(&mut conn, &HookMsg::Status(status));
    }
    let mut stdout = io::stdout().lock();
    let _ = writeln!(stdout, "{line}");
    let _ = stdout.flush();
}

fn status_text(s: &StatusLine) -> String {
    let mut parts = Vec::new();
    if let Some(m) = &s.model {
        parts.push(m.clone());
    }
    if let Some(c) = s.context_used_pct {
        parts.push(format!("ctx {c:.0}%"));
    }
    if let Some(w) = &s.five_hour {
        parts.push(format!("5h {:.0}%", w.used_pct));
    }
    if let Some(w) = &s.seven_day {
        parts.push(format!("7d {:.0}%", w.used_pct));
    }
    parts.join(" · ")
}
