# Changelog

All notable changes to clawed are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/). Until 1.0, minor versions may
change behavior.

## [Unreleased]

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

[Unreleased]: https://github.com/joezhuo2/clawed/compare/v0.1.3...HEAD
[0.1.3]: https://github.com/joezhuo2/clawed/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/joezhuo2/clawed/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/joezhuo2/clawed/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/joezhuo2/clawed/releases/tag/v0.1.0
