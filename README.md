# clawed

A small always-on-top island for Claude Code on Windows and macOS. It shows your
sessions live, answers permission prompts, and shows context, plan usage and
CPU/RAM/GPU as rings.

Built with Tauri 2 (Rust) and Svelte 5.

> **Status:** early (v0.1.x). Developed and tested on Windows 11. macOS support
> (panel, notch placement, GPU ring, dmg builds) is in place and type-checks,
> but hasn't been run on a Mac yet, so macOS builds are a preview. See
> [CHANGELOG.md](CHANGELOG.md) for what changed.
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

Plan usage comes from your Claude account's usage endpoint (the same numbers
`/usage` shows), polled at most every 2 minutes with the OAuth token Claude Code
stores in `~/.claude/.credentials.json`. The token is only sent to
api.anthropic.com and is never refreshed by clawed. Claude Code's status line
JSON (`rate_limits`) is used too when present. When neither is available,
usage is estimated from local transcripts and labeled "estimated".

## Install (Windows)

Download `clawed_<version>_x64-setup.exe` and `SHA256SUMS.txt` from
[GitHub Releases](https://github.com/joezhuo2/clawed/releases) and check the
hash before running it:

```powershell
Get-FileHash .\clawed_0.1.3_x64-setup.exe   # compare with SHA256SUMS.txt
```

Builds without a code signing certificate trigger SmartScreen ("Windows
protected your PC"); choose **More info → Run anyway**. There's no auto-update
yet: install a newer version over the old one. The hook binary lives at a
stable per-user path, so installed hooks keep working across updates.

## Install (macOS, preview)

Download the dmg for your Mac from
[GitHub Releases](https://github.com/joezhuo2/clawed/releases):
`clawed_<version>_aarch64.dmg` (Apple silicon) or `clawed_<version>_x64.dmg`
(Intel), and check it against `SHA256SUMS.txt`:

```bash
shasum -a 256 clawed_*.dmg
```

Drag clawed to Applications. Until releases are signed with an Apple
Developer ID and notarized, Gatekeeper blocks the first launch: right-click
the app and choose **Open**, or allow it in System Settings → Privacy &
Security. clawed is a menu bar app with no Dock icon.

## Build from source

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
If you already have a status line, it's left alone.

**Uninstall hooks…** removes only clawed's entries. Restart running Claude Code
sessions after either change.

## Tests

```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace     # proto, hook (spawns the real binary), backend
npm test                   # frontend helpers
npm run check              # svelte-check
npm run check:version      # versions in Cargo.toml, package.json, tauri.conf.json match
```

CI (`.github/workflows/ci.yml`) runs all of these on Windows and macOS.

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

- macOS hasn't been run on real hardware yet. The island hangs below the
  menu bar or notch; it doesn't draw inside the notch area.
- The local socket is restricted to your user account, not to clawed itself.
  On Windows the pipe has a DACL for your user only, rejects remote clients,
  and the hook refuses a server running as another user. Any process running
  as you can still connect, but such a process can already edit
  `~/.claude/settings.json`.

## Privacy

Session data stays on your machine. The hook forwards event metadata (session,
tool name, paths, prompts) to the local app only, and never forwards file
contents from Write/Edit. The only network request clawed makes is the usage
poll to api.anthropic.com described above.

## Project layout

```
crates/proto/     wire types, pipe name, payload stripping
crates/hook/      clawed-hook binary
src-tauri/        backend app (Rust)
ui/               island and settings windows (Svelte)
fixtures/         synthetic hook payloads for replay
scripts/          hook build, version check, release notes, memory measurement
docs/             design notes, measurements, release process
.github/          CI and release workflows
```

See [docs/design.md](docs/design.md) for the design and
[docs/memory.md](docs/memory.md) for memory measurements, and
[docs/releasing.md](docs/releasing.md) for how releases are cut.

## Contributing

Issues and pull requests are welcome. Please run the commands under
[Tests](#tests) before opening a PR and add an entry under `[Unreleased]` in
[CHANGELOG.md](CHANGELOG.md).

## License

[MIT](LICENSE)
