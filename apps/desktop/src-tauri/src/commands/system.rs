use std::sync::atomic::{AtomicU32, Ordering};

use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

use super::IpcResult;
use crate::AppState;
use crate::ipc::{ClientErrorReport, DiagnosticsDto, EngineInfoDto, IpcError};
use crate::logging::{clip, new_reference};

/// UI error reports accepted per session before further ones are dropped, so a render
/// loop that throws cannot flood the log.
const MAX_CLIENT_REPORTS: u32 = 200;

#[tauri::command]
pub fn engine_info(state: State<'_, AppState>) -> EngineInfoDto {
    state.engine.info().into()
}

#[tauri::command]
pub fn diagnostics(app: AppHandle, state: State<'_, AppState>) -> DiagnosticsDto {
    let info = state.engine.info();
    DiagnosticsDto {
        app_version: app.package_info().version.to_string(),
        os: std::env::consts::OS.to_owned(),
        arch: std::env::consts::ARCH.to_owned(),
        cpu_threads: std::thread::available_parallelism().map_or(1, |n| n.get() as u32),
        renderer_version: info.renderer_version,
        libraw_version: info.libraw_version,
        jpeg_encoder: info.jpeg_encoder.to_owned(),
        embedded_jpeg_decoder: info.embedded_jpeg_decoder.to_owned(),
        log_dir: app
            .path()
            .app_log_dir()
            .ok()
            .map(|p| p.display().to_string()),
    }
}

/// Opens the log directory in the system file manager.
#[tauri::command]
pub fn open_logs_folder(app: AppHandle) -> IpcResult<()> {
    let dir = app.path().app_log_dir().map_err(IpcError::internal)?;
    std::fs::create_dir_all(&dir).map_err(IpcError::internal)?;
    app.opener()
        .open_path(dir.display().to_string(), None::<&str>)
        .map_err(IpcError::internal)
}

/// Records an error that happened in the webview. Returns the reference shown to the
/// user, or `None` once the per-session limit is reached.
#[tauri::command]
pub fn report_client_error(report: ClientErrorReport) -> Option<String> {
    static COUNT: AtomicU32 = AtomicU32::new(0);
    let n = COUNT.fetch_add(1, Ordering::Relaxed);
    if n == MAX_CLIENT_REPORTS {
        log::warn!("further UI error reports suppressed this session");
    }
    if n >= MAX_CLIENT_REPORTS {
        return None;
    }
    let reference = new_reference();
    log::error!(
        target: "ui",
        "[{reference}] {:?}: {}{}",
        report.source,
        clip(&report.message, 2_000),
        report.stack.map(|s| format!("\n{}", clip(&s, 8_000))).unwrap_or_default()
    );
    Some(reference)
}
