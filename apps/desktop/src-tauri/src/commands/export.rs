use std::path::PathBuf;
use std::sync::atomic::Ordering;

use app_core::{EditRecipe, ExportFormat, ExportRequest, ImageId};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

use super::{IpcResult, wait};
use crate::AppState;
use crate::export_queue::QueuedExport;
use crate::ipc::{
    EXPORT_EVENT, ExportBatchDto, ExportEvent, ExportRequestDto, ExportStartedDto, FileFailureDto,
    IpcError,
};

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
            let format = single_format(&state);
            let picked = tauri::async_runtime::spawn_blocking(move || {
                let extensions = format.extensions();
                dialog_app
                    .dialog()
                    .file()
                    .add_filter(format_name(format), extensions)
                    .set_file_name(format!("export.{}", extensions[0]))
                    .blocking_save_file()
            })
            .await
            .map_err(|e| IpcError::internal(e.to_string()))?;
            match picked.and_then(|p| p.into_path().ok()) {
                Some(p) => with_extension_for(p, format),
                None => return Ok(None),
            }
        }
    };

    let judgements = state
        .engine
        .image_path(ImageId(request.image_id))
        .map(|path| marks_at(&state.catalogue, &path))
        .unwrap_or_default();
    let job_id = state.next_export_id.fetch_add(1, Ordering::Relaxed);
    let progress_app = app.clone();
    let handle = state.engine.export(
        ExportRequest {
            image: ImageId(request.image_id),
            recipe: request.recipe,
            destination: destination.clone(),
            format: single_format(&state),
            sharpening: output_sharpening(state.settings.get().export.sharpen),
            colour_space: colour_space(state.settings.get().export.colour_space),
            metadata: {
                let s = state.settings.get().export;
                app_core::MetadataChoice::from_switches(s.keep_metadata, s.strip_location)
            },
            judgements,
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

/// The export format for the remembered settings' choice (ADR 0057).
pub(crate) fn export_format(format: settings::ExportFileFormat, quality: u8) -> ExportFormat {
    match format {
        settings::ExportFileFormat::Jpeg => ExportFormat::Jpeg { quality },
        settings::ExportFileFormat::Tiff => ExportFormat::Tiff,
        settings::ExportFileFormat::Png => ExportFormat::Png,
    }
}

/// The output sharpening for the remembered settings' choice (ADR 0059).
pub(crate) fn output_sharpening(s: settings::OutputSharpening) -> app_core::OutputSharpening {
    match s {
        settings::OutputSharpening::None => app_core::OutputSharpening::None,
        settings::OutputSharpening::Screen => app_core::OutputSharpening::Screen,
        settings::OutputSharpening::Matte => app_core::OutputSharpening::Matte,
        settings::OutputSharpening::Glossy => app_core::OutputSharpening::Glossy,
    }
}

/// The colour space for the remembered settings' choice (ADR 0062).
/// The marks of the photo whose file is at `path`, as an export writes them (ADR
/// 0067); none for a file outside the library or when the catalogue cannot say.
fn marks_at(catalogue: &app_core::Catalogue, path: &std::path::Path) -> app_core::Judgements {
    let marks = catalogue
        .photo_at(path)
        .and_then(|photo| photo.map(|p| catalogue.marks(p)).transpose())
        .unwrap_or_else(|e| {
            log::warn!("marks lookup failed for {}: {e}", path.display());
            None
        });
    marks.map(|m| app_core::judgements(&m)).unwrap_or_default()
}

pub(crate) fn colour_space(s: settings::ExportColourSpace) -> app_core::ExportColourSpace {
    match s {
        settings::ExportColourSpace::Srgb => app_core::ExportColourSpace::Srgb,
        settings::ExportColourSpace::DisplayP3 => app_core::ExportColourSpace::DisplayP3,
        settings::ExportColourSpace::AdobeRgb => app_core::ExportColourSpace::AdobeRgb,
    }
}

/// The single-photo export's format: the remembered one.
fn single_format(state: &AppState) -> ExportFormat {
    let s = state.settings.get().export;
    export_format(s.format, s.jpeg_quality)
}

fn format_name(format: ExportFormat) -> &'static str {
    match format {
        ExportFormat::Jpeg { .. } => "JPEG",
        ExportFormat::Tiff => "TIFF",
        ExportFormat::Png => "PNG",
    }
}

/// Save panels can return a name without the format's extension: it is added.
fn with_extension_for(path: PathBuf, format: ExportFormat) -> PathBuf {
    let has = path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        format
            .extensions()
            .iter()
            .any(|x| x.eq_ignore_ascii_case(e))
    });
    if has {
        path
    } else {
        let mut s = path.into_os_string();
        s.push(".");
        s.push(format.extensions()[0]);
        PathBuf::from(s)
    }
}

