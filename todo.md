# Before first public release

Items gathered from the README, `docs/design.md`, `docs/memory.md` and the repo
state as of v0.1.2. Ordered roughly by priority within each section.

- instead of only showing the command used by claude, also show its latest actual instruction, so that users know what claude is doing

## Blockers

- [ ] **Prebuilt release artifacts.** No GitHub release exists yet. Produce a
      signed NSIS installer for Windows (and a dmg once macOS works), with
      checksums. 0.1.3: `release.yml` builds the NSIS installer +
      `SHA256SUMS.txt` into a draft release, signing when the
      `WINDOWS_CERTIFICATE` secret is set. Remaining: push the tag, get a
      certificate (or ship unsigned), publish the draft.
- [ ] **Version and tag hygiene.** Versions are 0.1.2 in `Cargo.toml`,
      `package.json` and `tauri.conf.json`, but there are no git tags. Pick the
      first public version, keep the three in sync, and tag it.
      0.1.3 chosen; `npm run check:version` enforces sync in CI and against the
      release tag. Remaining: commit and create the tags (`docs/releasing.md`).
- [ ] **Fresh-machine install test (Windows).** Install from the built
      installer on a clean user profile: first run, autostart registration,
      Install hooks diff + backup, approvals end to end, Uninstall hooks,
      app uninstall leaves no hooks pointing at a missing binary.
      Checklist written in `docs/releasing.md`; not run yet.

## Platform

- [ ] **macOS: build and test at all.** Code paths exist but have never been
      compiled or run.
- [ ] macOS: `tauri-nspanel` non-activating panel (design calls for it; not in
      `Cargo.toml` yet), placement around the notch / menu bar.
- [ ] macOS: GPU ring (currently `--`).
- [ ] macOS: notarization and signing (Apple Developer ID).
- [ ] macOS memory numbers in `docs/memory.md` (WebKit content process).

## Distribution

- [ ] Windows code signing certificate, or document the SmartScreen warning
      for unsigned builds. (SmartScreen warning documented in README; no
      certificate yet.)
- [ ] Auto-update: decide whether to add `tauri-plugin-updater` now or ship
      without it and document manual updates. The hook binary is already copied
      to a stable per-user path, so updates should not break installed hooks;
      verify that.

## Branding

- [ ] Real app icon. `src-tauri/icons` are placeholders from
      `make_placeholder.py` (white capsule on gray). Regenerate all sizes with
      `npx tauri icon`.
- [ ] Final tray glyphs (idle / working / approval).
- [ ] Screenshot or short GIF of the pill, expanded view and approval card for
      the README.
- [ ] Check name and wording against Anthropic trademark guidance ("clawed",
      "for Claude Code"); keep the non-affiliation notice.

## Robustness

- [ ] Multi-hour soak test (replay in a loop, confirm flat memory) — listed
      under "Not yet done" in `docs/memory.md`.
- [ ] Audit the ~77 `unwrap()` / `expect()` calls in `src-tauri/src` outside
      tests; make sure none can panic on bad input (malformed settings.json,
      transcript lines, status line JSON, missing app data dir).
- [ ] Behavior when `~/.claude/settings.json` is missing, invalid JSON, or
      read-only during Install / Uninstall hooks.
- [ ] Behavior when Claude Code changes its hook payload or status line schema
      (unknown fields, missing `rate_limits`): degrade to "estimated"/unknown,
      never crash.
- [ ] Multi-monitor, DPI scaling changes, and monitor unplug while the island
      is shown.
- [ ] Approval timeout (60 s) and app quit with pending approvals: confirm
      Claude Code always falls back to its own prompt.

## Docs and repo

- [ ] Mark the first release scope in `docs/design.md` (what shipped vs. out of
      scope).
- [ ] Issue templates (bug report with OS, Claude Code version, `CLAWED_DEBUG`
      output).
- [ ] SECURITY.md describing how to report issues (relevant because the app
      answers permission prompts).
