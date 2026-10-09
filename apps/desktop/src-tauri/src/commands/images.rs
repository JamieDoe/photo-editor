use std::path::PathBuf;

use app_core::{ImageId, PreviewRequest};
use image_core::OutputImage;
use tauri::ipc::Response;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use super::{IpcResult, wait};
use crate::AppState;
use crate::ipc::{
    FRAME_FLAG_CACHE_HIT, FRAME_FLAG_FILL_PENDING, FRAME_FLAG_HISTOGRAM, FRAME_FLAG_WINDOW,
    FRAME_HEADER_BYTES, FRAME_WINDOW_BYTES, ImageSummaryDto, IpcError, PreviewRequestDto,
};

/// Encodes a frame in the binary layout documented on [`FRAME_HEADER_BYTES`].
fn frame_bytes(
    img: &OutputImage,
    level: u32,
    mut flags: u32,
    ms: f64,
    full_size: (u32, u32),
    histogram: Option<&renderer::Histogram>,
    window: Option<[f64; 4]>,
) -> Vec<u8> {
    if histogram.is_some() {
        flags |= FRAME_FLAG_HISTOGRAM;
    }
    if window.is_some() {
        flags |= FRAME_FLAG_WINDOW;
    }
    let mut bytes = Vec::with_capacity(
        FRAME_HEADER_BYTES
            + FRAME_WINDOW_BYTES
            + renderer::histogram::ENCODED_BYTES
            + img.byte_size(),
    );
    bytes.extend_from_slice(&img.width().to_le_bytes());
    bytes.extend_from_slice(&img.height().to_le_bytes());
    bytes.extend_from_slice(&level.to_le_bytes());
    bytes.extend_from_slice(&flags.to_le_bytes());
    bytes.extend_from_slice(&(ms as f32).to_le_bytes());
    bytes.extend_from_slice(&full_size.0.to_le_bytes());
    bytes.extend_from_slice(&full_size.1.to_le_bytes());
    for v in window.into_iter().flatten() {
        bytes.extend_from_slice(&(v as f32).to_le_bytes());
    }
    if let Some(h) = histogram {
        h.write_le(&mut bytes);
    }
    bytes.extend_from_slice(img.data());
    bytes
}

