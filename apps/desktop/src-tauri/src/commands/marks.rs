//! Ratings, flags (ADR 0018) and colour labels (ADR 0064): setting them, and library-wide collections built
//! from them. Only photos inside granted folders can be marked or listed.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use app_core::{MarkChange, PhotoDetails, Rating};
use tauri::State;

use super::IpcResult;
use super::library::{folder_unavailable, library_photo, raw_extensions};
use crate::AppState;
use crate::ipc::{
    CollectionCountsDto, CollectionKindDto, CollectionListingDto, IpcError, MarkChangeDto,
    PhotoDetailsDto, PhotoEntryDto, SearchResultsDto,
};

/// Applies `change` to the photos at `paths` and returns the new collection counts.
#[tauri::command]
pub async fn set_photo_marks(
    state: State<'_, AppState>,
    paths: Vec<String>,
    change: MarkChangeDto,
) -> IpcResult<CollectionCountsDto> {
    let change = match change {
        MarkChangeDto::Rating { stars } => MarkChange::Rating(
            Rating::new(stars).ok_or_else(|| IpcError::internal(format!("rating {stars} > 5")))?,
        ),
        MarkChangeDto::Flag { flag } => MarkChange::Flag(flag.into()),
        MarkChangeDto::Label { label } => MarkChange::Label(label.into()),
    };
    let files = granted_files(&state, &paths)?;
    let catalogue = Arc::clone(&state.catalogue);
    let counts = tauri::async_runtime::spawn_blocking(move || {
        let mut photos = Vec::with_capacity(files.len());
        for (file, root) in &files {
            photos.push(library_photo(&catalogue, file, root)?);
        }
        catalogue.set_marks(&photos, change)?;
        catalogue.collection_counts()
    })
    .await
    .map_err(IpcError::internal)?
    .map_err(IpcError::internal)?;
    Ok(counts.into())
}

/// Each of `paths` as a granted file and its library root; an error if any is outside
/// the granted folders.
pub(super) fn granted_files(
    state: &AppState,
    paths: &[String],
) -> IpcResult<Vec<(PathBuf, PathBuf)>> {
    paths
        .iter()
        .map(|p| {
            let file = state
                .folders
                .check(Path::new(p))
                .ok_or_else(|| folder_unavailable(p))?;
            let root = state
                .folders
                .root_of(&file)
                .ok_or_else(|| folder_unavailable(p))?;
            Ok((file, root))
        })
        .collect()
}

/// The present photos of a library-wide collection that are inside granted folders.
#[tauri::command]
pub async fn library_collection(
    state: State<'_, AppState>,
    kind: CollectionKindDto,
) -> IpcResult<CollectionListingDto> {
    let catalogue = Arc::clone(&state.catalogue);
    let entries = tauri::async_runtime::spawn_blocking(move || catalogue.collection(kind.into()))
        .await
        .map_err(IpcError::internal)?
        .map_err(IpcError::internal)?;
    let raw = raw_extensions(&state.engine.info().extensions);
    let photos = entries
        .into_iter()
        .filter(|e| state.folders.covers(&e.path))
        .map(|e| {
            entry_dto(
                &e.path,
                e.size,
                e.modified_ns,
                &raw,
                e.details.as_ref(),
                e.marks,
                e.edited,
            )
        })
        .collect();
    Ok(CollectionListingDto { kind, photos })
}

/// The present photos inside granted folders whose file (from the library folder
/// down), camera, lens or capture date match every word of `query` (ADR 0056).
#[tauri::command]
pub async fn search_library(
    state: State<'_, AppState>,
    query: String,
) -> IpcResult<SearchResultsDto> {
    let catalogue = Arc::clone(&state.catalogue);
    let q = query.clone();
    let entries = tauri::async_runtime::spawn_blocking(move || catalogue.search(&q))
        .await
        .map_err(IpcError::internal)?
        .map_err(IpcError::internal)?;
    let raw = raw_extensions(&state.engine.info().extensions);
    let photos = entries
        .into_iter()
        .filter(|e| state.folders.covers(&e.path))
        .map(|e| {
            entry_dto(
                &e.path,
                e.size,
                e.modified_ns,
                &raw,
                e.details.as_ref(),
                e.marks,
                e.edited,
            )
        })
        .collect();
    Ok(SearchResultsDto { query, photos })
}

pub(super) fn entry_dto(
    path: &Path,
    size: u64,
    modified_ns: i64,
    raw_extensions: &[&str],
    details: Option<&PhotoDetails>,
    marks: app_core::Marks,
    edited: bool,
) -> PhotoEntryDto {
    let extension = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    PhotoEntryDto {
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        path: path.display().to_string(),
        size_bytes: size,
        modified_ms: (modified_ns.max(0) / 1_000_000) as u64,
        raw: raw_extensions.contains(&extension.as_str()),
        details: details.map(PhotoDetailsDto::from),
        marks: marks.into(),
        edited,
    }
}
