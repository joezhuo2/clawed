# Changelog

All notable changes to islet (named clawed before 0.2.1) are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/). Until 1.0, minor versions may
change behavior.

## [Unreleased]

## [0.3.0] - 2026-10-02

### Added

- CPU and GPU temperatures in degrees Celsius under the CPU and GPU rings in
  the expanded island, the same way RAM shows used/total memory. Windows reads
  the hottest ACPI thermal zone (PDH `\Thermal Zone Information(*)\Temperature`,
  no admin needed) for CPU and the driver-reported adapter temperature
  (D3DKMT `KMTQAITYPE_ADAPTERPERFDATA`, what Task Manager shows) for GPU.
  macOS reads the SMC / IOHID sensors through `sysinfo`. A sensor that isn't
  available leaves the line blank.

## [0.2.2] - 2026-10-02

### Fixed

- macOS release build failed at codesigning ("code object is not signed at
  all ... In subcomponent: Contents/MacOS/replay"). The Tauri bundler copied
  every bin target of the app crate into the `.app`, including the `replay`
  dev tool, which was never signed. `replay` now lives in its own crate
  (`crates/replay`, package `islet-replay`) and is no longer part of the
  bundle or the Windows install.

### Changed

- Build `replay` with `cargo build --release -p islet-replay` (or run it with
  `cargo run -p islet-replay -- ...`); `npx tauri build` no longer builds it.

## [0.2.1] - 2026-10-02

Renamed from clawed to islet, plus a robustness and branding pass before the
first public release. Details in `docs/robustness.md`.

### Changed

- **Renamed to islet.** "clawed" sounded too close to "Claude"; Anthropic's
  guidelines ask that third-party products keep their own branding and not
  appear to be an Anthropic product. Everything follows the new name: app and
  binaries (`islet`, `islet-hook`), crates, bundle identifier
  (`dev.islet.app`), socket (`islet-<user>`), config and data directories,
  settings backup names and the environment variables (`ISLET_SOCKET`,
  `ISLET_DEBUG`, `ISLET_NO_AUTOSTART`, `ISLET_WEBVIEW_ARGS`,
  `ISLET_APPROVAL_TIMEOUT_MS`).
- Upgrading from clawed: settings are copied from the old `clawed` config
  directory on first launch. Hooks registered by clawed point at the old
  `clawed-hook` and stop reaching the app; Settings opens on first launch, and
  **Install hooks** replaces them (Uninstall also still recognises them).
  Uninstall the old clawed app separately; its launch-at-login entry and
  `clawed/bin` data folder are not removed by islet.

### Added

- App icon: the island pill over three claw scratches, in the island's blue
  and violet, on a dark tile. Drawn by `src-tauri/icons/make_icon.py`
  (standard library only); all sizes regenerated with `npx tauri icon`.
- The non-affiliation notice now also appears at the bottom of the Settings
  window and in the installer metadata (`longDescription`, `copyright`).
- `scripts/soak.ps1`: starts a test instance, replays the fixture in a loop and
  logs memory, handles and threads to a CSV. `replay --fresh-ids` gives each
  run new session ids.

### Fixed

- Install hooks no longer overwrites a `~/.claude/settings.json` whose root is
  not a JSON object, or whose `hooks` (or one of its events) has an
  unexpected type. It refuses with a message instead, and the Install button
  is disabled while Settings shows the error.
- A read-only `settings.json` is reported as such instead of being replaced
  (macOS/Linux) or failing with a generic error (Windows). A failed write
  leaves no temp or backup file behind.
- A symlinked `settings.json` is written through to its target instead of
  being replaced by a regular file.
- islet's own settings file is moved aside to `settings.json.corrupt` when it
  does not parse, instead of being overwritten with defaults on the next save.
- The island is placed again when the primary monitor changes (plugged,
  unplugged, switched, resolution or display scaling changed), so it no
  longer stays off-screen or in the wrong spot.
- Status line `resets_at` given as a float is accepted; other unexpected
  shapes leave the reset time unknown instead of dropping the window.
- Token totals from transcripts saturate instead of overflowing on absurd
  values, and a huge `idle_minutes` can no longer overflow.
- `replay` prints an error instead of panicking on a missing fixture and skips
  lines that are not JSON.

## [0.2.0] - 2026-10-01

### Added

- Session rows show Claude's latest message from the transcript above the
  current tool step, so you can see what it is doing, not just the command.
- Configurable colors under **Colors** in Settings: working, finished,
  errored, approval request, the 5-hour, 7-day, CPU, RAM and GPU rings, the
  context bar, and shared colors for any meter above the warn (70%) and
  critical (90%) thresholds.
- Collapsed pill rings for CPU (inner) / RAM (outer) and GPU, left of the
  plan usage ring. The pill is 260 px wide.

### Changed

- System meters are sampled while the island exists (every 3 s collapsed,
  1.5 s expanded) instead of only while expanded.

## [0.1.5] - 2026-10-01

### Added

- Update notice. Once a day (first check a minute after launch) clawed asks
  GitHub for the latest published release; when it is newer, the tray menu
  shows "Update available: vX.Y.Z…", which opens the release page. Nothing
  is downloaded or installed automatically. Drafts and pre-releases are
  ignored, and only this repository's release pages are opened. Turn it off
  under **Updates** in Settings (`check_updates`).
- `SECURITY.md`: how to report vulnerabilities privately, what is in scope,
  and the fail-safe design (Claude Code's own prompt is the fallback).
- GitHub issue templates: bug report (OS, Claude Code version, how to capture
  `CLAWED_DEBUG` output), feature request, and a link to private
  vulnerability reporting instead of blank issues.

### Fixed

- After installing a new version, the hook copy registered in
  `~/.claude/settings.json` (`%LOCALAPPDATA%\clawed\bin`,
  `~/Library/Application Support/clawed/bin`) stayed at the old version until
  **Install hooks** was run again. clawed now replaces it at launch when it
  differs from the bundled hook; nothing is created if hooks were never
  installed.
- Replacing the hook binary while a hook process is still running (an
  approval waits up to 60 s) no longer fails on Windows: the running file is
  renamed aside first, and restored if the swap fails.

## [0.1.4] - 2026-10-01

macOS support, first pass. All macOS code now type-checks for
`aarch64-apple-darwin` with clippy `-D warnings`, but it has not yet been run on
a Mac; treat macOS builds as preview until the macOS checklist in
`docs/releasing.md` has been run.

### Added

- macOS: the island is a non-activating `NSPanel` (`tauri-nspanel` 2.1) at
  status-bar level, on every Space and over full-screen apps. Clicking it
  never activates clawed or takes focus from your terminal, and it stays out
  of Cmd-Tab and Mission Control cycling.
- macOS: the island is placed from the primary screen's real top inset
  instead of a fixed 26 pt: directly below the menu bar, and directly under
  the notch on notched MacBooks (where the menu bar is as tall as the notch),
  including when the menu bar auto-hides.
- macOS: GPU ring, from IOKit's `IOAccelerator` `PerformanceStatistics`
  ("Device Utilization %", the value Activity Monitor uses), taking the
  busiest GPU. Works on Apple silicon and Intel/AMD Macs.
- macOS: `LSUIElement` in `Info.plist`, so no Dock icon appears at launch.
- Release workflow builds dmgs for Apple silicon and Intel next to the
  Windows installer, with one `SHA256SUMS.txt` for all files. Developer ID
  signing and notarization run when the Apple secrets are configured;
  otherwise the app is ad-hoc signed.
- `scripts/measure-memory.sh`: macOS memory measurement (clawed plus its
  WebKit processes).

### Changed

- macOS uses the system TLS stack (Security.framework, keychain roots) for
  the usage endpoint. Windows keeps rustls. This also means no C TLS library
  is cross-compiled for macOS.
- The macOS CI job is now blocking.
- Island creation and teardown run on the main thread on macOS, as AppKit
  requires.

## [0.1.3] - 2026-10-01

First version intended for a public GitHub release.

### Security

- The Windows named pipe now only grants access to the current user: the
  server pipe is created with a protected DACL for the user's SID, together
  with `FILE_FLAG_FIRST_PIPE_INSTANCE` and `PIPE_REJECT_REMOTE_CLIENTS`. If
  another process already owns the pipe name, clawed refuses to start its
  server instead of sharing the name.
- `clawed-hook` checks that the process serving the socket runs as the same
  user before sending anything. If it doesn't (someone squatting the name),
  the hook exits silently and Claude Code shows its own permission prompt.
- The app drops connections from clients running as another user.

### Added

- Plan usage from your Claude account's usage endpoint (the numbers `/usage`
  shows), using the OAuth token Claude Code stores in
  `~/.claude/.credentials.json`. Polled at most every 2 minutes, sent only to
  api.anthropic.com, never refreshed by clawed. The status line and the local
  estimate remain as fallbacks.
- GitHub Actions CI: clippy (`-D warnings`), `cargo test --workspace`,
  `npm test` and `npm run check` on Windows; macOS runs as a non-blocking job.
- Tag-triggered release workflow that builds the Windows NSIS installer,
  writes `SHA256SUMS.txt`, and creates a draft GitHub release with notes from
  this file. Authenticode signing runs when a certificate is configured as a
  repository secret.
- `npm run check:version` (`scripts/check-version.mjs`) fails when
  `Cargo.toml`, `package.json`, `package-lock.json` and `tauri.conf.json`
  disagree, or when a release tag doesn't match them.
- `docs/releasing.md`: release steps and the fresh-machine install test.
- This changelog.

### Fixed

- Elapsed time on finished sessions (green dots) no longer keeps ticking.
  Sessions now track per-turn start and end, so a finished row shows the
  duration of its last turn.
- Usage rings were far off (for example 3% shown while the account was at
  88%) because the estimate divided local token counts by a fixed cap. The
  account endpoint now provides exact percentages.

## [0.1.2] - 2026-09-30

### Fixed

- When the status line goes stale and usage falls back to the estimate, the
  last known reset times stay visible until they pass.

## [0.1.1] - 2026-09-30

### Changed

- The expanded island shows all five rings (5-hour, 7-day, CPU, RAM, GPU) in
  one row.

## [0.1.0] - 2026-09-30

Initial version, built from source only.

### Added

- Always-on-top island with a collapsed pill and an expanded view: per-session
  state, current step, todo progress, context meter, files touched, elapsed
  time.
- Permission approvals from the island (Allow / Deny), with Claude Code's own
  prompt as the fallback after 60 s.
- 5-hour and 7-day plan usage rings, CPU, RAM and GPU rings.
- `clawed-hook` sidecar, hook installer with diff preview and backup, tray
  menu, launch at login, low memory mode.
- `replay` tool and synthetic fixtures.

[Unreleased]: https://github.com/joezhuo2/islet/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/joezhuo2/islet/compare/v0.2.2...v0.3.0
[0.2.2]: https://github.com/joezhuo2/islet/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/joezhuo2/islet/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/joezhuo2/islet/compare/v0.1.5...v0.2.0
[0.1.5]: https://github.com/joezhuo2/islet/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/joezhuo2/islet/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/joezhuo2/islet/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/joezhuo2/islet/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/joezhuo2/islet/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/joezhuo2/islet/releases/tag/v0.1.0
