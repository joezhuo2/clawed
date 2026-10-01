# Memory measurements

Windows 11, release build (`npx tauri build --no-bundle`), 2880x1800 at 200% scale.
Measured with `scripts/measure-memory.ps1`, which sums `clawed.exe` and every
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
120 MB private. Override with the `CLAWED_WEBVIEW_ARGS` environment variable,
which replaces the whole argument string.

## Reproduce

```bash
export CLAWED_SOCKET=clawed-measure CLAWED_NO_AUTOSTART=1
./target/release/clawed.exe --autostarted &   # torn down: no window until a hook event
powershell -File scripts/measure-memory.ps1 -Label torn-down
./target/release/replay.exe fixtures/sample-session.jsonl &
powershell -File scripts/measure-memory.ps1 -Label busy
```

`CLAWED_SOCKET` keeps real Claude Code sessions from waking the test instance.

## macOS

Not measured yet. On a Mac, with a release build (`npx tauri build --bundles
app`):

```bash
export CLAWED_SOCKET=clawed-measure CLAWED_NO_AUTOSTART=1
./target/release/bundle/macos/clawed.app/Contents/MacOS/clawed --autostarted &
sh scripts/measure-memory.sh torn-down
./target/release/replay fixtures/sample-session.jsonl &
sh scripts/measure-memory.sh busy
```

`measure-memory.sh` sums RSS of `clawed` and the WebKit processes
(`com.apple.WebKit.WebContent`, `Networking`, `GPU`) started after it, since
WebKit's XPC services are children of launchd rather than of clawed. Keep
Safari and other WebKit apps closed while measuring. It also prints each
process's `phys_footprint`, the closest equivalent of private bytes; record
both in a table like the Windows one above.

## Not yet done

- Multi-hour soak test (replay in a loop, confirm flat memory).
- macOS numbers (procedure above).
