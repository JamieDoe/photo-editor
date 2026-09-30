use std::path::PathBuf;

use app_core::{ImageId, PreviewRequest};
use image_core::OutputImage;
use tauri::ipc::Response;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use super::{IpcResult, wait};
use crate::AppState;
use crate::ipc::{
    FRAME_FLAG_CACHE_HIT, FRAME_HEADER_BYTES, ImageSummaryDto, IpcError, PreviewRequestDto,
};

/// Encodes a frame in the binary layout documented on [`FRAME_HEADER_BYTES`].
fn frame_bytes(
    img: &OutputImage,
    level: u32,
    flags: u32,
    ms: f64,
    full_size: (u32, u32),
) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(FRAME_HEADER_BYTES + img.byte_size());
    bytes.extend_from_slice(&img.width().to_le_bytes());
    bytes.extend_from_slice(&img.height().to_le_bytes());
    bytes.extend_from_slice(&level.to_le_bytes());
    bytes.extend_from_slice(&flags.to_le_bytes());
    bytes.extend_from_slice(&(ms as f32).to_le_bytes());
    bytes.extend_from_slice(&full_size.0.to_le_bytes());
    bytes.extend_from_slice(&full_size.1.to_le_bytes());
    bytes.extend_from_slice(img.data());
    bytes
}

/// Opens `path` and returns its summary with the photo's saved edit, so the first
/// render already shows it. Only renders are shown in the editor, never the camera's
/// embedded JPEG (ADR 0020).
async fn open(state: &AppState, path: PathBuf) -> IpcResult<ImageSummaryDto> {
    let summary = wait(state.engine.open(path)).await?;
    let (saved_recipe, edit_saving) = super::edits::saved_edit(state, &summary.path).await;
    Ok(ImageSummaryDto {
        saved_recipe,
        edit_saving,
        ..summary.into()
    })
}

#[tauri::command]
pub async fn open_image_dialog(
    app: AppHandle,
    state: State<'_, AppState>,
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
    Ok(Some(open(&state, path).await?))
}

/// Opens an image by path (from the Library, and the self-test). Read-only: the
/// engine only ever decodes the file. Paths must lie inside a granted folder.
#[tauri::command]
pub async fn open_image_path(
    state: State<'_, AppState>,
    path: String,
) -> IpcResult<ImageSummaryDto> {
    let requested = PathBuf::from(&path);
    // Only photos inside granted folders (or the self-test file) may be opened.
    let is_self_test = state
        .self_test
        .as_ref()
        .is_some_and(|t| t.canonicalize().ok() == requested.canonicalize().ok());
    let allowed = if is_self_test {
        requested.canonicalize().ok()
    } else {
        state.folders.check(&requested)
    };
    let Some(file) = allowed else {
        let reference = crate::logging::new_reference();
        log::warn!("[{reference}] open refused, not in a granted folder: {path}");
        return Err(IpcError {
            kind: crate::ipc::IpcErrorKind::NotFound,
            message: "This photo isn’t available. Its folder may have been moved; choose the folder again in the Library.".into(),
            reference: Some(reference),
        });
    };
    open(&state, file).await
}

/// Auto level (ADR 0033): the straighten angle that levels the open photo, or null
/// when it has no clear horizon or vertical.
#[tauri::command]
pub async fn auto_level(state: State<'_, AppState>, image_id: u64) -> IpcResult<Option<f32>> {
    wait(state.engine.auto_level(ImageId(image_id))).await
}

/// Remove chromatic aberration (ADR 0035): the correction measured on the open photo,
/// or null when it has too few clean edges to measure.
#[tauri::command]
pub async fn measure_chromatic_aberration(
    state: State<'_, AppState>,
    image_id: u64,
) -> IpcResult<Option<renderer::ChromaticAberration>> {
    wait(state.engine.measure_chromatic_aberration(ImageId(image_id))).await
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
        frame.full_size,
    )))
}