/// The estimated size of an export of the open photo (ADR 0068), for the dialog's
/// "≈ 4.2 MB". A newer request cancels one still running (it then fails as cancelled,
/// which the dialog ignores).
#[tauri::command]
pub async fn estimate_export(
    state: State<'_, AppState>,
    request: crate::ipc::ExportEstimateRequestDto,
) -> IpcResult<crate::ipc::ExportEstimateDto> {
    let quality = request.quality.clamp(
        settings::ExportSettings::JPEG_QUALITY_MIN,
        settings::ExportSettings::JPEG_QUALITY_MAX,
    );
    let long_edge = request.long_edge.map(|e| {
        e.clamp(
            settings::ExportSettings::LONG_EDGE_MIN,
            settings::ExportSettings::LONG_EDGE_MAX,
        )
    });
    let estimate = wait(state.engine.estimate_export(
        ImageId(request.image_id),
        &request.recipe,
        export_format(request.format, quality),
        long_edge,
        output_sharpening(request.sharpen),
        colour_space(request.colour_space),
    ))
    .await?;
    Ok(crate::ipc::ExportEstimateDto {
        bytes: estimate.bytes,
        width: estimate.width,
        height: estimate.height,
    })
}

/// Chooses the folder exports are saved to, in the system's folder dialog, and
/// remembers it (ADR 0050). Resolves to the folder, or `None` if cancelled.
#[tauri::command]
pub async fn choose_export_folder(
    app: AppHandle,
    state: State<'_, AppState>,
) -> IpcResult<Option<String>> {
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Export to")
            .blocking_pick_folder()
    })
    .await
    .map_err(IpcError::internal)?;
    let Some(folder) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    let folder = folder.display().to_string();
    let mut settings = state.settings.get();
    settings.export.folder = Some(folder.clone());
    state
        .settings
        .update(settings)
        .map_err(IpcError::internal)?;
    Ok(Some(folder))
}

/// Queues photos for export (ADR 0050): the open photo with its current edit, or
/// library photos with their saved edits. Progress and the outcome arrive as
/// [`crate::ipc::ExportQueueEvent`]s. Resolves to the photos now in the run.
#[tauri::command]
pub async fn start_export(
    app: AppHandle,
    state: State<'_, AppState>,
    batch: ExportBatchDto,
) -> IpcResult<u32> {
    let folder = match (&batch.folder, &state.self_test) {
        (Some(folder), Some(_)) => PathBuf::from(folder),
        (Some(_), None) => {
            return Err(IpcError::internal(
                "Export folder must be chosen by the user.",
            ));
        }
        (None, _) => match state.settings.get().export.folder {
            Some(folder) => PathBuf::from(folder),
            None => {
                return Err(app_core::EngineError::new(
                    app_core::ErrorKind::InvalidInput,
                    "Choose a folder to export to.",
                    "no export folder",
                )
                .into());
            }
        },
    };
    let quality = batch.quality.clamp(
        settings::ExportSettings::JPEG_QUALITY_MIN,
        settings::ExportSettings::JPEG_QUALITY_MAX,
    );
    let format = export_format(batch.format.unwrap_or_default(), quality);
    let sharpening = output_sharpening(batch.sharpen.unwrap_or_default());
    let colour_space = colour_space(batch.colour_space.unwrap_or_default());
    let metadata = app_core::MetadataChoice::from_switches(
        batch.keep_metadata.unwrap_or(true),
        batch.strip_location.unwrap_or(false),
    );
    let long_edge = batch.long_edge.map(|e| {
        e.clamp(
            settings::ExportSettings::LONG_EDGE_MIN,
            settings::ExportSettings::LONG_EDGE_MAX,
        )
    });
    let catalogue = std::sync::Arc::clone(&state.catalogue);
    let resolved: Vec<Result<(PathBuf, EditRecipe, app_core::Judgements), FileFailureDto>> = batch
        .items
        .into_iter()
        .map(|item| {
            let name = item.path.clone().unwrap_or_default();
            let refuse = |message: &str| FileFailureDto {
                file: std::path::Path::new(&name).file_name().map_or_else(
                    || "A photo".to_owned(),
                    |n| n.to_string_lossy().into_owned(),
                ),
                message: message.to_owned(),
            };
            let source = match (item.image_id, &item.path) {
                (Some(id), _) => state.engine.image_path(ImageId(id)),
                (None, Some(path)) => state.folders.check(std::path::Path::new(path)),
                (None, None) => None,
            };
            let Some(source) = source else {
                return Err(refuse("It isn’t open or in a library folder."));
            };
            let recipe = match item.recipe {
                Some(recipe) => recipe,
                None => {
                    let stored = catalogue.edit_at(&source).unwrap_or_else(|e| {
                        log::warn!("edit lookup failed for {}: {e}", source.display());
                        None
                    });
                    app_core::SavedEdit::from_stored(stored.as_ref())
                        .recipe()
                        .unwrap_or_default()
                }
            };
            let judgements = marks_at(&catalogue, &source);
            Ok((source, recipe, judgements))
        })
        .collect();
    let mut items = Vec::new();
    let mut refused = Vec::new();
    for r in resolved {
        match r {
            Ok((source, recipe, judgements)) => items.push(QueuedExport {
                source,
                recipe,
                judgements,
                folder: folder.clone(),
                long_edge,
                format,
                sharpening,
                colour_space,
                metadata,
            }),
            Err(f) => refused.push(f),
        }
    }
    Ok(state.export_queue.add(&app, items, refused))
}

/// Stops exporting: the photo exporting now is cancelled and the rest are dropped.
#[tauri::command]
pub fn cancel_exports(state: State<'_, AppState>) {
    state.export_queue.cancel();
}
