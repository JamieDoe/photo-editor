//! Tauri command handlers: thin adapters from IPC payloads to the engine.
//!
//! Handlers never do heavy work themselves. They submit engine jobs and await the
//! result on Tauri's blocking pool, so the main (UI) thread is never blocked.

use std::path::PathBuf;
use std::sync::atomic::Ordering;

use app_core::{
    EmbeddedFrame, EngineError, ExportFormat, ExportRequest, ImageId, JobHandle, PreviewRequest,
};
use image_core::OutputImage;
use tauri::ipc::{Channel, Response};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_dialog::DialogExt;

use crate::AppState;
use crate::ipc::{
    EXPORT_EVENT, EngineInfoDto, ExportEvent, ExportRequestDto, ExportStartedDto,
    FRAME_FLAG_CACHE_HIT, FRAME_FLAG_EMBEDDED, FRAME_HEADER_BYTES, ImageSummaryDto, IpcError,
    PreviewRequestDto, SelfTestConfigDto,
};

type IpcResult<T> = Result<T, IpcError>;

/// Waits for a job off the async runtime's worker threads.
async fn wait<T: Send + 'static>(handle: JobHandle<T, EngineError>) -> IpcResult<T> {
    tauri::async_runtime::spawn_blocking(move || handle.wait())
        .await
        .map_err(|e| IpcError::internal(format!("worker join failed: {e}")))?
        .map_err(|e| EngineError::from_job(e).into())
}

/// Encodes a frame in the binary layout documented on [`FRAME_HEADER_BYTES`].
fn frame_bytes(img: &OutputImage, level: u32, flags: u32, ms: f64) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(FRAME_HEADER_BYTES + img.byte_size());
    bytes.extend_from_slice(&img.width().to_le_bytes());
    bytes.extend_from_slice(&img.height().to_le_bytes());
    bytes.extend_from_slice(&level.to_le_bytes());
    bytes.extend_from_slice(&flags.to_le_bytes());
    bytes.extend_from_slice(&(ms as f32).to_le_bytes());
    bytes.extend_from_slice(img.data());
    bytes
}

/// Opens `path`, streaming the embedded preview (if any) over `on_preview` as soon
/// as it is extracted, before the decode finishes.
async fn open_streaming(
    state: &AppState,
    path: PathBuf,
    on_preview: Channel<Response>,
) -> IpcResult<ImageSummaryDto> {
    let handle = state
        .engine
        .open_with_preview(path, move |frame: EmbeddedFrame| {
            let bytes = frame_bytes(&frame.image, 0, FRAME_FLAG_EMBEDDED, frame.extract_ms);
            // The UI may have navigated away; a closed channel is not an error.
            let _ = on_preview.send(Response::new(bytes));
        });
    Ok(wait(handle).await?.into())
}

#[tauri::command]
pub fn engine_info(state: State<'_, AppState>) -> EngineInfoDto {
    state.engine.info().into()
}

#[tauri::command]
pub async fn open_image_dialog(
    app: AppHandle,
    state: State<'_, AppState>,
    on_preview: Channel<Response>,
) -> IpcResult<Option<ImageSummaryDto>> {
    let extensions: Vec<String> = state
        .engine
        .info()
        .extensions
        .into_iter()
        .map(str::to_owned)
        .collect();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        let exts: Vec<&str> = extensions.iter().map(String::as_str).collect();
        app.dialog()
            .file()
            .add_filter("Photographs", &exts)
            .blocking_pick_file()
    })
    .await
    .map_err(|e| IpcError::internal(e.to_string()))?;
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    Ok(Some(open_streaming(&state, path, on_preview).await?))
}

/// Opens an image by path (drag-and-drop and self-test). Read-only: the engine only
/// ever decodes the file.
#[tauri::command]
pub async fn open_image_path(
    state: State<'_, AppState>,
    path: String,
    on_preview: Channel<Response>,
) -> IpcResult<ImageSummaryDto> {
    open_streaming(&state, PathBuf::from(path), on_preview).await
}

#[tauri::command]
pub async fn render_preview(
    state: State<'_, AppState>,
    request: PreviewRequestDto,
) -> IpcResult<Response> {
    let handle = state.engine.render_preview(PreviewRequest {
        image: ImageId(request.image_id),
        recipe: request.recipe,
        quality: request.quality,
        target_long_edge: request.target_long_edge,
    });
    let frame = wait(handle).await?;
    let flags = if frame.cache_hit {
        FRAME_FLAG_CACHE_HIT
    } else {
        0
    };
    Ok(Response::new(frame_bytes(
        &frame.image,
        frame.level as u32,
        flags,
        frame.render_ms,
    )))
}

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
                quality: request.quality.clamp(1, 100),
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

    // Completion is reported asynchronously so the command returns immediately.
    tauri::async_runtime::spawn(async move {
        let event = match wait(handle).await {
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

#[tauri::command]
pub fn self_test_config(state: State<'_, AppState>) -> Option<SelfTestConfigDto> {
    let image = state.self_test.as_ref()?;
    let export = std::env::temp_dir().join("photo-editor-self-test-export.jpg");
    Some(SelfTestConfigDto {
        image_path: image.display().to_string(),
        export_path: export.display().to_string(),
    })
}

/// Receives the UI-side self-test report, prints it, and exits.
#[tauri::command]
pub fn self_test_report(app: AppHandle, state: State<'_, AppState>, report: serde_json::Value) {
    if state.self_test.is_none() {
        return;
    }
    println!("SELF_TEST_REPORT {report}");
    app.exit(
        if report.get("ok").and_then(serde_json::Value::as_bool) == Some(true) {
            0
        } else {
            1
        },
    );
}
