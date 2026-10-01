# Before first public release

Items gathered from the README, `docs/design.md`, `docs/memory.md` and the repo
state as of v0.1.2. Ordered roughly by priority within each section.

## Blockers

- [ ] **Prebuilt release artifacts.** No GitHub release exists yet. Produce a
      signed NSIS installer for Windows (and a dmg once macOS works), with
      checksums. 0.1.3: `release.yml` builds the NSIS installer +
      `SHA256SUMS.txt` into a draft release, signing when the
      `WINDOWS_CERTIFICATE` secret is set. Remaining: push the tag, get a
      certificate (or ship unsigned), publish the draft. 0.1.4: the same
      workflow also builds Apple silicon and Intel dmgs into the draft, with
      one `SHA256SUMS.txt` covering every file.
- [ ] **Version and tag hygiene.** Versions are 0.1.2 in `Cargo.toml`,
      `package.json` and `tauri.conf.json`, but there are no git tags. Pick the
      first public version, keep the three in sync, and tag it.
      0.1.3 chosen; `npm run check:version` enforces sync in CI and against the
      release tag. Remaining: commit and create the tags (`docs/releasing.md`).
- [ ] **Fresh-machine install test (Windows).** Install from the built
      installer on a clean user profile: first run, autostart registration,
      Install hooks diff + backup, approvals end to end, Uninstall hooks,
      app uninstall leaves no hooks pointing at a missing binary.
      Checklist written in `docs/releasing.md`; not run yet. 0.1.5 adds the
      update-over-previous-version and update notice steps.
- [ ] **macOS: build and test at all.** 0.1.4: the whole workspace passes
      `cargo clippy --target aarch64-apple-darwin -D warnings` (stub C
      compiler, see `docs/releasing.md`), and the macOS CI job is blocking.
      Remaining: first green CI run, then run the macOS fresh-machine
      checklist on real hardware.
- [ ] macOS: notarization and signing (Apple Developer ID). 0.1.4: the release
      workflow signs and notarizes when the Apple secrets are set and ad-hoc
      signs otherwise. Remaining: enroll in the Apple Developer Program and
      add the secrets.
- [ ] macOS memory numbers in `docs/memory.md`. 0.1.4: procedure and
      `scripts/measure-memory.sh` written; needs a Mac to run.

## Branding

- [ ] Real app icon. `src-tauri/icons` are placeholders from
      `make_placeholder.py` (white capsule on gray). Regenerate all sizes with
      `npx tauri icon`.
- [ ] Check name and wording against Anthropic trademark guidance ("clawed",
      "for Claude Code"); keep the non-affiliation notice.

- [ ] Final tray glyphs (idle / working / approval).
- [ ] Screenshot or short GIF of the pill, expanded view and approval card for
      the README.

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
