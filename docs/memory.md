# Memory measurements

Windows 11, release build (`npx tauri build --no-bundle`), 2880x1800 at 200% scale.
Measured with `scripts/measure-memory.ps1`, which sums `islet.exe` and every
descendant process (the WebView2 browser, GPU, renderer and utility processes).

Working set counts shared pages, so it overstates the real cost; private bytes is
the better number to compare.

## Results (2026-09-30)

| State | Processes | Working set | Private |
| --- | --- | --- | --- |
| Torn down (low memory mode, tray only) | 1 | 17 MB | 4 MB |
| Island visible, idle | 7 | 307 MB | 101 MB |
| Busy: 2 sessions, 2 approvals, expanded with system meters | 7 | 396 MB | 125 MB |

The Rust backend alone is 27 to 40 MB working set and 7 to 20 MB private.

## WebView2 flags

Island visible and idle, same build, varying only `additionalBrowserArgs`
(Tauri's default `--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection` kept in all rows):

| Extra flags | Processes | Working set | Private | Renders correctly |
| --- | --- | --- | --- | --- |
| none (Tauri default) | 7 | 371 MB | 174 MB | yes |
| `--in-process-gpu` | 6 | 308 MB | 122 MB | yes |
| `--disable-gpu` (shipped) | 7 | 307 MB | 100 MB | yes |
| `--disable-gpu --disable-gpu-compositing` | 7 | 308 MB | 100 MB | yes |

`--disable-gpu` is the default. Before it, the GPU process alone was about
120 MB private. Override with the `ISLET_WEBVIEW_ARGS` environment variable,
which replaces the whole argument string.

## Reproduce

Build `replay` first: `cargo build --release -p islet-replay`.

```bash
export ISLET_SOCKET=islet-measure ISLET_NO_AUTOSTART=1
./target/release/islet.exe --autostarted &   # torn down: no window until a hook event
powershell -File scripts/measure-memory.ps1 -Label torn-down
./target/release/replay.exe fixtures/sample-session.jsonl &
powershell -File scripts/measure-memory.ps1 -Label busy
```

`ISLET_SOCKET` keeps real Claude Code sessions from waking the test instance.

## macOS

Not measured yet. On a Mac, with a release build (`npx tauri build --bundles
app`, plus `cargo build --release -p islet-replay`):

```bash
export ISLET_SOCKET=islet-measure ISLET_NO_AUTOSTART=1
./target/release/bundle/macos/islet.app/Contents/MacOS/islet --autostarted &
sh scripts/measure-memory.sh torn-down
./target/release/replay fixtures/sample-session.jsonl &
sh scripts/measure-memory.sh busy
```

`measure-memory.sh` sums RSS of `islet` and the WebKit processes
(`com.apple.WebKit.WebContent`, `Networking`, `GPU`) started after it, since
WebKit's XPC services are children of launchd rather than of islet. Keep
Safari and other WebKit apps closed while measuring. It also prints each
process's `phys_footprint`, the closest equivalent of private bytes; record
both in a table like the Windows one above.

## Soak test

`scripts/soak.ps1` starts a release build on its own socket (`islet-soak`),
replays `fixtures/sample-session.jsonl` in a loop with fresh session ids each
time (`replay --fresh-ids`), samples memory, handles and threads every few
minutes into a CSV, and stops the instance at the end. Each loop also lets the
fixture's approval run into the 62 s server timeout, so that path is exercised
too.

```powershell
npx tauri build --no-bundle
cargo build --release -p islet-replay
powershell -File scripts/soak.ps1 -Hours 4
```

Quit any other islet first; the single-instance guard would otherwise hand the
launch to it. Pass: total private bytes, backend private bytes and handle count
over the last hour within a few MB (handles: a few dozen) of the first hour.

## Not yet done

- Multi-hour soak run. The script exists (above); no run recorded yet.
- macOS numbers (procedure above).
