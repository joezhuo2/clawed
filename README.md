# islet

A small always-on-top island for Claude Code on Windows and macOS. It shows your
sessions live, answers permission prompts, and shows context, plan usage and
CPU/RAM/GPU as rings.

Built with Tauri 2 (Rust) and Svelte 5.

> **Status:** early (v0.2.x). Developed and tested on Windows 11. macOS support
> (panel, notch placement, GPU ring, dmg builds) is in place and type-checks,
> but hasn't been run on a Mac yet, so macOS builds are a preview. See
> [CHANGELOG.md](CHANGELOG.md) for what changed.
>
> islet is an independent project. It is not affiliated with, endorsed by, or
> sponsored by Anthropic. "Claude" and "Claude Code" are trademarks of Anthropic.

## What it shows

- **Collapsed pill:** one dot per session colored by state, a todo progress bar
  for the most recently active session, and three mini rings: CPU (inner) and
  RAM (outer), GPU, then 7-day (outer) and 5-hour (inner) plan usage.
- **Expanded (hover):** per-session rows with Claude's latest message, the
  current tool step, todo progress, context window meter, files touched and
  elapsed time; 5-hour and 7-day rings with reset countdowns; CPU, RAM and GPU
  rings, with CPU/GPU temperature (°C) and RAM used/total under them.
- **Colors:** state, ring, context and warn/critical colors are set under
  **Colors** in Settings.
- **Approvals:** permission requests expand the island with Allow / Deny. If
  you don't answer within 60 s, Claude Code falls back to its normal prompt.
- **Tray menu:** usage, sessions, pause approvals, launch at login, low memory
  mode, hooks, settings, quit.

States: blue working, amber waiting for you, violet needs approval, green done,
red error, gray ended.

## How it works

```
Claude Code --stdin--> islet-hook --local socket--> islet (Rust) --events--> island (Svelte)
```

- `crates/hook`: `islet-hook`, a 260 KB std-only binary that Claude Code
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
api.anthropic.com and is never refreshed by islet. Claude Code's status line
JSON (`rate_limits`) is used too when present. When neither is available,
usage is estimated from local transcripts and labeled "estimated".

## Install (Windows)

Download `islet_<version>_x64-setup.exe` and `SHA256SUMS.txt` from
[GitHub Releases](https://github.com/joezhuo2/islet/releases) and check the
hash before running it:

```powershell
Get-FileHash .\islet_0.3.0_x64-setup.exe   # compare with SHA256SUMS.txt
```

Releases aren't code signed yet, so SmartScreen shows "Windows protected your
PC" on first run; choose **More info → Run anyway**. The checksum above is how
you know the file is the one the release workflow built.

## Updating

islet checks GitHub once a day for a newer release and, when there is one,
adds **Update available** to the tray menu, linking to the release page.
Nothing is downloaded or installed automatically: download the new installer
(or dmg) and install it over the old version. Turn the check off under
**Updates** in Settings.

Installed hooks keep working across updates. They point at a copy of the hook
in a stable per-user folder, and on its first launch the new version replaces
that copy if it changed, so you don't need to run **Install hooks** again.

## Install (macOS, preview)

Download the dmg for your Mac from
[GitHub Releases](https://github.com/joezhuo2/islet/releases):
`islet_<version>_aarch64.dmg` (Apple silicon) or `islet_<version>_x64.dmg`
(Intel), and check it against `SHA256SUMS.txt`:

```bash
shasum -a 256 islet_*.dmg
```

Drag islet to Applications. Until releases are signed with an Apple
Developer ID and notarized, Gatekeeper blocks the first launch: right-click
the app and choose **Open**, or allow it in System Settings → Privacy &
Security. islet is a menu bar app with no Dock icon.

## Build from source

Requirements: Rust 1.85+, Node 20+, and the Tauri 2 prerequisites for your OS.

```bash
npm install
npm run tauri dev          # dev build with hot reload
npx tauri build            # installer (NSIS on Windows, dmg on macOS)
```

`npm run build:hook` builds `islet-hook` and places it where Tauri bundles it
as a sidecar; both commands above run it automatically.

## Install the hooks

Open **Settings** from the tray and choose **Install hooks…**. You'll see the
exact diff to `~/.claude/settings.json` before anything is written, and a backup
is saved next to it. The hook binary is copied to a stable per-user location
(`%LOCALAPPDATA%\islet\bin` or `~/Library/Application Support/islet/bin`).
If you already have a status line, it's left alone.

**Uninstall hooks…** removes only islet's entries. Restart running Claude Code
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
cargo run -p islet --bin replay -- fixtures/sample-session.jsonl --speed 2
```

## Environment variables

| Variable | Effect |
| --- | --- |
| `ISLET_DEBUG` | Print state snapshots and window calls to stderr |
| `ISLET_SOCKET` | Use a different socket name (tests, measurements) |
| `ISLET_NO_AUTOSTART` | Don't register launch at login on first run |
| `ISLET_WEBVIEW_ARGS` | Replace the WebView2 browser arguments (Windows) |

## Known limitations

- macOS hasn't been run on real hardware yet. The island hangs below the
  menu bar or notch; it doesn't draw inside the notch area.
- The local socket is restricted to your user account, not to islet itself.
  On Windows the pipe has a DACL for your user only, rejects remote clients,
  and the hook refuses a server running as another user. Any process running
  as you can still connect, but such a process can already edit
  `~/.claude/settings.json`.

## Privacy

Session data stays on your machine. The hook forwards event metadata (session,
tool name, paths, prompts) to the local app only, and never forwards file
contents from Write/Edit. islet makes two kinds of network requests: the
usage poll to api.anthropic.com described above, and the daily update check
to api.github.com (no account data, just the latest release; can be turned
off in Settings).

## Project layout

```
crates/proto/     wire types, pipe name, payload stripping
crates/hook/      islet-hook binary
src-tauri/        backend app (Rust)
ui/               island and settings windows (Svelte)
fixtures/         synthetic hook payloads for replay
scripts/          hook build, version check, release notes, memory and soak tests
docs/             design notes, measurements, robustness, release process
.github/          CI and release workflows, issue templates
```

See [docs/design.md](docs/design.md) for the design,
[docs/memory.md](docs/memory.md) for memory measurements and the soak test,
[docs/robustness.md](docs/robustness.md) for how bad input and environment
changes are handled, and [docs/releasing.md](docs/releasing.md) for how
releases are cut.

## Contributing

Issues and pull requests are welcome. Please run the commands under
[Tests](#tests) before opening a PR and add an entry under `[Unreleased]` in
[CHANGELOG.md](CHANGELOG.md). Bug reports use the issue template, which
explains how to capture `ISLET_DEBUG` output.

Security issues (for example an approval applied without your click) should be
reported privately; see [SECURITY.md](SECURITY.md).

## License

[MIT](LICENSE)
