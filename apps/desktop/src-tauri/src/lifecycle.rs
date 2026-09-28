//! Application lifecycle: quitting safely while background work runs.
//!
//! Closing the window or quitting (Cmd+Q / Alt+F4) while an export is running is
//! intercepted: the UI asks the user, and a confirmed quit cancels the exports and
//! waits briefly for them to stop, so no temporary files are left behind (exports
//! write atomically, so a cancelled export never leaves a partial photo).

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, RunEvent, Runtime, WindowEvent};
use ts_rs::TS;

use crate::AppState;

/// Event asking the UI to confirm quitting.
pub const QUIT_REQUESTED_EVENT: &str = "app://quit-requested";
/// Longest time a confirmed quit waits for cancelled exports to stop.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct QuitRequestedDto {
    pub exports_running: u32,
}

fn exports_running(state: &AppState) -> usize {
    state.exports.lock().map_or(0, |e| e.len())
}

/// Whether a close/quit should be held for confirmation (and the UI asked).
fn hold_for_confirmation<R: Runtime>(app: &AppHandle<R>) -> bool {
    let Some(state) = app.try_state::<AppState>() else {
        return false;
    };
    let running = exports_running(&state);
    if running == 0 || state.quitting.load(Ordering::SeqCst) {
        return false;
    }
    log::info!("quit requested with {running} export(s) running; asking for confirmation");
    let _ = app.emit(
        QUIT_REQUESTED_EVENT,
        QuitRequestedDto {
            exports_running: running as u32,
        },
    );
    true
}

/// Window events: intercept closing the main window during an export.
pub fn on_window_event<R: Runtime>(window: &tauri::Window<R>, event: &WindowEvent) {
    if let WindowEvent::CloseRequested { api, .. } = event
        && hold_for_confirmation(window.app_handle())
    {
        api.prevent_close();
    }
}

/// App events: intercept user-initiated quit (e.g. Cmd+Q) during an export, and log
/// shutdown.
pub fn on_run_event<R: Runtime>(app: &AppHandle<R>, event: RunEvent) {
    match event {
        // `code` is None for user-initiated quits; programmatic exits went through
        // `shutdown` already.
        RunEvent::ExitRequested {
            code: None, api, ..
        } if hold_for_confirmation(app) => api.prevent_exit(),
        RunEvent::Exit => log::info!("exiting"),
        _ => {}
    }
}

/// Cancels running exports, waits up to [`SHUTDOWN_GRACE`] for them to stop, then
/// exits. Runs off the main thread.
pub fn shutdown<R: Runtime>(app: AppHandle<R>, code: i32) {
    if let Some(state) = app.try_state::<AppState>() {
        state.quitting.store(true, Ordering::SeqCst);
        let tokens: Vec<_> = state
            .exports
            .lock()
            .map(|e| e.values().cloned().collect())
            .unwrap_or_default();
        if !tokens.is_empty() {
            log::info!("cancelling {} export(s) before quitting", tokens.len());
        }
        for t in &tokens {
            t.cancel();
        }
    }
    tauri::async_runtime::spawn(async move {
        let start = Instant::now();
        loop {
            let running = app
                .try_state::<AppState>()
                .map_or(0, |s| exports_running(&s));
            if running == 0 || start.elapsed() > SHUTDOWN_GRACE {
                if running > 0 {
                    log::warn!(
                        "{running} export(s) did not stop within {SHUTDOWN_GRACE:?}; exiting anyway"
                    );
                }
                break;
            }
            sleep(Duration::from_millis(20)).await;
        }
        app.exit(code);
    });
}

/// Async sleep without adding a timer dependency: parks a blocking-pool thread.
async fn sleep(d: Duration) {
    let _ = tauri::async_runtime::spawn_blocking(move || std::thread::sleep(d)).await;
}

/// The user confirmed quitting from the UI.
#[tauri::command]
pub fn quit(app: AppHandle) {
    shutdown(app, 0);
}
