//! Presets (ADR 0046): listing the built-in and saved presets, and saving, renaming,
//! updating and deleting the photographer's own. Saved presets live in the catalogue.

use std::path::PathBuf;
use std::sync::Arc;

use app_core::{Catalogue, EditRecipe, EngineError, ErrorKind};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use super::IpcResult;
use crate::AppState;
use crate::ipc::{
    FileFailureDto, ImportedPresetDto, IpcError, PresetDto, PresetImportDto, preset_ref,
};

/// Runs `f` with the catalogue on the blocking pool.
async fn with_catalogue<T: Send + 'static>(
    state: &AppState,
    f: impl FnOnce(&Catalogue) -> Result<T, EngineError> + Send + 'static,
) -> IpcResult<T> {
    let catalogue = Arc::clone(&state.catalogue);
    tauri::async_runtime::spawn_blocking(move || f(&catalogue))
        .await
        .map_err(IpcError::internal)?
        .map_err(IpcError::from)
}

fn parse(id: &str) -> Result<app_core::PresetRef, EngineError> {
    preset_ref(id).ok_or_else(|| {
        EngineError::new(
            ErrorKind::NotFound,
            "That preset no longer exists.",
            format!("unknown preset id {id:?}"),
        )
    })
}

#[tauri::command]
pub async fn list_presets(state: State<'_, AppState>) -> IpcResult<Vec<PresetDto>> {
    let presets = with_catalogue(&state, app_core::list_presets).await?;
    Ok(presets.into_iter().map(PresetDto::from).collect())
}

/// Saves the look of `recipe` as a new preset.
#[tauri::command]
pub async fn create_preset(
    state: State<'_, AppState>,
    name: String,
    recipe: EditRecipe,
) -> IpcResult<PresetDto> {
    let preset =
        with_catalogue(&state, move |c| app_core::create_preset(c, &name, &recipe)).await?;
    Ok(preset.into())
}

#[tauri::command]
pub async fn rename_preset(state: State<'_, AppState>, id: String, name: String) -> IpcResult<()> {
    with_catalogue(&state, move |c| {
        app_core::rename_preset(c, &parse(&id)?, &name)
    })
    .await
}

/// Replaces a saved preset's look with that of `recipe`; returns the look stored.
#[tauri::command]
pub async fn update_preset(
    state: State<'_, AppState>,
    id: String,
    recipe: EditRecipe,
) -> IpcResult<EditRecipe> {
    with_catalogue(&state, move |c| {
        app_core::update_preset(c, &parse(&id)?, &recipe)
    })
    .await
}

#[tauri::command]
pub async fn delete_preset(state: State<'_, AppState>, id: String) -> IpcResult<()> {
    with_catalogue(&state, move |c| app_core::delete_preset(c, &parse(&id)?)).await
}

/// Paths from the webview are for the self-test only; otherwise files are chosen in
/// the system's dialogs.
fn self_test_paths<T>(state: &AppState, given: Option<T>) -> IpcResult<Option<T>> {
    match (given, &state.self_test) {
        (Some(p), Some(_)) => Ok(Some(p)),
        (Some(_), None) => Err(IpcError::internal(
            "Preset files must be chosen by the user.",
        )),
        (None, _) => Ok(None),
    }
}

/// Saves preset `id` as a file the photographer chooses (ADR 0047). Resolves to the
/// path written, or `None` if they cancelled.
#[tauri::command]
pub async fn export_preset(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    destination: Option<String>,
) -> IpcResult<Option<String>> {
    let preset = parse(&id).map_err(IpcError::from)?;
    let wanted = preset.clone();
    let name = with_catalogue(&state, move |c| {
        app_core::list_presets(c)?
            .into_iter()
            .find(|p| p.id == wanted)
            .map(|p| p.name)
            .ok_or_else(|| {
                EngineError::new(
                    ErrorKind::NotFound,
                    "That preset no longer exists.",
                    "preset not found",
                )
            })
    })
    .await?;
    let dest = match self_test_paths(&state, destination)? {
        Some(dest) => PathBuf::from(dest),
        None => {
            let file_name = app_core::preset_file_name(&name);
            let picked = tauri::async_runtime::spawn_blocking(move || {
                app.dialog()
                    .file()
                    .add_filter("Preset", &[app_core::PRESET_FILE_EXTENSION])
                    .set_file_name(file_name)
                    .blocking_save_file()
            })
            .await
            .map_err(IpcError::internal)?;
            match picked.and_then(|p| p.into_path().ok()) {
                Some(p) => with_preset_extension(p),
                None => return Ok(None),
            }
        }
    };
    let path = dest.display().to_string();
    with_catalogue(&state, move |c| {
        app_core::export_preset_file(c, &preset, &dest)
    })
    .await?;
    Ok(Some(path))
}

/// Imports preset files the photographer chooses: the app's own, or Lightroom `.xmp`
/// presets. Each file is imported or reported on its own.
#[tauri::command]
pub async fn import_presets(
    app: AppHandle,
    state: State<'_, AppState>,
    paths: Option<Vec<String>>,
) -> IpcResult<PresetImportDto> {
    let files: Vec<PathBuf> = match self_test_paths(&state, paths)? {
        Some(paths) => paths.into_iter().map(PathBuf::from).collect(),
        None => {
            let picked = tauri::async_runtime::spawn_blocking(move || {
                app.dialog()
                    .file()
                    .add_filter("Presets", &[app_core::PRESET_FILE_EXTENSION, "xmp"])
                    .blocking_pick_files()
            })
            .await
            .map_err(IpcError::internal)?;
            picked
                .unwrap_or_default()
                .into_iter()
                .filter_map(|p| p.into_path().ok())
                .collect()
        }
    };
    with_catalogue(&state, move |c| {
        let mut result = PresetImportDto {
            imported: Vec::new(),
            failed: Vec::new(),
        };
        for file in files {
            match app_core::import_preset_file(c, &file) {
                Ok(p) => result.imported.push(ImportedPresetDto {
                    preset: p.preset.into(),
                    from_lightroom: p.from_lightroom,
                    left_out: p.left_out.into_iter().map(str::to_owned).collect(),
                }),
                Err(e) => {
                    log::warn!("preset import: {}", e.detail);
                    result.failed.push(FileFailureDto {
                        file: file
                            .file_name()
                            .map_or_else(String::new, |f| f.to_string_lossy().into_owned()),
                        message: e.message,
                    });
                }
            }
        }
        Ok(result)
    })
    .await
}

/// `path` ending in `.preset` (the save dialog may leave the extension off).
fn with_preset_extension(path: PathBuf) -> PathBuf {
    let has = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(app_core::PRESET_FILE_EXTENSION));
    if has {
        path
    } else {
        let mut s = path.into_os_string();
        s.push(".");
        s.push(app_core::PRESET_FILE_EXTENSION);
        PathBuf::from(s)
    }
}
