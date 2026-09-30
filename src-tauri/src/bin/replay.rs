//! Replays fixture payloads into a running clawed app.
//!
//! `replay <fixtures.jsonl> [--speed N] [--hold SECS]`
//!
//! Sessions use this process as their parent pid, so they go stale shortly
//! after the replay exits. `--hold` keeps the process alive at the end.

use std::io::{BufRead, BufReader, Write};
use std::time::Duration;

use clawed_proto::{now_ms, pipe, strip_event, strip_status, AppMsg, HookMsg};
use interprocess::local_socket::{prelude::*, Stream};
use serde_json::Value;

fn send(msg: &HookMsg) -> std::io::Result<Stream> {
    let mut conn = Stream::connect(pipe::name()?)?;
    let mut line = serde_json::to_vec(msg)?;
    line.push(b'\n');
    conn.write_all(&line)?;
    Ok(conn)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.iter().find(|a| !a.starts_with("--") && a.parse::<f64>().is_err()) else {
        eprintln!("usage: replay <fixtures.jsonl> [--speed N] [--hold SECS]");
        std::process::exit(2);
    };
    let flag = |name: &str| {
        args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).and_then(|v| v.parse::<f64>().ok())
    };
    let speed = flag("--speed").unwrap_or(1.0).max(0.01);
    let hold = flag("--hold").unwrap_or(0.0);

    let text = std::fs::read_to_string(path).expect("read fixtures");
    let ppid = std::process::id();
    let mut prev_ts: Option<u64> = None;
    let mut waiters = Vec::new();

    for (i, line) in text.lines().enumerate().filter(|(_, l)| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).expect("fixture line is JSON");
        let raw = v.get("payload").unwrap_or(&v);
        if let (Some(ts), Some(prev)) = (v.get("ts").and_then(Value::as_u64), prev_ts) {
            std::thread::sleep(Duration::from_millis((ts.saturating_sub(prev) as f64 / speed) as u64));
        }
        prev_ts = v.get("ts").and_then(Value::as_u64).or(prev_ts);

        let now = now_ms();
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
                eprintln!("cannot reach clawed ({e}); is the app running?");
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
