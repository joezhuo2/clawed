# clawed

A small always-on-top island for Claude Code on Windows and macOS. It shows your
sessions live, answers permission prompts, and shows context, plan usage and
CPU/RAM/GPU as rings.

Built with Tauri 2 (Rust) and Svelte 5. There's no branding yet: no logo, mascot,
sounds, or Anthropic/Claude brand assets.

> **Status:** early (v0.1). Developed and tested on Windows 11. macOS code paths
> exist but haven't been built or tested. No prebuilt releases yet; build from
> source.
>
> clawed is an independent project. It is not affiliated with, endorsed by, or
> sponsored by Anthropic. "Claude" and "Claude Code" are trademarks of Anthropic.

## What it shows

- **Collapsed pill:** one dot per session colored by state, a todo progress bar
  for the most recently active session, and two mini rings (outer 7-day, inner
  5-hour plan usage).
- **Expanded (hover):** per-session rows with the current step, todo progress,
  context window meter, files touched and elapsed time; 5-hour and 7-day rings
  with reset countdowns; CPU, RAM and GPU rings.
- **Approvals:** permission requests expand the island with Allow / Deny. If
  you don't answer within 60 s, Claude Code falls back to its normal prompt.
- **Tray menu:** usage, sessions, pause approvals, launch at login, low memory
  mode, hooks, settings, quit.

States: blue working, amber waiting for you, violet needs approval, green done,
red error, gray ended.

## How it works

```
Claude Code --stdin--> clawed-hook --local socket--> clawed (Rust) --events--> island (Svelte)
```

- `crates/hook`: `clawed-hook`, a 260 KB std-only binary that Claude Code
  runs on every hook event and as its status line. If the app isn't running
  it exits 0 immediately with no output, so Claude Code is never blocked.
  File contents from Write/Edit are never forwarded.
- `crates/proto`: wire types and payload stripping.
- `src-tauri`: IPC server, session store, transcript tailer, usage rings,
  system meters, installer, tray, window.
- `ui`: the island and settings windows.

Plan usage comes from Claude Code's status line JSON (`rate_limits`). When
that isn't available, it's estimated from local transcripts and labeled
"estimated".

## Build

Requirements: Rust 1.85+, Node 20+, and the Tauri 2 prerequisites for your OS.

```bash
npm install
npm run tauri dev          # dev build with hot reload
npx tauri build            # installer (NSIS on Windows, dmg on macOS)
```

`npm run build:hook` builds `clawed-hook` and places it where Tauri bundles it
as a sidecar; both commands above run it automatically.

## Install the hooks

Open **Settings** from the tray and choose **Install hooks…**. You'll see the
exact diff to `~/.claude/settings.json` before anything is written, and a backup
is saved next to it. The hook binary is copied to a stable per-user location
(`%LOCALAPPDATA%\clawed\bin` or `~/Library/Application Support/clawed/bin`).
If you already have a status line, it's left alone and plan usage falls back
to the estimate.

**Uninstall hooks…** removes only clawed's entries. Restart running Claude Code
sessions after either change.

## Tests

```bash
cargo test --workspace     # proto, hook (spawns the real binary), backend
npm test                   # frontend helpers
npm run check              # svelte-check
```

Replay recorded or synthetic hook payloads into a running app:

```bash
cargo run -p clawed --bin replay -- fixtures/sample-session.jsonl --speed 2
```

## Environment variables

| Variable | Effect |
| --- | --- |
| `CLAWED_DEBUG` | Print state snapshots and window calls to stderr |
| `CLAWED_SOCKET` | Use a different socket name (tests, measurements) |
| `CLAWED_NO_AUTOSTART` | Don't register launch at login on first run |
| `CLAWED_WEBVIEW_ARGS` | Replace the WebView2 browser arguments (Windows) |

## Known limitations

- macOS: the island sits below the menu bar; placing it around the notch needs
  an NSPanel (`tauri-nspanel`). The GPU ring shows `--`. The macOS paths
  compile in principle but haven't been built or tested.
- Windows named pipes: another local process could create the pipe name before
  clawed starts and receive hook events or answer approvals. That's a low risk
  on a single-user machine and is planned to be hardened.

## Privacy

Everything stays on your machine. clawed makes no network requests. The hook
forwards event metadata (session, tool name, paths, prompts) to the local app
only, and never forwards file contents from Write/Edit.

## Project layout

```
crates/proto/     wire types, pipe name, payload stripping
crates/hook/      clawed-hook binary
src-tauri/        backend app (Rust)
ui/               island and settings windows (Svelte)
fixtures/         synthetic hook payloads for replay
scripts/          hook build and memory measurement helpers
docs/             design notes and measurements
```

See [docs/design.md](docs/design.md) for the design and
[docs/memory.md](docs/memory.md) for memory measurements.

## Contributing

Issues and pull requests are welcome. Please run `cargo test --workspace`,
`npm test` and `npm run check` before opening a PR.

## License

[MIT](LICENSE)
