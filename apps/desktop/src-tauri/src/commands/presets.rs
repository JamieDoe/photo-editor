//! Presets (ADR 0046): listing the built-in and saved presets, and saving, renaming,
//! updating and deleting the photographer's own. Saved presets live in the catalogue.

use std::sync::Arc;

use app_core::{Catalogue, EditRecipe, EngineError, ErrorKind};
use tauri::State;

use super::IpcResult;
use crate::AppState;
use crate::ipc::{IpcError, PresetDto, preset_ref};

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
