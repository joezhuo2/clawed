# Security policy

islet answers Claude Code permission prompts. A bug that lets a tool call be
allowed without your click, or lets another user or machine send decisions to
your Claude Code sessions, is a security issue. Please report it privately.

## Reporting a vulnerability

Use GitHub's private vulnerability reporting:
[Report a vulnerability](https://github.com/joezhuo2/islet/security/advisories/new)
(the **Security** tab of the repository, then **Report a vulnerability**).

Please don't open a public issue, discussion or pull request for a
vulnerability before a fix is released.

Include what you can of:

- islet version, OS and version, Claude Code version
- what an attacker needs (same user, another local user, network, a crafted
  transcript or settings file, ...)
- steps or a proof of concept, and what happens versus what should happen
- `ISLET_DEBUG` output, if it helps (see the bug report template for how to
  capture it). It contains your prompts, paths and tool inputs, so remove
  anything private first.

This is a one-person project. You can expect an acknowledgement within 7 days
and an assessment within 14 days. Fixes for confirmed issues ship in a new
release, credited to you in `CHANGELOG.md` and the advisory unless you'd rather
stay anonymous.

## Supported versions

islet is pre-1.0. Only the latest release gets security fixes; please check
that the issue still exists there.

## What counts

In scope, for example:

- A tool call is allowed (or denied) without the user choosing it in islet,
  or a decision is applied to a different session, tool call or input than
  the one shown on the approval card.
- The approval card shows something other than what Claude Code will run
  (truncation, escaping or Unicode tricks that hide part of a command).
- The local socket or named pipe can be reached by another user account or
  from the network, or the hook talks to a server run by another user.
- A crafted transcript, hook payload, status line JSON or `settings.json`
  makes the app or the hook crash in a way that blocks Claude Code, or run
  code.
- **Install hooks** writes anything other than the diff it showed, or
  registers a hook path another user can write to.
- The Claude Code OAuth token is sent anywhere other than api.anthropic.com,
  logged, or written to disk by islet.
- The update notice opens anything other than this repository's release pages.

Out of scope:

- Processes already running as your user. They can connect to the local
  socket, but they can also edit `~/.claude/settings.json` directly (see
  "Limitations" in the README).
- SmartScreen or Gatekeeper warnings on unsigned builds.
- Claude Code's own permission system, hooks API or the Anthropic API; report
  those to Anthropic.

## Design notes for reviewers

- If islet isn't running, or doesn't answer an approval within 60 seconds,
  the hook exits 0 with no output and Claude Code shows its own prompt. The
  hook is meant to fail open to Claude Code's normal behavior, never to an
  automatic allow.
- Pausing approvals or quitting islet releases every pending approval back to
  Claude Code.
- On Windows the named pipe has a DACL for the current user only, rejects
  remote clients, is created with `FILE_FLAG_FIRST_PIPE_INSTANCE`, and both
  sides check that the peer runs as the same user. On macOS the socket lives
  in the per-user temp directory (`$TMPDIR`, mode 0700).
- File contents from Write/Edit tool calls are stripped by the hook and never
  reach the app.