/// Opens `path` and returns its summary with the photo's saved edit, so the first
/// render already shows it. Only renders are shown in the editor, never the camera's
/// embedded JPEG (ADR 0020).
async fn open(state: &AppState, path: PathBuf) -> IpcResult<ImageSummaryDto> {
    let summary = wait(state.engine.open(path)).await?;
    let (saved_recipe, edit_saving) = super::edits::saved_edit(state, &summary.path).await;
    // An edit made on the file as stored, adapted to it upright (ADR 0078); saved so
    // with the next change.
    let saved_recipe = saved_recipe.map(|r| r.on_upright(summary.upright));
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

/// A new heal or clone spot (ADR 0054) at `at` (fractions of the photo) of `radius`
/// (a fraction of its long edge), its source found nearby, or null when none fits.
#[tauri::command]
pub async fn new_spot(
    state: State<'_, AppState>,
    image_id: u64,
    kind: renderer::retouch::SpotKind,
    at: [f32; 2],
    radius: f32,
    avoid: Vec<renderer::retouch::Spot>,
) -> IpcResult<Option<renderer::retouch::Spot>> {
    wait(
        state
            .engine
            .new_spot(ImageId(image_id), kind, at, radius, avoid),
    )
    .await
}

/// Sensor dust on the open photo (ADR 0058), as heal spots that `existing` spots do
/// not already cover, each with a source.
#[tauri::command]
pub async fn find_dust(
    state: State<'_, AppState>,
    image_id: u64,
    existing: Vec<renderer::retouch::Spot>,
) -> IpcResult<Vec<renderer::retouch::Spot>> {
    wait(state.engine.find_dust(ImageId(image_id), existing)).await
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
    let handle = state.engine.render_preview_in(
        PreviewRequest {
            image: ImageId(request.image_id),
            recipe: request.recipe,
            quality: request.quality,
            target_long_edge: request.target_long_edge,
            window: request.window.map(|[x, y, w, h]| (x, y, w, h)),
        },
        request.slot.into(),
    );
    let frame = wait(handle).await?;
    let mut flags = if frame.cache_hit {
        FRAME_FLAG_CACHE_HIT
    } else {
        0
    };
    if frame.fill_pending {
        flags |= FRAME_FLAG_FILL_PENDING;
    }
    Ok(Response::new(frame_bytes(
        &frame.image,
        frame.level as u32,
        flags,
        frame.render_ms,
        frame.full_size,
        frame.histogram.as_deref(),
        frame.window,
    )))
}

/// Decodes the open photo at full resolution (ADR 0070) for viewing it at 100 %;
/// resolves when window renders can use it.
/// The red eye nearest `at` on the open photo, within `radius`, as a correction sized
/// to it (ADR 0080); null when there is no red pupil there.
#[tauri::command]
pub async fn find_red_eye(
    state: State<'_, AppState>,
    image_id: u64,
    at: [f32; 2],
    radius: f32,
) -> IpcResult<Option<renderer::redeye::RedEye>> {
    wait(state.engine.find_red_eye(ImageId(image_id), at, radius)).await
}

/// Makes a `kind` mask of the open photo and keeps it (ADR 0074); null when the photo
/// has nothing of the kind (no subject, nobody).
#[tauri::command]
pub async fn generate_mask(
    state: State<'_, AppState>,
    image_id: u64,
    kind: renderer::masks::GeneratedKind,
) -> IpcResult<Option<crate::ipc::GeneratedMaskDto>> {
    let made = wait(
        state
            .engine
            .segment(ImageId(image_id), app_core::mask_kind(kind)),
    )
    .await?;
    Ok(made.map(|m| crate::ipc::GeneratedMaskDto {
        name: m.name,
        kind,
        share: m.coverage.share(),
    }))
}

/// The generated masks `recipe` names that the open photo can't use (ADR 0074): not
/// kept on this computer, or made from another photo.
#[tauri::command]
pub fn missing_masks(
    state: State<'_, AppState>,
    image_id: u64,
    recipe: renderer::EditRecipe,
) -> Vec<String> {
    state.engine.missing_masks(ImageId(image_id), &recipe)
}

/// A stored mask over the shown part of the open photo (ADR 0074): `width` ×
/// `height` bytes of coverage over `crop` of the frame the request describes, for
/// drawing its tint; empty when the mask isn't kept.
#[tauri::command]
pub async fn mask_view(
    state: State<'_, AppState>,
    image_id: u64,
    view: crate::ipc::MaskViewRequestDto,
) -> IpcResult<Response> {
    let bytes = wait(state.engine.mask_view(
        ImageId(image_id),
        view.geometry,
        view.profile_corrections,
        &view.name,
        view.crop,
        (view.width, view.height),
    ))
    .await?;
    Ok(Response::new(bytes.unwrap_or_default()))
}

/// Auto tone (ADR 0071): the open photo's tone sliders as a starting point, for the
/// photo as `recipe` edits it.
#[tauri::command]
pub async fn auto_tone(
    state: State<'_, AppState>,
    image_id: u64,
    recipe: renderer::EditRecipe,
) -> IpcResult<renderer::auto_tone::AutoTone> {
    wait(state.engine.auto_tone(ImageId(image_id), &recipe)).await
}

/// Auto for one setting (ADR 0071): its value for the open photo as `recipe` edits
/// it, the rest of the edit as it is.
#[tauri::command]
pub async fn auto_setting(
    state: State<'_, AppState>,
    image_id: u64,
    recipe: renderer::EditRecipe,
    setting: renderer::auto_tone::ToneSetting,
) -> IpcResult<f32> {
    wait(
        state
            .engine
            .auto_setting(ImageId(image_id), &recipe, setting),
    )
    .await
}

/// Fills `removals` on the open photo at full resolution (ADR 0070), so every view
/// shows the same fill; resolves when renders use it.
#[tauri::command]
pub async fn prepare_fill(
    state: State<'_, AppState>,
    image_id: u64,
    removals: Vec<renderer::remove::Removal>,
) -> IpcResult<()> {
    wait(state.engine.prepare_fill(ImageId(image_id), removals)).await
}

#[tauri::command]
pub async fn prepare_full(state: State<'_, AppState>, image_id: u64) -> IpcResult<()> {
    wait(state.engine.prepare_full(ImageId(image_id))).await
}
