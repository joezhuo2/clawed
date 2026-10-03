# Releasing

Each release ships a Windows NSIS installer and two macOS dmgs (Apple silicon
and Intel). macOS builds are a preview until the macOS checklist below has
been run on a real Mac.

## Versions

The version lives in four places and must match:

- `Cargo.toml` (`[workspace.package] version`, inherited by all crates)
- `package.json` and `package-lock.json`
- `src-tauri/tauri.conf.json`

`npm run check:version` compares them. CI runs it on every push, and the
release workflow runs it again with the tag (`node scripts/check-version.mjs
v0.3.1`), so a tag that doesn't match the files fails the release.

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
   git tag -a v0.3.1 -m "islet 0.3.1"
   git push origin main v0.3.1
   ```

5. The `Release` workflow builds the NSIS installer and both dmgs, writes
   one `SHA256SUMS.txt` for all of them, and creates a **draft** GitHub
   release. Run the fresh-machine tests below against the draft's files, then
   publish it. Installed apps only see the release once it is published and
   not marked as a pre-release: the update notice reads GitHub's
   `releases/latest`, which skips drafts and pre-releases. Mark a release as
   a pre-release to test it without notifying users.

### Checking macOS from Windows

The macOS code can be type-checked without a Mac. Build scripts for `ring`
and `objc2-exception-helper` want a C/Objective-C compiler for the target;
for `cargo check`/`clippy` nothing is linked, so a stub compiler that writes
empty object files is enough:

```bash
rustup target add aarch64-apple-darwin
touch src-tauri/binaries/islet-hook-aarch64-apple-darwin   # sidecar placeholder
CC_aarch64_apple_darwin=/path/to/stub-cc AR_aarch64_apple_darwin=/path/to/stub-ar \
  cargo clippy --workspace --all-targets --target aarch64-apple-darwin -- -D warnings
```

The stub only needs to create the file after `-o` (compiler) or the `.a`
argument (archiver). This catches type and API errors, not runtime
behavior; the CI macOS job builds, links and runs the tests for real.

### One-time: tag earlier versions

The changelog's compare links expect tags for the versions released before the
workflow existed:

```bash
git tag -a v0.1.0 13c6279 -m "islet 0.1.0"
git tag -a v0.1.1 1e5f1e3 -m "islet 0.1.1"
git tag -a v0.1.2 9d32454 -m "islet 0.1.2"
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

Current decision (0.1.5): ship unsigned and rely on `SHA256SUMS.txt` plus the
README's SmartScreen instructions. Revisit when a certificate (OV, or a cloud
signing service such as Azure Trusted Signing) is worth the cost; the
workflow only needs the two secrets above for a `.pfx`.

## Updates

There is no in-app updater (`tauri-plugin-updater` is not used: it would need
its own signing key and update manifest, and unsigned Windows installers would
still hit SmartScreen). Instead the app checks
`https://api.github.com/repos/joezhuo2/islet/releases/latest` a minute after
launch and then daily, and shows a tray item linking to the release page when
the tag is a newer `vX.Y.Z`. Users install over the old version.

Installed hooks survive updates: `~/.claude/settings.json` points at the copy
in `%LOCALAPPDATA%\islet\bin` (macOS: `~/Library/Application
Support/islet/bin`), and at launch the app replaces that copy when it differs
from the hook bundled with the new version. A copy held open by a running
hook is renamed aside first; if the swap still fails it is retried on the
next launch.

### macOS signing and notarization

`tauri build` signs with a Developer ID and notarizes when these repository
secrets are set:

| Secret | Value |
| --- | --- |
| `APPLE_CERTIFICATE` | Base64 of the exported "Developer ID Application" `.p12` (`base64 -i cert.p12`) |
| `APPLE_CERTIFICATE_PASSWORD` | The `.p12` password |
| `APPLE_SIGNING_IDENTITY` | e.g. `Developer ID Application: Your Name (TEAMID)` |
| `APPLE_ID` | Apple ID email used for notarization |
| `APPLE_PASSWORD` | An app-specific password for that Apple ID |
| `APPLE_TEAM_ID` | The 10-character team ID |

Notarization runs only when a certificate is set too. Without a certificate
the workflow ad-hoc signs (`APPLE_SIGNING_IDENTITY=-`), which Apple silicon
needs to run the binary at all; Gatekeeper then blocks the first launch until
the user right-clicks → **Open**. Both `islet` and the `islet-hook` sidecar
are signed by the bundler.

## Fresh-machine install test (Windows)

Run on a clean Windows user profile (a new local account, or a VM snapshot)
with Claude Code installed and signed in.

