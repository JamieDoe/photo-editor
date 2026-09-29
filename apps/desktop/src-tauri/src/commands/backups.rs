//! Library backups in Settings (ADR 0021): status, "Back up now", "Show in Finder".

use app_core::BackupKind;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

use super::IpcResult;
use crate::AppState;
use crate::backups::{CopyTarget, copy_target};
use crate::ipc::{BackupCopyDto, BackupStatusDto, IpcError};

fn status(state: &AppState) -> IpcResult<BackupStatusDto> {
    let all = state.backups.list().map_err(IpcError::internal)?;
    let copy = state.settings.get().backups.copy_folder.map(|folder| {
        let (connected, copies) = match copy_target(Some(&folder)) {
            CopyTarget::Ready(store) => (true, store.list().unwrap_or_default()),
            _ => (false, Vec::new()),
        };
        BackupCopyDto {
            folder,
            connected,
            count: copies.len() as u32,
            latest_at_ms: copies.first().map(|b| b.taken_at_ms),
        }
    });
    Ok(BackupStatusDto {
        enabled: state.catalogue.path().is_some(),
        count: all.len() as u32,
        total_bytes: all.iter().map(|b| b.bytes).sum(),
        latest_at_ms: all.first().map(|b| b.taken_at_ms),
        folder: state.backups.dir().display().to_string(),
        copy,
    })
}

/// Chooses (with the native dialog) a folder, typically on another drive, that also
/// receives every backup, and copies the newest backup there straight away. Returns
/// `None` if the user cancelled.
#[tauri::command]
pub async fn choose_backup_copy_folder(app: AppHandle) -> IpcResult<Option<BackupStatusDto>> {
    let dialog = app.clone();
    let picked =
        tauri::async_runtime::spawn_blocking(move || dialog.dialog().file().blocking_pick_folder())
            .await
            .map_err(IpcError::internal)?;
    let Some(folder) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    let folder = folder.display().to_string();
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        state.settings.modify(|mut s| {
            s.backups.copy_folder = Some(folder.clone());
            s
        })?;
        log::info!("backups will also be copied to {folder}");
        if let Some(newest) = state.backups.list()?.into_iter().next() {
            crate::backups::copy_to_other_drive(&state, &newest);
        }
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    })
    .await
    .map_err(IpcError::internal)?
    .map_err(IpcError::internal)?;
    status(&app.state::<AppState>()).map(Some)
}

/// Stops copying backups to the other drive. Copies already there are kept.
#[tauri::command]
pub fn stop_backup_copies(state: State<'_, AppState>) -> IpcResult<BackupStatusDto> {
    state
        .settings
        .modify(|mut s| {
            s.backups.copy_folder = None;
            s
        })
        .map_err(IpcError::internal)?;
    log::info!("backups are no longer copied to another folder");
    status(&state)
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
