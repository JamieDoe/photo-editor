# ADR 0008: Local logging and error reporting

- Status: Accepted (Phase 1)
- Date: 2026-09-28

## Context

PRODUCT.md §41 says a photographer must never see panics or decoder error codes;
technical detail belongs in logs. CLAUDE.md §25 forbids silently adding analytics or
external services. Phase 1 needs logging and error reporting that work offline and
help diagnose problems a user reports.

## Decision

1. **`log` facade everywhere, one sink in the shell.** Engine crates log through the
   `log` crate and stay independent of Tauri. The desktop shell installs
   `tauri-plugin-log`, configured as follows:
   - Log file: `photo-editor.log` in the OS log directory
     (macOS `~/Library/Logs/<identifier>/`).
   - Rotation at 5 MB, keeping 5 files (about 25 MB maximum).
   - Levels: info in release builds, debug in development builds (which also log to
     stdout).
2. **Error references.** Every non-cancellation error sent to the UI gets a short
   reference (e.g. `E-M2P8J-1`). The same reference is written to the log line that
   carries the technical detail.
   - Expected failures (unsupported file, bad destination) log as warnings; internal
     and export failures log as errors.
   - The UI shows the photographer-facing message plus the reference.
3. **UI errors are logged by Rust.** Three sources reach `report_client_error`:
   - uncaught errors;
   - unhandled promise rejections (except superseded requests);
   - React render errors (via an error boundary that shows a recoverable "Reload
     window" screen).
   
   Reports are clipped (2 KB message, 8 KB stack) and capped at 200 per session.
   No JS logging plugin is used, so the webview gets no log permission.
4. **Panics** are logged with a backtrace by a panic hook before the default handler
   runs. Engine jobs already contain panics per job.
5. **Help without a network.** The error banner offers:
   - "Copy details": message, reference, time, app/OS/renderer/codec versions and the
     log path;
   - "Open logs folder", opened from Rust via `tauri-plugin-opener`.
   
   Nothing is uploaded.

## Consequences

- A bug report of "Ref E-M2P8J-1 plus the log file" is enough to find the technical
  cause.
- Logs contain file paths of the user's photographs. They stay local; anyone asking a
  user for logs should say so.
- Remote crash reporting, if ever wanted, would be a separate, opt-in product decision
  with its own ADR.

## Alternatives considered

- *`tracing` + `tracing-appender`*: richer spans, but more setup. The `log` facade is
  enough for Phase 1 and `tracing` can consume `log` records later.
- *`@tauri-apps/plugin-log` in JS*: forwards all console output, but needs webview
  permissions and would log noise. The single typed error-report command is narrower.
