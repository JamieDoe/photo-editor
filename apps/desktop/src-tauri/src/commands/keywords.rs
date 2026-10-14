//! Keywords (ADR 0088): adding them to photos and taking them off, and the library's
//! list for suggestions. Only photos inside granted folders can be changed. When
//! sidecars are on (ADR 0067), the changed RAWs' sidecars get the new keywords.

use std::path::PathBuf;
use std::sync::Arc;

use app_core::{Catalogue, PhotoId};
use tauri::State;

use super::IpcResult;
use super::library::{library_photo, raw_extensions};
use super::marks::granted_files;
use crate::AppState;
use crate::ipc::{IpcError, KeywordUseDto};

/// The most keywords one change takes.
const MAX_NAMES: usize = 200;

fn uses(found: Vec<app_core::KeywordUse>) -> Vec<KeywordUseDto> {
    found.into_iter().map(KeywordUseDto::from).collect()
}

/// Every keyword in the library, by name, with how many photos have it.
#[tauri::command]
pub async fn list_keywords(state: State<'_, AppState>) -> IpcResult<Vec<KeywordUseDto>> {
    let catalogue = Arc::clone(&state.catalogue);
    let found = tauri::async_runtime::spawn_blocking(move || catalogue.keywords())
        .await
        .map_err(IpcError::internal)?
        .map_err(IpcError::internal)?;
    Ok(uses(found))
}

/// The keywords of the photos at `paths`, each with how many of them have it.
#[tauri::command]
pub async fn photo_keywords(
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> IpcResult<Vec<KeywordUseDto>> {
    change(&state, paths, Vec::new(), |_, _, _| Ok(())).await
}

/// Adds `names` to the photos at `paths`; their keywords afterwards.
#[tauri::command]
pub async fn add_keywords(
    state: State<'_, AppState>,
    paths: Vec<String>,
    names: Vec<String>,
) -> IpcResult<Vec<KeywordUseDto>> {
    change(&state, paths, names, |c, photos, names| {
        c.add_keywords(photos, names)
    })
    .await
}

/// Takes `names` off the photos at `paths`; their keywords afterwards.
#[tauri::command]
pub async fn remove_keywords(
    state: State<'_, AppState>,
    paths: Vec<String>,
    names: Vec<String>,
) -> IpcResult<Vec<KeywordUseDto>> {
    change(&state, paths, names, |c, photos, names| {
        c.remove_keywords(photos, names)
    })
    .await
}

/// Applies `apply` with `names` to the photos at `paths` (nothing to apply when there
/// are no names), syncs their sidecars when they changed, and returns their keywords.
async fn change(
    state: &AppState,
    paths: Vec<String>,
    names: Vec<String>,
    apply: impl FnOnce(&Catalogue, &[PhotoId], &[String]) -> Result<(), app_core::CatalogueError>
    + Send
    + 'static,
) -> IpcResult<Vec<KeywordUseDto>> {
    if names.len() > MAX_NAMES {
        return Err(IpcError::internal(format!(
            "{} keywords at once; at most {MAX_NAMES}",
            names.len()
        )));
    }
    let files = granted_files(state, &paths)?;
    let catalogue = Arc::clone(&state.catalogue);
    let sidecars = state.settings.get().library.write_sidecars && !names.is_empty();
    let raw = raw_extensions(&state.engine.info().extensions);
    let found = tauri::async_runtime::spawn_blocking(move || {
        let mut photos = Vec::with_capacity(files.len());
        for (file, root) in &files {
            photos.push(library_photo(&catalogue, file, root)?);
        }
        if !names.is_empty() {
            apply(&catalogue, &photos, &names)?;
        }
        if sidecars {
            let changed: Vec<PathBuf> = files.iter().map(|(file, _)| file.clone()).collect();
            app_core::sidecars::sync_files(&catalogue, &changed, &raw, true);
        }
        catalogue.keywords_on(&photos)
    })
    .await
    .map_err(IpcError::internal)?
    .map_err(IpcError::internal)?;
    Ok(uses(found))
}
