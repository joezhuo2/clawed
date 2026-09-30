"""Generates fixtures/sample-session.jsonl: a synthetic two-session run
matching the documented Claude Code hook payload shapes."""
import json
from pathlib import Path

T0 = 1790000000000
lines = []


def ev(dt, payload):
    lines.append({"ts": T0 + dt, "payload": payload})


def base(kind, sid="replay-a", cwd="C:/code/alpha"):
    return {
        "session_id": sid,
        "cwd": cwd,
        "transcript_path": f"C:/Users/dev/.claude/projects/alpha/{sid}.jsonl",
        "hook_event_name": kind,
        "permission_mode": "default",
    }


def tool(kind, name, inp, sid="replay-a", cwd="C:/code/alpha", **kw):
    p = base(kind, sid, cwd)
    p.update({"tool_name": name, "tool_input": inp})
    p.update(kw)
    return p


B = dict(sid="replay-b", cwd="C:/code/beta")

ev(0, {**base("SessionStart"), "source": "startup", "model": "claude-opus-5-5"})
ev(500, {**base("UserPromptSubmit"), "prompt": "Add a login form with validation"})
todos = [
    {"content": "Read existing auth code", "status": "in_progress", "activeForm": "Reading"},
    {"content": "Write login form", "status": "pending", "activeForm": "Writing"},
    {"content": "Add validation", "status": "pending", "activeForm": "Validating"},
    {"content": "Run tests", "status": "pending", "activeForm": "Testing"},
]
ev(1500, tool("PreToolUse", "TodoWrite", {"todos": todos}))
ev(1600, tool("PostToolUse", "TodoWrite", {"todos": todos}, tool_response={}))
ev(2000, tool("PreToolUse", "Read", {"file_path": "C:/code/alpha/src/auth.ts"}))
ev(2300, tool("PostToolUse", "Read", {"file_path": "C:/code/alpha/src/auth.ts"}, tool_response={}))
ev(2600, {**base("SessionStart", **B), "source": "startup"})
ev(2800, {**base("UserPromptSubmit", **B), "prompt": "Fix the flaky test"})
ev(3000, {
    "session_id": "replay-a",
    "model": {"id": "claude-opus-5-5", "display_name": "Opus"},
    "context_window": {"context_window_size": 200000, "used_percentage": 18},
    "rate_limits": {
        "five_hour": {"used_percentage": 34, "resets_at": 1790010800},
        "seven_day": {"used_percentage": 61, "resets_at": 1790400000},
    },
})
todos[0]["status"] = "completed"
todos[1]["status"] = "in_progress"
ev(3500, tool("PostToolUse", "TodoWrite", {"todos": todos}, tool_response={}))
login = {"file_path": "C:/code/alpha/src/Login.tsx", "content": "export function Login() {}"}
ev(3700, tool("PreToolUse", "Write", login))
ev(3750, tool("PermissionRequest", "Write", login))
ev(4000, tool("PreToolUse", "Bash", {"command": "npm test -- --run flaky.spec.ts"}, **B))
ev(4100, tool("PermissionRequest", "Bash", {"command": "npm test -- --run flaky.spec.ts"}, **B))
ev(9000, tool("PostToolUse", "Write", login, tool_response={}))
todos[1]["status"] = "completed"
todos[2]["status"] = "in_progress"
ev(9500, tool("PostToolUse", "TodoWrite", {"todos": todos}, tool_response={}))
ev(10000, {**base("Notification", **B), "message": "Claude is waiting for your input",
           "notification_type": "idle_prompt"})
ev(11000, {**base("PreCompact"), "trigger": "auto"})
todos[2]["status"] = "completed"
todos[3]["status"] = "completed"
ev(12000, tool("PostToolUse", "TodoWrite", {"todos": todos}, tool_response={}))
ev(12500, {**base("Stop"), "stop_hook_active": False, "last_assistant_message": "Done."})
ev(14000, {**base("SessionEnd", **B), "reason": "prompt_input_exit"})

out = Path(__file__).with_name("sample-session.jsonl")
out.write_text("".join(json.dumps(l) + "\n" for l in lines), newline="\n")
print(f"wrote {len(lines)} lines to {out}")
