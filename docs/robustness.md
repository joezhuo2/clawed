# Robustness

What happens when inputs are bad or the environment changes underneath islet.
Reviewed for v0.2.1. The rule throughout: degrade to "unknown" or Claude Code's
own behavior, never crash and never destroy a user's file.

Release builds use `panic = "abort"`, so a panic anywhere, including inside a
tokio task, ends the whole app. That is why panics matter here.

## Panics (`unwrap` / `expect`)

Of the roughly 90 `unwrap()` / `expect()` calls under `src-tauri/src`, all but
three are in `#[cfg(test)]` code. Those three, plus the one `unreachable!`:

| Where | Why it is safe |
| --- | --- |
| `lib.rs` `install_runtime` (2) | Building a tokio runtime and spawning its thread at startup; failure means the process cannot run at all. |
| `lib.rs` `tauri::Builder::build` | Same: no window system, nothing to do. |
| `installer.rs` `ensure_object` | `unreachable!` on a value that was just made an object (was an `unwrap`). |

`replay.rs` (a dev tool) now prints an error instead of panicking on a missing
fixture or a non-JSON line. Mutexes go through `state::lock`, which recovers
from poisoning. Token arithmetic on transcript numbers saturates, and
`idle_minutes` is multiplied with `saturating_mul`, so absurd values give wrong
numbers rather than overflow. Byte-indexed slicing was checked: every index
follows a check on the same ASCII byte, so it lands on a char boundary.

`crates/hook` and `crates/proto` contain no `unwrap` / `expect` outside tests.
The hook exits 0 with no output on any failure (see `crates/hook/src/main.rs`).

## Claude Code's `settings.json`

| Situation | Install / Uninstall hooks |
| --- | --- |
| Missing, or the `.claude` directory missing | Treated as `{}`; the directory is created on install. No backup (nothing to back up). |
| Empty or whitespace only | Treated as `{}`. |
| Invalid JSON | Settings shows the parse error; Install is disabled and `installer_apply` refuses. The file is not touched. |
| Valid JSON but not an object (`[]`, `null`, a string) | Same as invalid. Before 0.2.1 install replaced it with `{}`. |
| `hooks` is not an object, or one of our events is not a list | Install refuses with a message naming the key, instead of replacing it. Uninstall leaves values it does not understand alone. |
| Read-only | Refused with "… is read-only"; the file is unchanged and no backup or temp file is left. (On macOS/Linux a rename would otherwise have replaced a read-only file silently.) |
| Symlink (dotfile managers) | Written through to the target; the link stays a link. |
| Entries from clawed (the pre-rename name) | `clawed-hook` commands count as ours: Install replaces them, Uninstall removes them. |
| Write fails halfway (disk full, permission) | The temp file and the new backup are removed; the original is untouched because it is only ever replaced by an atomic rename. |

islet's own settings (`%APPDATA%\islet\settings.json`,
`~/Library/Application Support/islet/settings.json`): missing gives defaults; a
file that fails to parse (bad JSON or wrong types) is renamed to
`settings.json.corrupt` and defaults are used, so the next save does not
silently overwrite it. If the OS config or data directory cannot be found, the
temp directory is used instead.

Tests: `installer::tests::*`, `settings::tests::corrupt_file_kept_aside`.

## Hook payload and status line schema changes

The hook reads Claude Code's JSON as untyped `serde_json::Value` and copies
only the fields it knows (`crates/proto/src/strip.rs`):

- Unknown fields are ignored. Unknown event names are forwarded as-is; the
  store only refreshes that session's last-seen time for kinds it does not
  handle.
- A missing or non-string `session_id` or `hook_event_name` drops the event;
  the hook still exits 0.
- A field of the wrong type (`cwd` as a number, `tool_input` as a string) is
  treated as absent.
- Status line without `rate_limits`, or with a window missing
  `used_percentage`: that window is absent, the usage rings fall back to the
  OAuth usage endpoint, then to the local estimate ("estimated"), then to
  "unknown".
- `resets_at` may be an integer or a float; anything else (an ISO string, a
  negative number) means "reset time unknown" rather than dropping the window.
- The wire format between hook and app is versioned (`PROTO_VERSION`).
  Messages with another version are dropped, and serde ignores unknown wire
  fields, so a newer hook talking to an older app (or the reverse during an
  update) loses data instead of crashing.

Tests: `strip::tests::status_tolerates_changed_shapes`,
`status_without_rate_limits_or_context`, `event_tolerates_changed_shapes`.

## Monitors and DPI

The island is placed top-center on the primary monitor when it is created.
While it exists, the hover poll checks the primary monitor's position, size and
scale factor about every 2 seconds and places the island again when any of
them changes. That covers:

- plugging in or unplugging a monitor, including the one the island was on,
- changing which monitor is primary,
- changing resolution or display scaling (DPI),
- on macOS, the menu bar / notch inset of the new primary screen.

With no monitor at all (lid closed, all displays off) the check waits and
places the island when one comes back. Window sizes are in logical pixels, so
a DPI change keeps the island the same apparent size.

Not automated: needs a manual pass with real hardware (see the fresh-machine
checklist in `docs/releasing.md`).

## Approvals: timeouts and quitting

Claude Code always falls back to its own permission prompt when islet does not
answer. The layers, from the inside out:

1. The island answers: the hook prints the decision.
2. Nobody answers within 60 s: the hook's watchdog exits 0 with no output.
3. The app holds the connection at most 62 s, then forgets the request.
4. Claude Code's own hook timeout is 65 s (`APPROVAL_TIMEOUT_SECS`), above the
   hook's 60 s, so the hook always exits first.
5. Quit from the tray: every pending request is released (`release_approvals`
   on `RunEvent::Exit`). If the process exits before the release is written,
   the socket closes, which the hook treats the same way.
6. Crash or kill: the socket closes; the hook reads EOF and exits 0 at once.
7. App not running at all: connecting fails and the hook exits 0 immediately.
8. A reply with the wrong id, a `Release`, or unparseable bytes: no output.

In every case except 1 the hook prints nothing, which Claude Code reads as
"no decision", and it shows its own prompt.

Tests: `crates/hook/tests/hook.rs` (`approval_timeout_prints_nothing`,
`approval_app_gone_prints_nothing_quickly`, `approval_release_prints_nothing`,
`approval_wrong_id_prints_nothing`, `app_absent_exits_zero_fast`) and
`ipc::tests` (`release_all_unblocks`, `hook_disconnect_forgets_approval`).
