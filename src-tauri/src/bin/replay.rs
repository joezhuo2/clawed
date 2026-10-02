//! Replays fixture payloads into a running islet app.
//!
//! `replay <fixtures.jsonl> [--speed N] [--hold SECS] [--fresh-ids]`
//!
//! Sessions use this process as their parent pid, so they go stale shortly
//! after the replay exits. `--hold` keeps the process alive at the end.
//! `--fresh-ids` appends this process id to every session id, so repeated
//! runs (the soak test) create new sessions instead of reusing old ones.

use std::io::{BufRead, BufReader, Write};
use std::time::Duration;

use islet_proto::{now_ms, pipe, strip_event, strip_status, AppMsg, HookMsg};
use interprocess::local_socket::{prelude::*, Stream};
use serde_json::Value;

fn send(msg: &HookMsg) -> std::io::Result<Stream> {
    let mut conn = Stream::connect(pipe::name()?)?;
    let mut line = serde_json::to_vec(msg)?;
    line.push(b'\n');
    conn.write_all(&line)?;
    Ok(conn)
}

/// Moves rate limit reset times so they stay in the future relative to the
/// replay, as they were when the fixture was recorded.
fn shift_resets(raw: &Value, recorded_ms: Option<u64>, now_ms: u64) -> Value {
    let mut out = raw.clone();
    let Some(recorded) = recorded_ms else { return out };
    let delta = (now_ms / 1000) as i64 - (recorded / 1000) as i64;
    if let Some(limits) = out.get_mut("rate_limits").and_then(Value::as_object_mut) {
        for w in limits.values_mut() {
            if let Some(r) = w.get("resets_at").and_then(Value::as_i64) {
                w["resets_at"] = Value::from(r + delta);
            }
        }
    }
    out
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.iter().find(|a| !a.starts_with("--") && a.parse::<f64>().is_err()) else {
        eprintln!("usage: replay <fixtures.jsonl> [--speed N] [--hold SECS] [--fresh-ids]");
        std::process::exit(2);
    };
    let flag = |name: &str| {
        args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).and_then(|v| v.parse::<f64>().ok())
    };
    let speed = flag("--speed").unwrap_or(1.0).max(0.01);
    let hold = flag("--hold").unwrap_or(0.0);
    let fresh_ids = args.iter().any(|a| a == "--fresh-ids");

    let text = std::fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("replay: cannot read {path}: {e}");
        std::process::exit(1);
    });
    let ppid = std::process::id();
    let mut prev_ts: Option<u64> = None;
    let mut waiters = Vec::new();

    for (i, line) in text.lines().enumerate().filter(|(_, l)| !l.trim().is_empty()) {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            eprintln!("replay: line {} is not JSON, skipped", i + 1);
            continue;
        };
        let raw = v.get("payload").unwrap_or(&v);
        if let (Some(ts), Some(prev)) = (v.get("ts").and_then(Value::as_u64), prev_ts) {
            std::thread::sleep(Duration::from_millis((ts.saturating_sub(prev) as f64 / speed) as u64));
        }
        prev_ts = v.get("ts").and_then(Value::as_u64).or(prev_ts);

        let now = now_ms();
        let mut shifted = shift_resets(raw, v.get("ts").and_then(Value::as_u64), now);
        if fresh_ids {
            if let Some(id) = shifted.get("session_id").and_then(Value::as_str) {
                shifted["session_id"] = Value::from(format!("{id}-{ppid}"));
            }
        }
        let raw = &shifted;
        let msg = if let Some(ev) = strip_event(raw, ppid, now) {
            if ev.kind == "PermissionRequest" {
                HookMsg::Approval { id: format!("replay-{i}"), event: ev }
            } else {
                HookMsg::Event(ev)
            }
        } else if let Some(st) = strip_status(raw, ppid, now) {
            HookMsg::Status(st)
        } else {
            eprintln!("line {}: skipped", i + 1);
            continue;
        };
        let label = match &msg {
            HookMsg::Event(e) => e.kind.clone(),
            HookMsg::Approval { .. } => "PermissionRequest (waiting)".into(),
            HookMsg::Status(_) => "status line".into(),
        };
        match send(&msg) {
            Ok(conn) => {
                println!("line {:>3}: {label}", i + 1);
                if let HookMsg::Approval { id, .. } = msg {
                    waiters.push(std::thread::spawn(move || {
                        let mut reply = String::new();
                        let _ = BufReader::new(conn).read_line(&mut reply);
                        match serde_json::from_str::<AppMsg>(&reply) {
                            Ok(AppMsg::Decision { behavior, .. }) => println!("{id}: {behavior:?}"),
                            _ => println!("{id}: released"),
                        }
                    }));
                }
            }
            Err(e) => {
                eprintln!("cannot reach islet ({e}); is the app running?");
                std::process::exit(1);
            }
        }
    }
    for w in waiters {
        let _ = w.join();
    }
    if hold > 0.0 {
        std::thread::sleep(Duration::from_secs_f64(hold));
    }
}
