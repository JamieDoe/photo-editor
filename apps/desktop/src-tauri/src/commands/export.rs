use std::path::PathBuf;
use std::sync::atomic::Ordering;

use app_core::{ExportFormat, ExportRequest, ImageId};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

use super::{IpcResult, wait};
use crate::AppState;
use crate::ipc::{EXPORT_EVENT, ExportEvent, ExportRequestDto, ExportStartedDto, IpcError};

/// Starts a background export. Returns `None` if the user cancelled the save dialog.
/// Progress and completion are delivered as [`ExportEvent`]s.
#[tauri::command]
pub async fn export_image(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ExportRequestDto,
) -> IpcResult<Option<ExportStartedDto>> {
    let destination = match (&request.destination, &state.self_test) {
        (Some(dest), Some(_)) => PathBuf::from(dest),
        (Some(_), None) => {
            return Err(IpcError::internal(
                "Export destination must be chosen by the user.",
            ));
        }
        (None, _) => {
            let dialog_app = app.clone();
            let picked = tauri::async_runtime::spawn_blocking(move || {
                dialog_app
                    .dialog()
                    .file()
                    .add_filter("JPEG", &["jpg", "jpeg"])
                    .set_file_name("export.jpg")
                    .blocking_save_file()
            })
            .await
            .map_err(|e| IpcError::internal(e.to_string()))?;
            match picked.and_then(|p| p.into_path().ok()) {
                Some(p) => with_jpeg_extension(p),
                None => return Ok(None),
            }
        }
    };

    let job_id = state.next_export_id.fetch_add(1, Ordering::Relaxed);
    let progress_app = app.clone();
    let handle = state.engine.export(
        ExportRequest {
            image: ImageId(request.image_id),
            recipe: request.recipe,
            destination: destination.clone(),
            format: ExportFormat::Jpeg {
                quality: state.settings.get().export.jpeg_quality,
            },
        },
        move |p| {
            let event = ExportEvent::Progress {
                job_id,
                stage: p.stage.into(),
                fraction: p.fraction,
            };
            let _ = progress_app.emit(EXPORT_EVENT, event);
        },
    );

    // Tracked so quitting can cancel it and wait for it (see `lifecycle`).
    state
        .exports
        .lock()
        .expect("exports lock")
        .insert(job_id, handle.token().clone());

    // Completion is reported asynchronously so the command returns immediately.
    tauri::async_runtime::spawn(async move {
        let result = wait(handle).await;
        if let Some(state) = app.try_state::<AppState>() {
            state.exports.lock().expect("exports lock").remove(&job_id);
        }
        let event = match result {
            Ok(summary) => ExportEvent::finished(job_id, &summary),
            Err(error) => ExportEvent::Failed { job_id, error },
        };
        let _ = app.emit(EXPORT_EVENT, event);
    });

    Ok(Some(ExportStartedDto {
        job_id,
        path: destination.display().to_string(),
    }))
}

/// Save panels can return a name without an extension; exports are always JPEG.
fn with_jpeg_extension(path: PathBuf) -> PathBuf {
    let is_jpeg = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("jpg") || e.eq_ignore_ascii_case("jpeg"));
    if is_jpeg {
        path
    } else {
        let mut s = path.into_os_string();
        s.push(".jpg");
        PathBuf::from(s)
    }
}
