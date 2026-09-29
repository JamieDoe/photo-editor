# ADR 0011: Application lifecycle: single instance, window state, safe quit

- Status: Accepted (Phase 1)
- Date: 2026-09-28

## Context

Phase 1 covers the application lifecycle. Background exports must not be cut off
mid-write, and two app instances would contend for the Phase 2 catalogue.

## Decision

- **Single instance** (`tauri-plugin-single-instance`, registered first): a second
  launch focuses the existing window and exits (verified: returns in ~0.1 s).
- **Window state** (`tauri-plugin-window-state`): size, position and maximised state
  are restored.
- **Safe quit:** the shell tracks a cancel token per running export.
  - A window close or user-initiated quit (Cmd+Q) with exports running is held.
    The UI receives `app://quit-requested` and asks "Keep working" / "Cancel export
    and quit".
  - Confirming runs `lifecycle::shutdown`: cancel the exports, wait up to 5 s for
    them to stop (cancellation is cooperative), then exit.
  - Programmatic exits, including the self-test's, use the same routine.
- Shutdown is logged; a failure to start is logged before exiting with an error code.

## Verification

The self-test starts an export, requests a window close and checks it was held. The
final shutdown then cancels the export. Runs on all six camera samples exit with
code 0 and leave no temporary files.

## Consequences

- Exports never leave partial files: writes are atomic, and quitting waits for the
  cancelled job.
- A decode that ignores cancellation for more than 5 s delays quitting by at most
  that long, and a warning is logged.
- The plugins are official Tauri plugins (MIT/Apache-2.0), used from Rust only. The
  webview gains no permissions.
