//! Wire types shared by `clawed-hook` and the clawed backend.
//!
//! Messages are newline-delimited JSON over a per-user local socket
//! (named pipe on Windows, Unix socket elsewhere).

use serde::{Deserialize, Serialize};

mod strip;
#[cfg(feature = "ipc")]
pub mod pipe;

pub use strip::{strip_event, strip_status, truncate};

pub const PROTO_VERSION: u32 = 1;

/// Hook binary to backend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum HookMsg {
    Event(Event),
    Approval { id: String, event: Event },
    Status(StatusLine),
}

/// Backend to hook binary, only sent on approval connections.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum AppMsg {
    Decision { id: String, behavior: Behavior },
    /// No decision: the hook exits silently and Claude Code shows its own prompt.
    Release { id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Behavior {
    Allow,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub v: u32,
    pub session_id: String,
    pub cwd: String,
    pub transcript_path: Option<String>,
    /// Claude Code `hook_event_name`, e.g. `PreToolUse`.
    pub kind: String,
    pub tool_name: Option<String>,
    /// File path, truncated command, pattern or URL. Never file contents.
    pub tool_summary: Option<String>,
    pub todos: Option<Vec<Todo>>,
    pub task: Option<TaskDelta>,
    pub notification_type: Option<String>,
    pub message: Option<String>,
    /// Present when the event fired inside a subagent.
    pub agent_id: Option<String>,
    pub ppid: u32,
    /// Milliseconds since the Unix epoch.
    pub ts: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Todo {
    pub content: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskDelta {
    pub id: Option<String>,
    pub subject: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatusLine {
    pub v: u32,
    pub session_id: String,
    pub context_used_pct: Option<f64>,
    pub context_window_size: Option<u64>,
    pub five_hour: Option<Window>,
    pub seven_day: Option<Window>,
    pub model: Option<String>,
    pub ppid: u32,
    pub ts: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Window {
    pub used_pct: f64,
    /// Unix epoch seconds.
    pub resets_at: Option<u64>,
}

/// Socket name for the current user. `CLAWED_SOCKET` overrides it (tests, dev).
pub fn socket_name() -> String {
    if let Ok(name) = std::env::var("CLAWED_SOCKET") {
        if !name.is_empty() {
            return name;
        }
    }
    let user = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "user".into());
    let user: String = user
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    format!("clawed-{user}")
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_hookmsg() {
        let ev = Event {
            v: PROTO_VERSION,
            session_id: "s".into(),
            cwd: "/x".into(),
            transcript_path: None,
            kind: "Stop".into(),
            tool_name: None,
            tool_summary: None,
            todos: None,
            task: None,
            notification_type: None,
            message: None,
            agent_id: None,
            ppid: 1,
            ts: 2,
        };
        let msg = HookMsg::Approval { id: "a".into(), event: ev };
        let s = serde_json::to_string(&msg).unwrap();
        assert!(s.contains("\"t\":\"approval\""));
        assert_eq!(serde_json::from_str::<HookMsg>(&s).unwrap(), msg);

        let d = AppMsg::Decision { id: "a".into(), behavior: Behavior::Deny };
        let s = serde_json::to_string(&d).unwrap();
        assert!(s.contains("\"behavior\":\"deny\""));
        assert_eq!(serde_json::from_str::<AppMsg>(&s).unwrap(), d);
    }
}
