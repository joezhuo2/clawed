use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use clawed_proto::{AppMsg, Behavior, HookMsg};
use interprocess::local_socket::{prelude::*, ListenerOptions, Name};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Unique socket name for one test, as passed via `CLAWED_SOCKET`.
fn unique_socket() -> String {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let base = format!("clawed-test-{}-{n}", std::process::id());
    if cfg!(windows) {
        base
    } else {
        std::env::temp_dir().join(format!("{base}.sock")).to_string_lossy().into_owned()
    }
}

fn to_name(s: &str) -> Name<'static> {
    #[cfg(windows)]
    {
        s.to_string().to_ns_name::<interprocess::local_socket::GenericNamespaced>().unwrap()
    }
    #[cfg(not(windows))]
    {
        s.to_string().to_fs_name::<interprocess::local_socket::GenericFilePath>().unwrap()
    }
}

fn run_hook(socket: &str, args: &[&str], stdin: &str, extra_env: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_clawed-hook"));
    cmd.args(args)
        .env("CLAWED_SOCKET", socket)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().unwrap();
    child.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
    child.wait_with_output().unwrap()
}

/// Fake backend: accepts one connection, sends the received message on
/// `tx`, then answers with `reply(msg)` if it returns Some.
fn fake_server(
    socket: &str,
    reply: impl Fn(&HookMsg) -> Option<AppMsg> + Send + 'static,
) -> mpsc::Receiver<HookMsg> {
    let listener = ListenerOptions::new().name(to_name(socket)).create_sync().unwrap();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let conn = listener.incoming().next().unwrap().unwrap();
        let mut reader = BufReader::new(conn);
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let msg: HookMsg = serde_json::from_str(&line).unwrap();
        let answer = reply(&msg);
        tx.send(msg).unwrap();
        if let Some(a) = answer {
            let mut out = serde_json::to_vec(&a).unwrap();
            out.push(b'\n');
            reader.get_mut().write_all(&out).unwrap();
            reader.get_mut().flush().unwrap();
        }
        // Keep the connection open briefly so the hook reads the reply.
        thread::sleep(Duration::from_millis(500));
    });
    rx
}

const PRE_TOOL: &str = r#"{"session_id":"s1","cwd":"C:\\p","transcript_path":"t","hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":"ls"}}"#;
const PERM: &str = r#"{"session_id":"s1","cwd":"C:\\p","hook_event_name":"PermissionRequest","tool_name":"Bash","tool_input":{"command":"rm -rf x"}}"#;

fn approval_id(msg: &HookMsg) -> String {
    match msg {
        HookMsg::Approval { id, .. } => id.clone(),
        other => panic!("expected approval, got {other:?}"),
    }
}

#[test]
fn app_absent_exits_zero_fast() {
    let socket = unique_socket();
    let start = Instant::now();
    let out = run_hook(&socket, &[], PERM, &[]);
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    assert!(start.elapsed() < Duration::from_millis(1500), "took {:?}", start.elapsed());
}

#[test]
fn malformed_stdin_exits_zero() {
    let out = run_hook(&unique_socket(), &[], "not json{", &[]);
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
}

#[test]
fn event_forwarded() {
    let socket = unique_socket();
    let rx = fake_server(&socket, |_| None);
    let out = run_hook(&socket, &[], PRE_TOOL, &[]);
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    match rx.recv_timeout(Duration::from_secs(5)).unwrap() {
        HookMsg::Event(ev) => {
            assert_eq!(ev.kind, "PreToolUse");
            assert_eq!(ev.tool_summary.as_deref(), Some("ls"));
            assert!(ev.ppid > 0);
        }
        other => panic!("unexpected {other:?}"),
    }
}

fn decide(behavior: Behavior) -> impl Fn(&HookMsg) -> Option<AppMsg> {
    move |m| Some(AppMsg::Decision { id: approval_id(m), behavior })
}

#[test]
fn approval_allow_prints_decision() {
    let socket = unique_socket();
    let _rx = fake_server(&socket, decide(Behavior::Allow));
    let out = run_hook(&socket, &[], PERM, &[]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["hookSpecificOutput"]["hookEventName"], "PermissionRequest");
    assert_eq!(v["hookSpecificOutput"]["decision"]["behavior"], "allow");
}

#[test]
fn approval_deny_prints_decision() {
    let socket = unique_socket();
    let _rx = fake_server(&socket, decide(Behavior::Deny));
    let out = run_hook(&socket, &[], PERM, &[]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["hookSpecificOutput"]["decision"]["behavior"], "deny");
    assert!(v["hookSpecificOutput"]["decision"]["message"].is_string());
}

#[test]
fn approval_timeout_prints_nothing() {
    let socket = unique_socket();
    let _rx = fake_server(&socket, |_| None);
    let start = Instant::now();
    let out = run_hook(&socket, &[], PERM, &[("CLAWED_APPROVAL_TIMEOUT_MS", "300")]);
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    assert!(start.elapsed() < Duration::from_secs(3));
}

#[test]
fn approval_release_prints_nothing() {
    let socket = unique_socket();
    let _rx = fake_server(&socket, |m| Some(AppMsg::Release { id: approval_id(m) }));
    let out = run_hook(&socket, &[], PERM, &[]);
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
}

#[test]
fn approval_wrong_id_prints_nothing() {
    let socket = unique_socket();
    let _rx = fake_server(&socket, |_| {
        Some(AppMsg::Decision { id: "other".into(), behavior: Behavior::Allow })
    });
    let out = run_hook(&socket, &[], PERM, &[]);
    assert!(out.stdout.is_empty());
}

#[test]
fn statusline_prints_line_without_app() {
    let raw = r#"{"session_id":"s1","model":{"display_name":"Opus"},"context_window":{"used_percentage":42},"rate_limits":{"five_hour":{"used_percentage":31},"seven_day":{"used_percentage":12}}}"#;
    let out = run_hook(&unique_socket(), &["statusline"], raw, &[]);
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "Opus · ctx 42% · 5h 31% · 7d 12%");
}

#[test]
fn statusline_forwarded() {
    let socket = unique_socket();
    let rx = fake_server(&socket, |_| None);
    let raw = r#"{"session_id":"s1","rate_limits":{"five_hour":{"used_percentage":31,"resets_at":100}}}"#;
    run_hook(&socket, &["statusline"], raw, &[]);
    match rx.recv_timeout(Duration::from_secs(5)).unwrap() {
        HookMsg::Status(s) => assert_eq!(s.five_hour.unwrap().resets_at, Some(100)),
        other => panic!("unexpected {other:?}"),
    }
}
