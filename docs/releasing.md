# Releasing

Windows only for now. macOS builds run in CI as a non-blocking job but are not
released.

## Versions

The version lives in four places and must match:

- `Cargo.toml` (`[workspace.package] version`, inherited by all crates)
- `package.json` and `package-lock.json`
- `src-tauri/tauri.conf.json`

`npm run check:version` compares them. CI runs it on every push, and the
release workflow runs it again with the tag (`node scripts/check-version.mjs
v0.1.3`), so a tag that doesn't match the files fails the release.

## Steps

1. Bump the version in the four files above, then run `cargo check` so
   `Cargo.lock` picks it up.
2. Move the `[Unreleased]` entries in `CHANGELOG.md` under a new
   `## [x.y.z] - YYYY-MM-DD` heading and add the compare link at the bottom.
   The release notes are taken from that section
   (`scripts/changelog-section.mjs`).
3. Run the checks locally:

   ```bash
   npm run check:version
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   npm test
   npm run check
   ```

4. Commit, then tag and push:

   ```bash
   git tag -a v0.1.3 -m "clawed 0.1.3"
   git push origin main v0.1.3
   ```

5. The `Release` workflow builds the NSIS installer, writes
   `SHA256SUMS.txt`, and creates a **draft** GitHub release. Run the
   fresh-machine test below against the draft's installer, then publish it.

### One-time: tag earlier versions

The changelog's compare links expect tags for the versions released before the
workflow existed:

```bash
git tag -a v0.1.0 13c6279 -m "clawed 0.1.0"
git tag -a v0.1.1 1e5f1e3 -m "clawed 0.1.1"
git tag -a v0.1.2 9d32454 -m "clawed 0.1.2"
```

These commits predate `.github/workflows/release.yml`, and GitHub runs the
workflow file from the tagged commit, so pushing them builds nothing.

## Code signing

The release workflow signs with Authenticode when these repository secrets are
set:

| Secret | Value |
| --- | --- |
| `WINDOWS_CERTIFICATE` | Base64 of the `.pfx` file (`[Convert]::ToBase64String([IO.File]::ReadAllBytes("cert.pfx"))`) |
| `WINDOWS_CERTIFICATE_PASSWORD` | The `.pfx` password |

The workflow imports the certificate and passes its thumbprint to `tauri build`
through a generated `src-tauri/tauri.sign.conf.json` (SHA-256 digest, DigiCert
timestamp server). Without the secrets the installer is unsigned and Windows
SmartScreen shows "Windows protected your PC" on first run; users choose
**More info → Run anyway**.

## Fresh-machine install test

Run on a clean Windows user profile (a new local account, or a VM snapshot)
with Claude Code installed and signed in.

- [ ] Verify the installer hash: `Get-FileHash clawed_*_x64-setup.exe` matches
      `SHA256SUMS.txt`.
- [ ] Install. If unsigned, SmartScreen appears; note the exact wording.
- [ ] First run: the island appears, the tray icon is present, no errors with
      `CLAWED_DEBUG=1`.
- [ ] Launch at login is registered (Task Manager → Startup apps) and the app
      starts after signing out and in.
- [ ] Settings → **Install hooks…** shows a diff of `~/.claude/settings.json`,
      writes a backup next to it, and copies the hook to
      `%LOCALAPPDATA%\clawed\bin`.
- [ ] Start a Claude Code session: the session appears, the step and todo bar
      update, the usage rings show exact numbers (not "estimated").
- [ ] Trigger a permission prompt: the island expands; **Allow** and **Deny**
      both reach Claude Code. Let one time out and confirm Claude Code shows
      its own prompt.
- [ ] Quit clawed with an approval pending: Claude Code falls back to its own
      prompt immediately.
- [ ] With clawed quit, Claude Code runs normally (the hook exits 0 silently).
- [ ] Settings → **Uninstall hooks…** removes only clawed's entries.
- [ ] Uninstall the app (Settings → Apps). If hooks are still installed at this
      point, Claude Code must keep working: `%LOCALAPPDATA%\clawed\bin` holds
      the hook copy, so check whether the uninstaller removes it and whether
      `~/.claude/settings.json` is left pointing at a missing binary.
