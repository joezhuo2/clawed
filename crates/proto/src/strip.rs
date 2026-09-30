//! Turns raw Claude Code hook JSON into small wire messages.
//!
//! Only fields listed here are forwarded. File contents from Write/Edit
//! never leave the hook process.

use serde_json::Value;

use crate::{Event, StatusLine, TaskDelta, Todo, Window, PROTO_VERSION};

const SUMMARY_MAX: usize = 200;
const TODO_MAX: usize = 120;
const PROMPT_MAX: usize = 120;
const TODOS_MAX: usize = 100;

/// Cuts `s` to at most `max` chars, ending with `…` when cut.
pub fn truncate(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn str_field<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

fn first_str(v: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|k| match v.get(*k) {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Number(n)) => Some(n.to_string()),
        _ => None,
    })
}

fn tool_summary(tool: &str, input: &Value) -> Option<String> {
    let pick = |keys: &[&str]| first_str(input, keys).map(|s| truncate(&s, SUMMARY_MAX));
    match tool {
        "Bash" | "PowerShell" => pick(&["command"]).map(|s| s.replace(['\r', '\n'], " ")),
        "Write" | "Edit" | "MultiEdit" | "Read" | "NotebookEdit" => {
            pick(&["file_path", "notebook_path", "path"])
        }
        "Grep" | "Glob" => pick(&["pattern"]),
        "WebFetch" => pick(&["url"]),
        "WebSearch" => pick(&["query"]),
        "Agent" | "Task" => pick(&["description"]),
        "TaskCreate" | "TaskUpdate" => pick(&["subject"]),
        // MCP and unknown tools: only a path-like field if one exists.
        _ => pick(&["file_path", "path", "url"]),
    }
}

fn todos(input: &Value) -> Option<Vec<Todo>> {
    let arr = input.get("todos")?.as_array()?;
    Some(
        arr.iter()
            .take(TODOS_MAX)
            .filter_map(|t| {
                let content = str_field(t, "content").or_else(|| str_field(t, "subject"))?;
                Some(Todo {
                    content: truncate(content, TODO_MAX),
                    status: str_field(t, "status").unwrap_or("pending").to_string(),
                })
            })
            .collect(),
    )
}

fn task_delta(kind: &str, tool: Option<&str>, raw: &Value) -> Option<TaskDelta> {
    let (src, default_status) = match (kind, tool) {
        ("TaskCreated", _) => (raw, Some("pending")),
        ("TaskCompleted", _) => (raw, Some("completed")),
        ("PostToolUse", Some("TaskCreate")) => (raw.get("tool_input")?, Some("pending")),
        ("PostToolUse", Some("TaskUpdate")) => (raw.get("tool_input")?, None),
        _ => return None,
    };
    let mut id = first_str(src, &["task_id", "taskId", "id"]);
    if id.is_none() && tool == Some("TaskCreate") {
        // The new task's id is only in the tool response.
        id = raw
            .get("tool_response")
            .and_then(|r| first_str(r, &["task_id", "taskId", "id"]).or_else(|| {
                r.get("task").and_then(|t| first_str(t, &["id", "taskId"]))
            }));
    }
    let subject = first_str(src, &["task_subject", "subject"]).map(|s| truncate(&s, TODO_MAX));
    let status = str_field(src, "status")
        .map(str::to_string)
        .or(default_status.map(str::to_string));
    if id.is_none() && subject.is_none() {
        return None;
    }
    Some(TaskDelta { id, subject, status })
}