- [ ] Verify the installer hash: `Get-FileHash islet_*_x64-setup.exe` matches
      `SHA256SUMS.txt`.
- [ ] Install. If unsigned, SmartScreen appears; note the exact wording.
- [ ] First run: the island appears, the tray icon is present, no errors with
      `ISLET_DEBUG=1`.
- [ ] Launch at login is registered (Task Manager → Startup apps) and the app
      starts after signing out and in.
- [ ] Settings → **Install hooks…** shows a diff of `~/.claude/settings.json`,
      writes a backup next to it, and copies the hook to
      `%LOCALAPPDATA%\islet\bin`.
- [ ] Start a Claude Code session: the session appears, the step and todo bar
      update, the usage rings show exact numbers (not "estimated").
- [ ] Trigger a permission prompt: the island expands; **Allow** and **Deny**
      both reach Claude Code. Let one time out and confirm Claude Code shows
      its own prompt.
- [ ] Quit islet with an approval pending: Claude Code falls back to its own
      prompt immediately.
- [ ] With islet quit, Claude Code runs normally (the hook exits 0 silently).
- [ ] Update: with hooks installed from the previous release, install this
      one over it without reinstalling hooks. After launch,
      `%LOCALAPPDATA%\islet\bin\islet-hook.exe` has the new file's date
      and size, and sessions and approvals still work.
- [ ] Update notice: while a newer release is published, the tray menu shows
      **Update available** and it opens the release page. With **Updates**
      off in Settings it doesn't appear after the next check.
- [ ] Settings → **Uninstall hooks…** removes only islet's entries.
- [ ] Bad `settings.json` (see `docs/robustness.md`): with the file set to
      invalid JSON, Settings shows the error and **Install hooks…** is
      disabled; with it marked read-only, Install reports "read-only" and the
      file is unchanged. Restore the file afterwards.
- [ ] Monitors, with the island shown: change display scaling (100% → 150%),
      switch the primary display, unplug the display the island is on, plug
      it back in. Within about 2 seconds of each change the pill is
      top-center on the primary display at the right size.
- [ ] Uninstall the app (Settings → Apps). If hooks are still installed at this
      point, Claude Code must keep working: `%LOCALAPPDATA%\islet\bin` holds
      the hook copy, so check whether the uninstaller removes it and whether
      `~/.claude/settings.json` is left pointing at a missing binary.

## Fresh-machine install test (macOS)

Run on a clean macOS user account (or a VM), once on Apple silicon and, if
possible, once on Intel, with Claude Code installed and signed in. Run the
app from a terminal with `ISLET_DEBUG=1` the first time
(`/Applications/islet.app/Contents/MacOS/islet`).

- [ ] `shasum -a 256` of the dmg matches `SHA256SUMS.txt`.
- [ ] Gatekeeper: unsigned builds need right-click → **Open**; signed and
      notarized builds open without a prompt. Note the exact wording.
- [ ] No Dock icon, at launch or later. The menu bar icon is a template image
      (follows light/dark menu bar).
- [ ] The pill sits centered directly below the menu bar. On a notched
      MacBook it hangs directly under the notch, not behind it. With "Automatically
      hide and show the menu bar" on, it still clears the notch.
- [ ] The island shows on every Space and over a full-screen app, and is not in
      Cmd-Tab or Mission Control.
- [ ] Clicking Allow / Deny on an approval does not take focus from the
      terminal (keystrokes still go to the terminal afterwards).
- [ ] Hover expands the island; leaving collapses it and clicks pass through
      the transparent margins.
- [ ] GPU ring shows a number, roughly matching Activity Monitor → Window →
      GPU History while a GPU load runs.
- [ ] CPU and GPU rings show a temperature line (°C) that rises under load.
- [ ] Usage rings show exact numbers (the usage request works with the system
      TLS stack).
- [ ] Launch at login: a LaunchAgent is registered and islet starts after
      logging out and in.
- [ ] Low memory mode: the island is torn down after 3 minutes idle and comes
      back on the next hook event, with no crash (panel to window conversion).
- [ ] Low memory mode off: the island shows at once, stays up while idle,
      and shows on an autostarted launch.
- [ ] Install / Uninstall hooks as in the Windows list; the hook is copied to
      `~/Library/Application Support/islet/bin`.
- [ ] Unplug / switch the primary display and change its resolution while the
      island is shown: within about 2 seconds the pill is back below the menu
      bar of the new primary display.
- [ ] Record memory with `scripts/measure-memory.sh` in `docs/memory.md`.
