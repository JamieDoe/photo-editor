//! Library backups in Settings (ADR 0021): status, "Back up now", "Show in Finder".

use app_core::BackupKind;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

use super::IpcResult;
use crate::AppState;
use crate::ipc::{BackupStatusDto, IpcError};

fn status(state: &AppState) -> IpcResult<BackupStatusDto> {
    let all = state.backups.list().map_err(IpcError::internal)?;
    Ok(BackupStatusDto {
        enabled: state.catalogue.path().is_some(),
        count: all.len() as u32,
        total_bytes: all.iter().map(|b| b.bytes).sum(),
        latest_at_ms: all.first().map(|b| b.taken_at_ms),
        folder: state.backups.dir().display().to_string(),
    })
}

#[tauri::command]
pub async fn library_backups(state: State<'_, AppState>) -> IpcResult<BackupStatusDto> {
    status(&state)
}

/// Takes a backup now (off the async runtime; it copies the whole catalogue).
#[tauri::command]
pub async fn back_up_library(app: AppHandle) -> IpcResult<BackupStatusDto> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::backups::back_up(&handle.state::<AppState>(), BackupKind::Manual)
    })
    .await
    .map_err(IpcError::internal)?
    .map_err(IpcError::internal)?;
    status(&app.state::<AppState>())
}

/// Opens the backups folder in Finder / Explorer.
#[tauri::command]
pub fn show_backups(app: AppHandle, state: State<'_, AppState>) -> IpcResult<()> {
    let dir = state.backups.dir();
    std::fs::create_dir_all(dir).map_err(IpcError::internal)?;
    app.opener()
        .open_path(dir.display().to_string(), None::<&str>)
        .map_err(IpcError::internal)
}