/// Builds a wire `Event` from a raw hook payload. `None` when required
/// fields are missing.
pub fn strip_event(raw: &Value, ppid: u32, ts: u64) -> Option<Event> {
    let session_id = str_field(raw, "session_id")?.to_string();
    let kind = str_field(raw, "hook_event_name")?.to_string();
    let tool_name = str_field(raw, "tool_name").map(str::to_string);
    let null = Value::Null;
    let input = raw.get("tool_input").unwrap_or(&null);

    let tool_summary = tool_name.as_deref().and_then(|t| tool_summary(t, input));
    let todos = match (kind.as_str(), tool_name.as_deref()) {
        ("PostToolUse", Some("TodoWrite")) => todos(input),
        _ => None,
    };
    let task = task_delta(&kind, tool_name.as_deref(), raw);
    let message = match kind.as_str() {
        "UserPromptSubmit" => str_field(raw, "prompt").map(|p| truncate(p, PROMPT_MAX)),
        _ => str_field(raw, "message").map(|m| truncate(m, SUMMARY_MAX)),
    };

    Some(Event {
        v: PROTO_VERSION,
        session_id,
        cwd: str_field(raw, "cwd").unwrap_or_default().to_string(),
        transcript_path: str_field(raw, "transcript_path").map(str::to_string),
        kind,
        tool_name,
        tool_summary,
        todos,
        task,
        notification_type: str_field(raw, "notification_type").map(str::to_string),
        message,
        agent_id: str_field(raw, "agent_id").map(str::to_string),
        ppid,
        ts,
    })
}

fn window(v: Option<&Value>) -> Option<Window> {
    let v = v?;
    Some(Window {
        used_pct: v.get("used_percentage")?.as_f64()?,
        resets_at: v.get("resets_at").and_then(Value::as_u64),
    })
}

