//! Session store: one entry per Claude Code session, driven by hook events.

use std::collections::{HashMap, HashSet, VecDeque};

use islet_proto::{Event, StatusLine, TaskDelta};
use serde::Serialize;

pub const STEP_HISTORY: usize = 20;
const FILES_MAX: usize = 500;
const STALE_GRACE_MS: u64 = 30_000;
const COMPACT_WARNING_MS: u64 = 10_000;
/// Sessions with no known pid are dropped after this long without events.
const ORPHAN_MS: u64 = 6 * 60 * 60 * 1000;
/// Status line context wins over the transcript estimate for this long.
const STATUSLINE_CONTEXT_FRESH_MS: u64 = 60_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Working,
    WaitingInput,
    AwaitingApproval,
    Done,
    Error,
    Stale,
}

impl SessionState {
    /// States that keep the island alive in low memory mode.
    pub fn is_active(self) -> bool {
        matches!(self, Self::Working | Self::WaitingInput | Self::AwaitingApproval)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::WaitingInput => "waiting",
            Self::AwaitingApproval => "needs approval",
            Self::Done => "done",
            Self::Error => "error",
            Self::Stale => "ended",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TodoItem {
    pub id: Option<String>,
    pub content: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Step {
    pub ts: u64,
    pub tool: String,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ContextMeter {
    pub used_pct: f64,
    pub window: u64,
    pub source: &'static str,
    #[serde(skip)]
    pub ts: u64,
}

#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub repo: String,
    pub cwd: String,
    pub state: SessionState,
    pub current_step: Option<Step>,
    pub todos: Vec<TodoItem>,
    pub context: Option<ContextMeter>,
    pub model: Option<String>,
    pub prompt: Option<String>,
    /// Claude's latest text in this turn, from the transcript.
    pub narration: Option<String>,
    pub message: Option<String>,
    pub started_at: u64,
    /// Start of the current (or last) turn.
    pub turn_started_at: u64,
    /// Set once the turn finishes; the elapsed timer stops here.
    pub turn_ended_at: Option<u64>,
    pub last_event_at: u64,
    pub steps: VecDeque<Step>,
    pub files: HashSet<String>,
    pub ppid: u32,
    pub transcript_path: Option<String>,
    pub compact_warning_until: Option<u64>,
    pub stale_since: Option<u64>,
}

/// What the frontend and tray see.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SessionView {
    pub id: String,
    pub repo: String,
    pub state: SessionState,
    pub current_step: Option<Step>,
    pub active_todo: Option<String>,
    pub todos_done: usize,
    pub todos_total: usize,
    pub context: Option<ContextMeter>,
    pub compact_warning: bool,
    pub model: Option<String>,
    pub prompt: Option<String>,
    pub narration: Option<String>,
    pub message: Option<String>,
    pub started_at: u64,
    pub turn_started_at: u64,
    pub turn_ended_at: Option<u64>,
    pub last_event_at: u64,
    pub files_touched: usize,
    pub steps: Vec<Step>,
}

pub fn repo_name(cwd: &str) -> String {
    cwd.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(cwd)
        .to_string()
}

impl Session {
    fn new(id: &str, cwd: &str, ppid: u32, ts: u64) -> Self {
        Self {
            id: id.to_string(),
            repo: repo_name(cwd),
            cwd: cwd.to_string(),
            state: SessionState::Done,
            current_step: None,
            todos: Vec::new(),
            context: None,
            model: None,
            prompt: None,
            narration: None,
            message: None,
            started_at: ts,
            turn_started_at: ts,
            turn_ended_at: None,
            last_event_at: ts,
            steps: VecDeque::with_capacity(STEP_HISTORY),
            files: HashSet::new(),
            ppid,
            transcript_path: None,
            compact_warning_until: None,
            stale_since: None,
        }
    }

    fn start_turn(&mut self, ts: u64) {
        self.turn_started_at = ts;
        self.turn_ended_at = None;
    }

    fn end_turn(&mut self, ts: u64) {
        self.turn_ended_at.get_or_insert(ts);
    }

    fn push_step(&mut self, step: Step) {
        if self.steps.len() == STEP_HISTORY {
            self.steps.pop_front();
        }
        self.steps.push_back(step.clone());
        self.current_step = Some(step);
    }

    fn apply_task(&mut self, delta: &TaskDelta) {
        let pos = self.todos.iter().position(|t| {
            (delta.id.is_some() && t.id == delta.id)
                || (delta.subject.is_some() && Some(&t.content) == delta.subject.as_ref())
        });
        match pos {
            Some(i) => {
                let t = &mut self.todos[i];
                if let Some(s) = &delta.status {
                    t.status = s.clone();
                }
                if t.id.is_none() {
                    t.id = delta.id.clone();
                }
                if let Some(subj) = &delta.subject {
                    t.content = subj.clone();
                }
                if t.status == "deleted" {
                    self.todos.remove(i);
                }
            }
            None => {
                if let Some(content) = delta.subject.clone() {
                    self.todos.push(TodoItem {
                        id: delta.id.clone(),
                        content,
                        status: delta.status.clone().unwrap_or_else(|| "pending".into()),
                    });
                }
            }
        }
    }

    pub fn view(&self, now: u64) -> SessionView {
        let active_todo = self
            .todos
            .iter()
            .find(|t| t.status == "in_progress")
            .map(|t| t.content.clone());
        SessionView {
            id: self.id.clone(),
            repo: self.repo.clone(),
            state: self.state,
            current_step: self.current_step.clone(),
            active_todo,
            todos_done: self.todos.iter().filter(|t| t.status == "completed").count(),
            todos_total: self.todos.len(),
            context: self.context.clone(),
            compact_warning: self.compact_warning_until.is_some_and(|t| t > now),
            model: self.model.clone(),
            prompt: self.prompt.clone(),
            narration: self.narration.clone(),
            message: self.message.clone(),
            started_at: self.started_at,
            turn_started_at: self.turn_started_at,
            turn_ended_at: self.turn_ended_at,
            last_event_at: self.last_event_at,
            files_touched: self.files.len(),
            steps: self.steps.iter().cloned().collect(),
        }
    }
}

const FILE_TOOLS: &[&str] = &["Write", "Edit", "MultiEdit", "NotebookEdit"];
const WAITING_NOTIFICATIONS: &[&str] =
    &["idle_prompt", "elicitation_dialog", "elicitation_url_dialog", "agent_needs_input"];

#[derive(Debug, Default)]
pub struct Store {
    pub sessions: HashMap<String, Session>,
}

impl Store {
    /// Applies a hook event. Returns true when anything visible changed.
    pub fn apply_event(&mut self, ev: &Event) -> bool {
        if ev.kind == "SessionEnd" {
            return self.sessions.remove(&ev.session_id).is_some();
        }
        let s = self
            .sessions
            .entry(ev.session_id.clone())
            .or_insert_with(|| Session::new(&ev.session_id, &ev.cwd, ev.ppid, ev.ts));
        s.last_event_at = s.last_event_at.max(ev.ts);
        s.stale_since = None;
        if ev.ppid != 0 {
            s.ppid = ev.ppid;
        }
        if !ev.cwd.is_empty() && ev.cwd != s.cwd {
            s.repo = repo_name(&ev.cwd);
            s.cwd = ev.cwd.clone();
        }
        if ev.transcript_path.is_some() {
            s.transcript_path = ev.transcript_path.clone();
        }
        if let Some(todos) = &ev.todos {
            s.todos = todos
                .iter()
                .map(|t| TodoItem { id: None, content: t.content.clone(), status: t.status.clone() })
                .collect();
        }
        if let Some(task) = &ev.task {
            s.apply_task(task);
        }

        match ev.kind.as_str() {
            "SessionStart" => {}
            "UserPromptSubmit" => {
                s.start_turn(ev.ts);
                s.state = SessionState::Working;
                s.prompt = ev.message.clone();
                s.narration = None;
                s.message = None;
            }
            "PreToolUse" => {
                // Tool use after a finished turn without a prompt event starts a new turn.
                if s.turn_ended_at.is_some() {
                    s.start_turn(ev.ts);
                }
                s.state = SessionState::Working;
                s.message = None;
                if let Some(tool) = &ev.tool_name {
                    s.push_step(Step { ts: ev.ts, tool: tool.clone(), summary: ev.tool_summary.clone() });
                }
            }
            "PostToolUse" => {
                if s.state != SessionState::AwaitingApproval {
                    s.state = SessionState::Working;
                }
                if let (Some(tool), Some(path)) = (&ev.tool_name, &ev.tool_summary) {
                    if FILE_TOOLS.contains(&tool.as_str()) && s.files.len() < FILES_MAX {
                        s.files.insert(path.clone());
                    }
                }
            }
            "PermissionRequest" => {
                s.state = SessionState::AwaitingApproval;
                if let Some(tool) = &ev.tool_name {
                    s.current_step =
                        Some(Step { ts: ev.ts, tool: tool.clone(), summary: ev.tool_summary.clone() });
                }
            }
            "Notification" => {
                let t = ev.notification_type.as_deref().unwrap_or_default();
                if WAITING_NOTIFICATIONS.contains(&t) {
                    s.state = SessionState::WaitingInput;
                    s.message = ev.message.clone();
                }
            }
            "Stop" => {
                if ev.agent_id.is_none() {
                    s.end_turn(ev.ts);
                    s.state = SessionState::Done;
                    s.message = None;
                }
            }
            "StopFailure" => {
                s.end_turn(ev.ts);
                s.state = SessionState::Error;
            }
            "PreCompact" => s.compact_warning_until = Some(ev.ts + COMPACT_WARNING_MS),
            // SubagentStop, TaskCreated, TaskCompleted: data only.
            _ => {}
        }
        true
    }

    /// Returns a session to Working after its approval is answered or released.
    pub fn approval_finished(&mut self, session_id: &str) -> bool {
        match self.sessions.get_mut(session_id) {
            Some(s) if s.state == SessionState::AwaitingApproval => {
                s.state = SessionState::Working;
                true
            }
            _ => false,
        }
    }

    pub fn apply_status(&mut self, st: &StatusLine) -> bool {
        let s = self
            .sessions
            .entry(st.session_id.clone())
            .or_insert_with(|| Session::new(&st.session_id, "", st.ppid, st.ts));
        if st.ppid != 0 && s.ppid == 0 {
            s.ppid = st.ppid;
        }
        let mut changed = false;
        if st.model.is_some() && s.model != st.model {
            s.model = st.model.clone();
            changed = true;
        }
        if let Some(pct) = st.context_used_pct {
            let meter = ContextMeter {
                used_pct: pct,
                window: st.context_window_size.unwrap_or(200_000),
                source: "statusline",
                ts: st.ts,
            };
            changed |= s.context.as_ref().is_none_or(|c| c.used_pct != pct || c.window != meter.window);
            s.context = Some(meter);
        }
        changed
    }

    /// Context from the transcript tailer. Ignored while status line data is fresh.
    pub fn apply_transcript_context(&mut self, id: &str, used: u64, window: u64, now: u64) -> bool {
        let Some(s) = self.sessions.get_mut(id) else { return false };
        if let Some(c) = &s.context {
            if c.source == "statusline" && now.saturating_sub(c.ts) < STATUSLINE_CONTEXT_FRESH_MS {
                return false;
            }
        }
        let pct = (used as f64 / window.max(1) as f64 * 100.0).min(100.0);
        let pct = (pct * 10.0).round() / 10.0;
        let changed = s.context.as_ref().is_none_or(|c| c.used_pct != pct);
        s.context = Some(ContextMeter { used_pct: pct, window, source: "transcript", ts: now });
        changed
    }

    /// Claude's latest text from the transcript tailer.
    pub fn apply_narration(&mut self, id: &str, text: String) -> bool {
        let Some(s) = self.sessions.get_mut(id) else { return false };
        if s.narration.as_ref() == Some(&text) {
            return false;
        }
        s.narration = Some(text);
        true
    }

    pub fn mark_stale(&mut self, id: &str, now: u64) -> bool {
        match self.sessions.get_mut(id) {
            Some(s) if s.state != SessionState::Stale => {
                s.state = SessionState::Stale;
                s.stale_since = Some(now);
                let last = s.last_event_at;
                s.end_turn(last);
                true
            }
            _ => false,
        }
    }

    /// Drops stale sessions after the grace period and orphans after 6 h.
    pub fn sweep(&mut self, now: u64) -> bool {
        let before = self.sessions.len();
        self.sessions.retain(|_, s| {
            let stale_expired = s.stale_since.is_some_and(|t| now.saturating_sub(t) >= STALE_GRACE_MS);
            let orphan = s.ppid == 0 && now.saturating_sub(s.last_event_at) >= ORPHAN_MS;
            !(stale_expired || orphan)
        });
        let mut changed = self.sessions.len() != before;
        for s in self.sessions.values_mut() {
            if s.compact_warning_until.is_some_and(|t| t <= now) {
                s.compact_warning_until = None;
                changed = true;
            }
        }
        changed
    }

    pub fn tracked_pids(&self) -> Vec<(String, u32)> {
        self.sessions
            .values()
            .filter(|s| s.ppid != 0 && s.state != SessionState::Stale)
            .map(|s| (s.id.clone(), s.ppid))
            .collect()
    }

    pub fn any_active(&self) -> bool {
        self.sessions.values().any(|s| s.state.is_active())
    }

    pub fn snapshot(&self, now: u64) -> Vec<SessionView> {
        let mut v: Vec<_> = self.sessions.values().map(|s| s.view(now)).collect();
        v.sort_by(|a, b| a.started_at.cmp(&b.started_at).then_with(|| a.id.cmp(&b.id)));
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use islet_proto::{Todo, PROTO_VERSION};

    fn ev(kind: &str) -> Event {
        Event {
            v: PROTO_VERSION,
            session_id: "s1".into(),
            cwd: "C:\\code\\proj".into(),
            transcript_path: Some("t.jsonl".into()),
            kind: kind.into(),
            tool_name: None,
            tool_summary: None,
            todos: None,
            task: None,
            notification_type: None,
            message: None,
            agent_id: None,
            ppid: 42,
            ts: 1000,
        }
    }

    fn tool(kind: &str, name: &str, summary: &str) -> Event {
        Event { tool_name: Some(name.into()), tool_summary: Some(summary.into()), ..ev(kind) }
    }

    fn state(store: &Store) -> SessionState {
        store.sessions["s1"].state
    }

    #[test]
    fn turn_timer_stops_on_stop() {
        let mut st = Store::default();
        let at = |kind: &str, ts: u64| Event { ts, ..ev(kind) };
        st.apply_event(&at("UserPromptSubmit", 1000));
        st.apply_event(&Event { tool_name: Some("Read".into()), ..at("PreToolUse", 2000) });
        let v = st.sessions["s1"].view(5000);
        assert_eq!((v.turn_started_at, v.turn_ended_at), (1000, None));
        st.apply_event(&at("Stop", 3000));
        // A later idle notification does not move the end.
        st.apply_event(&Event { notification_type: Some("idle_prompt".into()), ..at("Notification", 63_000) });
        let v = st.sessions["s1"].view(90_000);
        assert_eq!((v.turn_started_at, v.turn_ended_at), (1000, Some(3000)));
        st.apply_event(&at("UserPromptSubmit", 100_000));
        let v = st.sessions["s1"].view(100_000);
        assert_eq!((v.turn_started_at, v.turn_ended_at), (100_000, None));
    }

    #[test]
    fn narration_cleared_on_new_prompt() {
        let mut st = Store::default();
        st.apply_event(&ev("UserPromptSubmit"));
        assert!(st.apply_narration("s1", "Running tests.".into()));
        assert!(!st.apply_narration("s1", "Running tests.".into()));
        assert_eq!(st.sessions["s1"].view(0).narration.as_deref(), Some("Running tests."));
        st.apply_event(&ev("UserPromptSubmit"));
        assert_eq!(st.sessions["s1"].view(0).narration, None);
        assert!(!st.apply_narration("nope", "x".into()));
    }

    #[test]
    fn transition_table() {
        let mut st = Store::default();
        st.apply_event(&ev("SessionStart"));
        assert_eq!(state(&st), SessionState::Done);
        st.apply_event(&ev("UserPromptSubmit"));
        assert_eq!(state(&st), SessionState::Working);
        st.apply_event(&tool("PermissionRequest", "Bash", "rm x"));
        assert_eq!(state(&st), SessionState::AwaitingApproval);
        assert!(st.approval_finished("s1"));
        assert_eq!(state(&st), SessionState::Working);
        st.apply_event(&Event { notification_type: Some("idle_prompt".into()), ..ev("Notification") });
        assert_eq!(state(&st), SessionState::WaitingInput);
        st.apply_event(&tool("PreToolUse", "Read", "a.rs"));
        assert_eq!(state(&st), SessionState::Working);
        st.apply_event(&ev("Stop"));
        assert_eq!(state(&st), SessionState::Done);
        st.apply_event(&ev("StopFailure"));
        assert_eq!(state(&st), SessionState::Error);
        st.apply_event(&ev("SessionEnd"));
        assert!(st.sessions.is_empty());
    }

    #[test]
    fn permission_prompt_notification_ignored() {
        let mut st = Store::default();
        st.apply_event(&ev("UserPromptSubmit"));
        st.apply_event(&Event { notification_type: Some("permission_prompt".into()), ..ev("Notification") });
        assert_eq!(state(&st), SessionState::Working);
    }

    #[test]
    fn post_tool_use_keeps_awaiting_approval() {
        let mut st = Store::default();
        st.apply_event(&tool("PermissionRequest", "Bash", "x"));
        st.apply_event(&tool("PostToolUse", "Read", "y"));
        assert_eq!(state(&st), SessionState::AwaitingApproval);
    }

    #[test]
    fn subagent_stop_does_not_finish_main() {
        let mut st = Store::default();
        st.apply_event(&ev("UserPromptSubmit"));
        st.apply_event(&Event { agent_id: Some("a1".into()), ..ev("SubagentStop") });
        st.apply_event(&Event { agent_id: Some("a1".into()), ..ev("Stop") });
        assert_eq!(state(&st), SessionState::Working);
    }

    #[test]
    fn todowrite_replaces() {
        let mut st = Store::default();
        let todos = |v: &[(&str, &str)]| {
            v.iter().map(|(c, s)| Todo { content: (*c).into(), status: (*s).into() }).collect()
        };
        st.apply_event(&Event { todos: Some(todos(&[("a", "pending"), ("b", "pending")])), ..ev("PostToolUse") });
        st.apply_event(&Event {
            todos: Some(todos(&[("a", "completed"), ("b", "in_progress"), ("c", "pending")])),
            ..ev("PostToolUse")
        });
        let v = st.sessions["s1"].view(0);
        assert_eq!((v.todos_done, v.todos_total), (1, 3));
        assert_eq!(v.active_todo.as_deref(), Some("b"));
    }

    #[test]
    fn task_created_then_completed() {
        let mut st = Store::default();
        let delta = |id: &str, subj: Option<&str>, status: Option<&str>| TaskDelta {
            id: Some(id.into()),
            subject: subj.map(Into::into),
            status: status.map(Into::into),
        };
        st.apply_event(&Event { task: Some(delta("1", Some("one"), Some("pending"))), ..ev("TaskCreated") });
        st.apply_event(&Event { task: Some(delta("2", Some("two"), Some("pending"))), ..ev("TaskCreated") });
        st.apply_event(&Event { task: Some(delta("1", None, Some("in_progress"))), ..ev("PostToolUse") });
        st.apply_event(&Event { task: Some(delta("1", Some("one"), Some("completed"))), ..ev("TaskCompleted") });
        let v = st.sessions["s1"].view(0);
        assert_eq!((v.todos_done, v.todos_total), (1, 2));
    }

    #[test]
    fn task_matched_by_subject_when_id_unknown() {
        let mut st = Store::default();
        st.apply_event(&Event {
            task: Some(TaskDelta { id: None, subject: Some("one".into()), status: Some("pending".into()) }),
            ..ev("PostToolUse")
        });
        st.apply_event(&Event {
            task: Some(TaskDelta { id: Some("7".into()), subject: Some("one".into()), status: Some("completed".into()) }),
            ..ev("TaskCompleted")
        });
        let s = &st.sessions["s1"];
        assert_eq!(s.todos.len(), 1);
        assert_eq!(s.todos[0].id.as_deref(), Some("7"));
    }

    #[test]
    fn steps_capped_at_20() {
        let mut st = Store::default();
        for i in 0..50 {
            st.apply_event(&tool("PreToolUse", "Read", &format!("f{i}")));
        }
        let s = &st.sessions["s1"];
        assert_eq!(s.steps.len(), STEP_HISTORY);
        assert_eq!(s.steps.back().unwrap().summary.as_deref(), Some("f49"));
    }

    #[test]
    fn files_touched_counts_unique_writes() {
        let mut st = Store::default();
        st.apply_event(&tool("PostToolUse", "Write", "a.rs"));
        st.apply_event(&tool("PostToolUse", "Edit", "a.rs"));
        st.apply_event(&tool("PostToolUse", "Edit", "b.rs"));
        st.apply_event(&tool("PostToolUse", "Read", "c.rs"));
        assert_eq!(st.sessions["s1"].view(0).files_touched, 2);
    }

    #[test]
    fn stale_removed_after_30s() {
        let mut st = Store::default();
        st.apply_event(&ev("UserPromptSubmit"));
        assert!(st.mark_stale("s1", 10_000));
        assert!(!st.sweep(20_000));
        assert_eq!(state(&st), SessionState::Stale);
        assert!(st.sweep(40_000));
        assert!(st.sessions.is_empty());
    }

    #[test]
    fn event_revives_stale() {
        let mut st = Store::default();
        st.apply_event(&ev("UserPromptSubmit"));
        st.mark_stale("s1", 10_000);
        st.apply_event(&tool("PreToolUse", "Read", "x"));
        assert!(!st.sweep(100_000));
        assert_eq!(state(&st), SessionState::Working);
    }

    #[test]
    fn repo_from_cwd_windows_and_unix() {
        assert_eq!(repo_name("C:\\code\\proj"), "proj");
        assert_eq!(repo_name("/home/u/proj/"), "proj");
        assert_eq!(repo_name("proj"), "proj");
        assert_eq!(repo_name(""), "");
    }

    #[test]
    fn compaction_warning_expires() {
        let mut st = Store::default();
        st.apply_event(&ev("PreCompact"));
        assert!(st.sessions["s1"].view(1500).compact_warning);
        assert!(st.sweep(1000 + COMPACT_WARNING_MS));
        assert!(!st.sessions["s1"].view(1000 + COMPACT_WARNING_MS).compact_warning);
    }

    #[test]
    fn statusline_context_beats_transcript() {
        let mut st = Store::default();
        st.apply_event(&ev("SessionStart"));
        let status = StatusLine {
            v: 1,
            session_id: "s1".into(),
            context_used_pct: Some(40.0),
            context_window_size: Some(200_000),
            five_hour: None,
            seven_day: None,
            model: Some("Opus".into()),
            ppid: 42,
            ts: 1000,
        };
        assert!(st.apply_status(&status));
        assert!(!st.apply_transcript_context("s1", 10_000, 200_000, 2000));
        assert_eq!(st.sessions["s1"].context.as_ref().unwrap().used_pct, 40.0);
        assert!(st.apply_transcript_context("s1", 100_000, 200_000, 1000 + STATUSLINE_CONTEXT_FRESH_MS));
        assert_eq!(st.sessions["s1"].context.as_ref().unwrap().used_pct, 50.0);
    }
}
