//! Saving and loading photos' edits (ADR 0019). Only photos inside library folders
//! have saved edits; everything else is edited for the session only.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use app_core::{EditRecipe, SavedEdit};
use tauri::State;

use super::IpcResult;
use super::library::{folder_unavailable, library_photo};
use crate::AppState;
use crate::ipc::{
    EditSavedDto, EditSavingDto, FileFailureDto, IpcError, PastedEditsDto, PastedPhotoDto,
};

/// The saved edit of the photo at `path` and whether its edits are saved. Never fails:
/// a catalogue problem means "not saved" rather than a photo that cannot be opened.
pub(super) async fn saved_edit(
    state: &AppState,
    path: &Path,
) -> (Option<EditRecipe>, EditSavingDto) {
    if state.folders.check(path).is_none() {
        return (None, EditSavingDto::NotInLibrary);
    }
    let catalogue = Arc::clone(&state.catalogue);
    let path = path.to_path_buf();
    let loaded = tauri::async_runtime::spawn_blocking(move || {
        match catalogue.photo_at(&path)? {
            Some(photo) => app_core::load_edit(&catalogue, photo),
            // Not indexed yet: no edit so far; the first save records it.
            None => Ok(SavedEdit::None),
        }
    })
    .await;
    match loaded {
        Ok(Ok(SavedEdit::Recipe(r))) => (Some(*r), EditSavingDto::Library),
        Ok(Ok(SavedEdit::None)) => (None, EditSavingDto::Library),
        Ok(Ok(SavedEdit::TooNew { .. })) => (None, EditSavingDto::NewerVersion),
        Ok(Err(e)) => {
            log::warn!("saved edit unavailable: {e:?}");
            (None, EditSavingDto::NotInLibrary)
        }
        Err(e) => {
            log::warn!("saved edit lookup failed: {e}");
            (None, EditSavingDto::NotInLibrary)
        }
    }
}

/// Saves `recipe` as the edit of the library photo at `path` (a default recipe removes
/// the edit). Called by the editor's autosave.
#[tauri::command]
pub async fn save_edit(
    state: State<'_, AppState>,
    path: String,
    recipe: EditRecipe,
) -> IpcResult<EditSavedDto> {
    let file: PathBuf = state
        .folders
        .check(Path::new(&path))
        .ok_or_else(|| folder_unavailable(&path))?;
    let root = state
        .folders
        .root_of(&file)
        .ok_or_else(|| folder_unavailable(&path))?;
    let catalogue = Arc::clone(&state.catalogue);
    let edited = tauri::async_runtime::spawn_blocking(move || {
        let photo = library_photo(&catalogue, &file, &root).map_err(app_core::EngineError::from)?;
        app_core::save_edit(&catalogue, photo, &recipe)
    })
    .await
    .map_err(IpcError::internal)?
    .map_err(IpcError::from)?;
    Ok(EditSavedDto { edited })
}

/// Sets the settings of `groups` from `source` on each library photo in `paths` (ADR
/// 0049): pasting or syncing edits onto photos that are not open. Each photo is
/// changed or reported on its own; photos outside the library are refused.
#[tauri::command]
pub async fn paste_edits_to(
    state: State<'_, AppState>,
    paths: Vec<String>,
    source: EditRecipe,
    groups: Vec<String>,
) -> IpcResult<PastedEditsDto> {
    let targets: Vec<(String, Option<(PathBuf, PathBuf)>)> = paths
        .into_iter()
        .map(|p| {
            let file = state.folders.check(Path::new(&p));
            let root = file.as_deref().and_then(|f| state.folders.root_of(f));
            (p, file.zip(root))
        })
        .collect();
    let catalogue = Arc::clone(&state.catalogue);
    let result = tauri::async_runtime::spawn_blocking(move || {
        let mut result = PastedEditsDto {
            applied: Vec::new(),
            failed: Vec::new(),
        };
        for (path, target) in targets {
            let name = Path::new(&path)
                .file_name()
                .map_or_else(|| path.clone(), |n| n.to_string_lossy().into_owned());
            let outcome = match target {
                None => Err("It isn’t in a library folder.".to_owned()),
                Some((file, root)) => library_photo(&catalogue, &file, &root)
                    .map_err(app_core::EngineError::from)
                    .and_then(|photo| {
                        app_core::paste_onto(&catalogue, photo, &file, &source, &groups)
                    })
                    .map_err(|e| {
                        log::warn!("paste onto {}: {}", file.display(), e.detail);
                        e.message
                    }),
            };
            match outcome {
                Ok(edited) => result.applied.push(PastedPhotoDto { path, edited }),
                Err(message) => result.failed.push(FileFailureDto {
                    file: name,
                    message,
                }),
            }
        }
        result
    })
    .await
    .map_err(IpcError::internal)?;
    Ok(result)
}
