# islet v1 design

A cross-platform (Windows + macOS) dynamic island built with Tauri 2 (Rust + Svelte). It tracks Claude Code sessions live, shows context and plan usage, and answers permission prompts. Functionality only: no logo, mascot, sounds, or Anthropic/Claude brand assets. Inspired by coucou; nothing from it is reused.

Development and testing happen on Windows. macOS code is verified by `cargo clippy --target aarch64-apple-darwin` locally and by the macOS CI job; it has not yet been run on a Mac.

## Decisions

| Topic | Decision |
| --- | --- |
| Name | `islet` (repo, crates, installer) |
| Scope | Full v1 |
| Frontend | Svelte 5 + Vite, plain CSS |
| Hook shipping | Separate tiny `islet-hook` binary, std + sync `interprocess`, no async runtime |
| Approvals | `PermissionRequest` hook only (no PreToolUse fallback) |
| Approval timeout | 60 s in the hook; settings `timeout` 65 s |
| Usage source | Account usage endpoint (`/api/oauth/usage`, Claude Code's OAuth token), Claude Code status line JSON (`rate_limits.*`, `context_window.*`), local JSONL estimate until the first status line arrives or when it goes stale |
| Launch at login | On by default, toggle in tray menu |
| Low memory mode | On by default, toggle in tray menu. Island window destroyed after 3 min with no Working/WaitingInput/AwaitingApproval session |
| Collapsed step bar | Most recently active working session |
| Updates | No in-app updater. Daily check of GitHub `releases/latest`; tray item links to the release page (opt-out setting) |

## Architecture

```
Claude Code --stdin JSON--> islet-hook --named pipe / unix socket--> backend --Tauri events--> island UI
     ^                           ^                                       |
     |                           +------------- Decision ----------------+
     +-- stdout decision JSON ---+
```

Workspace:

```
islet/
  Cargo.toml            workspace
  crates/proto/         serde wire types, pipe name, payload stripping
  crates/hook/          islet-hook binary
  crates/replay/        replay dev tool (kept out of the app bundle)
  src-tauri/            backend app
  ui/                   Svelte frontend
  fixtures/             captured payloads (*.jsonl) + replay input
  docs/design.md        this document
```

## Wire protocol (`proto`)

Newline-delimited JSON over one connection per hook invocation. Every message carries `v: 1`; the backend drops messages with an unknown `v`.

- Pipe name: `\\.\pipe\islet-<username>` (Windows), `$TMPDIR/islet-<uid>.sock` (macOS), via `interprocess` local sockets.
- Access (`proto::peer`): on Windows the server pipe is created with `FILE_FLAG_FIRST_PIPE_INSTANCE`, `PIPE_REJECT_REMOTE_CLIENTS` and a protected DACL `D:P(A;;GA;;;<user SID>)`. Both ends check the peer: the hook gets the server pid from the pipe and compares its token user SID with its own (on Unix, the peer euid); on mismatch it sends nothing and exits 0. The backend drops clients that fail the same check.
- `HookMsg::Event(Event)` fire-and-forget.
- `HookMsg::Approval { id, event }` then the hook reads one line: `AppMsg::Decision { id, behavior: allow | deny }` or `AppMsg::Release { id }` (no decision).
- `HookMsg::Status(StatusLine)` fire-and-forget, from the status line subcommand.

`Event { v, session_id, cwd, transcript_path, kind, tool_name?, tool_summary?, todos?, task?, notification_type?, message?, ppid, ts }`

Stripping (in `proto`, unit tested):
- `tool_summary`: `file_path` / `path` / `notebook_path` for file tools, `command` truncated to 200 chars for Bash/PowerShell, `pattern` for Grep/Glob, `url` for WebFetch, otherwise none. `content`, `old_string`, `new_string`, `edits` never leave the hook.
- `todos`: from `TodoWrite` `tool_input.todos` as `[{content (<=120 chars), status}]`.
- `task`: from `TaskCreate` / `TaskUpdate` tool input and `TaskCreated` / `TaskCompleted` events as `{id?, subject?, status?}`.
- `message`: Notification text truncated to 200 chars. `UserPromptSubmit` prompt truncated to 120 chars.

`StatusLine { v, session_id, context_used_pct?, context_window_size?, five_hour {used_pct, resets_at}?, seven_day {used_pct, resets_at}?, model?, ppid, ts }`

## Hook binary

- `islet-hook` reads stdin (max 4 MB, then truncates), parses, strips, connects, writes, exits.
- Connect failure or any error: exit 0, empty stdout. This is the core guarantee and is tested first.
- `islet-hook statusline`: forwards `StatusLine`, prints a one-line status (`<model> · ctx 42% · 5h 31% · 7d 12%`) so the terminal status line stays useful.
- For `PermissionRequest`: sends `Approval`, waits up to 60 s. On `Decision` prints `{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}` (deny adds `"message":"Denied from islet"`). On timeout, `Release`, EOF, or error prints nothing, exits 0, and Claude Code shows its own prompt.
- ppid: parent process id (`GetCurrentProcess` parent via `sysinfo` is too heavy; use `std::os::unix::process::parent_id` on Unix and a `CreateToolhelp32Snapshot` lookup on Windows).
- Release profile: `opt-level="s"`, `lto`, `codegen-units=1`, `panic="abort"`, `strip`.

Hook registration (exec form, no shell): `{"type":"command","command":"<abs>/islet-hook.exe","args":[]}`. All events except `PermissionRequest` also get `"async": true` so Claude Code never waits on them. `PermissionRequest` gets `"timeout": 65`.

Events registered: `SessionStart`, `SessionEnd`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `Notification`, `Stop`, `StopFailure`, `SubagentStop`, `PreCompact`, `PermissionRequest`, `TaskCreated`, `TaskCompleted`.

## Backend

Single-thread tokio runtime (`current_thread` for our tasks; Tauri owns its own main loop).

### IPC server
Fails to start if another process already owns the socket name (first-instance flag). Accepts connections, rejects other-user peers, reads one line, dispatches. Approvals keep the connection open in a pending map `id -> (Approval, writer)`. Shutdown and "Pause approvals" send `Release` to every pending connection.

### Session store
`HashMap<String, Session>` behind `tokio::sync::RwLock`.

`Session { id, repo (basename of cwd), cwd, state, current_step, todos: Vec<Todo>, context {used_pct, window, source}, started_at, last_event_at, steps: ring buffer of 20, ppid, transcript_path, compact_warning_until }`

State transitions:

| Event | New state |
| --- | --- |
| SessionStart | Done (idle, waiting for a prompt) |
| UserPromptSubmit, PreToolUse, PostToolUse | Working |
| PermissionRequest | AwaitingApproval (returns to Working on decision/release) |
| Notification `idle_prompt`, `elicitation_*`, `agent_needs_input` | WaitingInput |
| Stop, SubagentStop (main thread) | Done |
| StopFailure | Error |
| SessionEnd | removed |
| ppid dead | Stale, removed 30 s later |

Todo adapter: `TodoWrite` replaces the list; `TaskCreate`/`TaskCreated` append; `TaskUpdate`/`TaskCompleted` update by id or subject. Progress = completed / total.

Liveness: every 10 s, refresh only tracked PIDs with `sysinfo` (`ProcessesToUpdate::Some`).

Emission: `sessions-updated` with the full (small) session list, coalesced to at most 10 Hz. Session count is small, so full snapshots are simpler than diffs and still cheap.

### Meters
- Context: status line `context_used_pct` when present (source `statusline`). Otherwise tail the transcript with `notify`, byte offset per file, parse only lines containing `"usage"`, compute input + cache read + cache creation over the window size (config map, default 200k, `[1m]` models 1M). Watch only active sessions' transcripts.
- PreCompact sets `compact_warning_until = now + 10 s`.
- Usage rings: account usage endpoint, polled at most every 2 min, or latest status line `rate_limits` (source `exact`). If none within 10 min, local estimate (source `estimated`): scan `~/.claude/projects/**/*.jsonl` modified within 7 days, cache offsets and per-file hour buckets, sum tokens in 5 h / 7 d windows against user caps from settings. Refresh every 60 s while any session is active, 5 min when idle. Unknown state when neither is available.
- `usage-updated` event; tray menu text updated on the same refresh.

### Window
- Transparent, undecorated, always-on-top, `skipTaskbar`, not focusable, top-center of the primary monitor.
- Windows: `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW` set on the HWND.
- macOS: `ActivationPolicy::Accessory` plus `LSUIElement` (no Dock icon). The window is converted to a `tauri-nspanel` panel (`can_become_key_window: false`, `NSWindowStyleMaskNonactivatingPanel`) at `NSStatusWindowLevel` (25, above the menu bar), with collection behavior `canJoinAllSpaces | stationary | fullScreenAuxiliary | ignoresCycle`. Creation and teardown are dispatched to the main thread; teardown converts the panel back to a window before destroying it.
- macOS placement: top offset is the primary screen's top inset, `max(frame.maxY - visibleFrame.maxY, safeAreaInsets.top)`. That is the menu bar height, which on notched MacBooks equals the notch height, so the pill hangs directly under the notch; with an auto-hiding menu bar the notch inset still applies. Drawing inside the notch is out of scope (the pill content would sit behind the camera housing).
- Click-through while collapsed (`set_ignore_cursor_events(true)`). A 60 ms cursor poll (only while the window exists) checks the pill rect and emits `hover` enter/leave; while expanded, click-through is off.
- Native window resized to expanded bounds before the expand animation and shrunk after the collapse animation (frontend calls `set_island_size` when the transition ends).
- Low memory: window destroyed after 3 min of no active session and no pending approval; recreated on the next hook event.

### Tray
`TrayIconBuilder`, template glyph (rounded pill) with idle / working / approval variants. Menu, rebuilt only on state or usage change:

- `Update available: vX.Y.Z…` (only when the update check found a newer release; opens the release page)
- Show / hide island
- `5h: 31% (resets 2h 10m)` and `7d: 12%` (disabled text items, "estimated" suffix when applicable)
- Active sessions (repo — state)
- Pause approvals (checkbox)
- Launch at login (checkbox, reads `autolaunch().is_enabled()`)
- Low memory mode (checkbox)
- Install hooks / Uninstall hooks
- Settings
- Quit

### Installer
- Reads `~/.claude/settings.json`, writes `settings.json.islet-backup-<ts>`, merges our handlers into `hooks.<Event>` arrays without touching others, sets `statusLine` only when none is set (otherwise leaves it and notes the estimate is used), shows a diff in the settings window, writes on confirm.
- Our entries are identified by `command` ending in `islet-hook` / `islet-hook.exe`. Uninstall removes only those, and removes `statusLine` only if it is ours.
- The hook binary is copied to the app data dir (`%LOCALAPPDATA%\islet\bin`, `~/Library/Application Support/islet/bin`) so the path is stable across app updates. At launch, if that copy exists and differs from the bundled hook, it is replaced (a copy held by a running hook is renamed aside first); if hooks were never installed nothing is created.

### Settings
JSON in the app config dir: context window overrides, estimate caps (5 h, 7 d tokens), ring thresholds (70 / 90), low memory mode, pause approvals, idle teardown minutes (3), update check (on). Launch at login is read from the OS.

### Autostart
`tauri-plugin-autostart` (LaunchAgent / HKCU Run) with `--autostarted`. Enabled on first run. Autostarted launches stay in the tray; if hooks are not installed, one tray notification.

Single instance via `tauri-plugin-single-instance`.

## Frontend

Svelte 5, no component libraries. `theme.css` holds every color and dimension as CSS variables.

- Collapsed pill (black, fully rounded, 260 px): one dot per session colored by state, step bar with `3/7` for the most recently active working session (hidden when none), then three mini rings: CPU (inner) / RAM (outer), GPU, 5 h (inner) / 7 d (outer).
- Expanded: rows with repo, state, Claude's latest text (last `text` block of the newest main-thread assistant transcript line, whitespace collapsed, 200 chars, two lines; cleared on a new prompt), current step, todo bar, context meter (flashes on compaction warning), elapsed time; full rings with percent, reset countdown, and an "estimated" label when applicable; a second row of CPU / RAM / GPU rings in the same style.
- Approval card: tool, summary, Allow / Deny. Auto-expands, stays open until answered or released, queue shown as `1 of 3`.
- WaitingInput sessions also auto-expand, showing the notification message.
- Transitions: the shape animates with `clip-path` (compositor-friendly, no layout), content fades with opacity. Collapse starts immediately when the cursor leaves; re-entering mid-collapse reverses.
- No timers while collapsed. Elapsed and countdown tick once per second only while expanded.
- State colors: working blue, waiting amber, approval violet, done green, error red, stale gray.
- User colors (`colors` in settings, `#rrggbb`, anything else falls back to the default): working, done, error, approval request, each ring (5 h, 7 d, CPU, RAM, GPU), context bar, and the shared warn / critical colors every meter switches to above the thresholds. Sent in each snapshot and written over the `theme.css` variables on `:root`.

## System meters

CPU and RAM via `sysinfo` (global CPU usage, used/total memory). GPU on Windows via PDH `\GPU Engine(*)\Utilization Percentage`, summed per engine type and taking the busiest type (Task Manager's method). macOS GPU via IOKit: every `IOAccelerator` service's `PerformanceStatistics` dictionary, key `Device Utilization %` (what Activity Monitor shows), taking the busiest GPU. Sampling runs while the island window exists, every 1.5 s expanded and every 3 s collapsed (for the pill rings); the sampler and its PDH query are dropped when low memory mode tears the window down. Emitted as a separate `system` event so the tray is never rebuilt for it.

## Testing

- `proto`: stripping (oversized Write/Edit never forwarded), serde round trips, fixtures parse.
- `hook`: app absent exits 0 fast with no output; approval allow/deny/timeout/release against a fake server; malformed stdin.
- Store: transition table, todo adapter for TodoWrite and Task tools, stale handling, ring buffer bound.
- Usage: status line parsing, estimate windows, threshold colors, unknown state, reset rollover.
- Installer: merge into settings with other hooks, idempotent reinstall, uninstall leaves others untouched, statusLine rules, backup written.
- `replay` bin: feeds `fixtures/*.jsonl` into the pipe at real or accelerated speed.
- IPC: a second server on the same name fails; peer SID checks for own pid, System pid, SDDL shape.
- Manual: window focus, click-through, hover, tray, autostart on Windows; fresh-machine install checklist in `docs/releasing.md`.
- CI (`.github/workflows/ci.yml`): clippy `-D warnings`, `cargo test --workspace`, `npm test`, `npm run check`, version sync, on Windows and macOS (both blocking). Tags `v*` run `release.yml`: NSIS installer, Apple silicon and Intel dmgs (Developer ID signed and notarized when secrets are set, ad-hoc otherwise), one `SHA256SUMS.txt`, draft release.

## Memory

- Release profile as above for both binaries.
- Measure idle-torn-down, idle-visible, busy across the full process tree (msedgewebview2 children included) and record in `docs/memory.md`.

## Out of scope

Git gate, style lint, recaps, remote sessions, integrations, sounds, branding.