/// Builds a wire `StatusLine` from Claude Code's status line JSON.
pub fn strip_status(raw: &Value, ppid: u32, ts: u64) -> Option<StatusLine> {
    let session_id = str_field(raw, "session_id")?.to_string();
    let ctx = raw.get("context_window");
    let limits = raw.get("rate_limits");
    Some(StatusLine {
        v: PROTO_VERSION,
        session_id,
        context_used_pct: ctx.and_then(|c| c.get("used_percentage")).and_then(Value::as_f64),
        context_window_size: ctx
            .and_then(|c| c.get("context_window_size"))
            .and_then(Value::as_u64),
        five_hour: window(limits.and_then(|l| l.get("five_hour"))),
        seven_day: window(limits.and_then(|l| l.get("seven_day"))),
        model: raw
            .get("model")
            .and_then(|m| str_field(m, "display_name").or_else(|| str_field(m, "id")))
            .map(str::to_string),
        ppid,
        ts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn base(kind: &str) -> Value {
        json!({
            "session_id": "abc",
            "cwd": "C:\\code\\proj",
            "transcript_path": "C:\\t.jsonl",
            "hook_event_name": kind,
        })
    }

    fn with(mut v: Value, extra: Value) -> Value {
        for (k, val) in extra.as_object().unwrap() {
            v[k] = val.clone();
        }
        v
    }

    #[test]
    fn write_content_never_forwarded() {
        let big = "x".repeat(1_000_000);
        let raw = with(
            base("PreToolUse"),
            json!({"tool_name": "Write", "tool_input": {"file_path": "C:\\a.rs", "content": big}}),
        );
        let ev = strip_event(&raw, 1, 2).unwrap();
        let s = serde_json::to_string(&ev).unwrap();
        assert!(s.len() < 1024, "serialized len {}", s.len());
        assert_eq!(ev.tool_summary.as_deref(), Some("C:\\a.rs"));
    }

    #[test]
    fn edit_strings_dropped() {
        let raw = with(
            base("PostToolUse"),
            json!({"tool_name": "Edit", "tool_input": {
                "file_path": "/a.rs", "old_string": "SECRET_OLD", "new_string": "SECRET_NEW"
            }}),
        );
        let s = serde_json::to_string(&strip_event(&raw, 1, 2).unwrap()).unwrap();
        assert!(!s.contains("SECRET_OLD") && !s.contains("SECRET_NEW"));
    }

    #[test]
    fn bash_command_truncated() {
        let cmd = "a".repeat(500);
        let raw = with(
            base("PermissionRequest"),
            json!({"tool_name": "Bash", "tool_input": {"command": cmd}}),
        );
        let sum = strip_event(&raw, 1, 2).unwrap().tool_summary.unwrap();
        assert_eq!(sum.chars().count(), 200);
        assert!(sum.ends_with('…'));
    }

    #[test]
    fn todowrite_parsed() {
        let raw = with(
            base("PostToolUse"),
            json!({"tool_name": "TodoWrite", "tool_input": {"todos": [
                {"content": "one", "status": "completed", "activeForm": "Doing one"},
                {"content": "two", "status": "in_progress"},
                {"content": "three", "status": "pending"}
            ]}}),
        );
        let todos = strip_event(&raw, 1, 2).unwrap().todos.unwrap();
        assert_eq!(todos.len(), 3);
        assert_eq!(todos[0], Todo { content: "one".into(), status: "completed".into() });
        assert_eq!(todos[1].status, "in_progress");
    }

    #[test]
    fn task_update_parsed() {
        let raw = with(
            base("PostToolUse"),
            json!({"tool_name": "TaskUpdate", "tool_input": {"taskId": "3", "status": "completed"}}),
        );
        let t = strip_event(&raw, 1, 2).unwrap().task.unwrap();
        assert_eq!(t.id.as_deref(), Some("3"));
        assert_eq!(t.status.as_deref(), Some("completed"));
    }

    #[test]
    fn task_created_event_parsed() {
        let raw = with(
            base("TaskCreated"),
            json!({"task_id": "task-001", "task_subject": "Implement auth"}),
        );
        let t = strip_event(&raw, 1, 2).unwrap().task.unwrap();
        assert_eq!(t.id.as_deref(), Some("task-001"));
        assert_eq!(t.subject.as_deref(), Some("Implement auth"));
        assert_eq!(t.status.as_deref(), Some("pending"));
    }

    #[test]
    fn prompt_truncated() {
        let raw = with(base("UserPromptSubmit"), json!({"prompt": "p".repeat(400)}));
        let m = strip_event(&raw, 1, 2).unwrap().message.unwrap();
        assert_eq!(m.chars().count(), 120);
    }

    #[test]
    fn missing_session_id_is_none() {
        assert!(strip_event(&json!({"hook_event_name": "Stop"}), 1, 2).is_none());
        assert!(strip_event(&json!({"session_id": "x"}), 1, 2).is_none());
    }

    #[test]
    fn statusline_rate_limits_parsed() {
        let raw = json!({
            "session_id": "abc",
            "model": {"id": "claude-opus-5-5", "display_name": "Opus"},
            "context_window": {"context_window_size": 200000, "used_percentage": 8},
            "rate_limits": {
                "five_hour": {"used_percentage": 31.5, "resets_at": 1738429200},
                "seven_day": {"used_percentage": 12}
            }
        });
        let s = strip_status(&raw, 1, 2).unwrap();
        assert_eq!(s.context_used_pct, Some(8.0));
        assert_eq!(s.context_window_size, Some(200000));
        assert_eq!(s.five_hour, Some(Window { used_pct: 31.5, resets_at: Some(1738429200) }));
        assert_eq!(s.seven_day, Some(Window { used_pct: 12.0, resets_at: None }));
        assert_eq!(s.model.as_deref(), Some("Opus"));
    }

    #[test]
    fn statusline_without_limits() {
        let s = strip_status(&json!({"session_id": "abc"}), 1, 2).unwrap();
        assert!(s.five_hour.is_none() && s.context_used_pct.is_none());
    }

    #[test]
    fn fixtures_parse() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures");
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            for (i, line) in text.lines().enumerate().filter(|(_, l)| !l.trim().is_empty()) {
                let v: Value = serde_json::from_str(line)
                    .unwrap_or_else(|e| panic!("{}:{}: {e}", path.display(), i + 1));
                let raw = v.get("payload").unwrap_or(&v);
                assert!(
                    strip_event(raw, 1, 2).is_some() || strip_status(raw, 1, 2).is_some(),
                    "{}:{} did not strip",
                    path.display(),
                    i + 1
                );
            }
        }
    }
}
