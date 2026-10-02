# Before first public release
## To Test

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
- [ ] Multi-hour soak test (replay in a loop, confirm flat memory). 0.2.1:
      `scripts/soak.ps1` written (`docs/memory.md`). Remaining: run it for
      4 h with no other islet running and record the result.
- [ ] Multi-monitor, DPI scaling changes, and monitor unplug while the island
      is shown. 0.2.1: the island is placed again when the primary monitor
      changes. Remaining: the manual monitor pass in `docs/releasing.md`.

## To Do
- [ ] Final tray glyphs (idle / working / approval).
- [ ] Screenshot or short GIF of the pill, expanded view and approval card for
      the README.