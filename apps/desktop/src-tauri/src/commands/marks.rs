//! Ratings and flags (ADR 0018): setting them, and library-wide collections built
//! from them. Only photos inside granted folders can be marked or listed.

use std::path::Path;
use std::sync::Arc;

use app_core::{Catalogue, CatalogueError, MarkChange, PhotoDetails, Rating, SourceIdentity};
use tauri::State;

use super::IpcResult;
use super::library::{folder_unavailable, raw_extensions};
use crate::AppState;
use crate::ipc::{
    CollectionCountsDto, CollectionKindDto, CollectionListingDto, IpcError, MarkChangeDto,
    PhotoDetailsDto, PhotoEntryDto,
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
    };
    let mut files = Vec::with_capacity(paths.len());
    for p in &paths {
        let file = state
            .folders
            .check(Path::new(p))
            .ok_or_else(|| folder_unavailable(p))?;
        let root = state
            .folders
            .root_of(&file)
            .ok_or_else(|| folder_unavailable(p))?;
        files.push((file, root));
    }
    let catalogue = Arc::clone(&state.catalogue);
    let counts = tauri::async_runtime::spawn_blocking(move || {
        let mut photos = Vec::with_capacity(files.len());
        for (file, root) in &files {
            let photo = match catalogue.photo_at(file)? {
                Some(photo) => photo,
                None => record_now(&catalogue, file, root)?,
            };
            photos.push(photo);
        }
        catalogue.set_marks(&photos, change)?;
        catalogue.collection_counts()
    })
    .await
    .map_err(IpcError::internal)?
    .map_err(IpcError::internal)?;
    Ok(counts.into())
}

/// A photo marked before its folder's index reached it: record just this file now. The
/// next full index pass sees it as unchanged.
fn record_now(
    catalogue: &Catalogue,
    file: &Path,
    root: &Path,
) -> Result<app_core::PhotoId, CatalogueError> {
    let folder = catalogue.add_folder(root)?;
    let identity = SourceIdentity::from_path(file)?;
    let scan = catalogue.begin_scan()?;
    Ok(catalogue.record_file(folder, &identity, scan)?.0)
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
        .filter(|e| state.folders.check(&e.path).is_some())
        .map(|e| {
            entry_dto(
                &e.path,
                e.size,
                e.modified_ns,
                &raw,
                e.details.as_ref(),
                e.marks,
            )
        })
        .collect();
    Ok(CollectionListingDto { kind, photos })
}

fn entry_dto(
    path: &Path,
    size: u64,
    modified_ns: i64,
    raw_extensions: &[&str],
    details: Option<&PhotoDetails>,
    marks: app_core::Marks,
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
    }
}
