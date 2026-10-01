//! Albums (ADR 0055): the photographer's own groups of photos, kept in the catalogue as
//! references to photos. Only photos inside granted folders can be added or listed.

use std::sync::Arc;

use app_core::{AlbumId, CatalogueError, clean_album_name};
use tauri::State;

use super::IpcResult;
use super::library::{library_photo, raw_extensions};
use super::marks::{entry_dto, granted_files};
use crate::AppState;
use crate::ipc::{AlbumDto, AlbumListingDto, IpcError};

/// Runs catalogue work off the async runtime's threads.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, IpcError> + Send + 'static,
) -> IpcResult<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(IpcError::internal)?
}

fn catalogue_error(e: CatalogueError) -> IpcError {
    IpcError::internal(e)
}

/// An album as the library shows it; its cover only if its folder is granted.
fn album_dto(state: &AppState, album: app_core::Album) -> AlbumDto {
    AlbumDto {
        id: album.id.0,
        name: album.name,
        count: album.count,
        cover: album
            .cover
            .filter(|p| state.folders.check(p).is_some())
            .map(|p| p.display().to_string()),
    }
}

fn find(state: &AppState, catalogue: &app_core::Catalogue, id: AlbumId) -> IpcResult<AlbumDto> {
    match catalogue.album(id).map_err(catalogue_error)? {
        Some(album) => Ok(album_dto(state, album)),
        None => Err(app_core::album_not_found().into()),
    }
}

/// Every album, by name.
#[tauri::command]
pub async fn list_albums(state: State<'_, AppState>) -> IpcResult<Vec<AlbumDto>> {
    let catalogue = Arc::clone(&state.catalogue);
    let albums = blocking(move || catalogue.albums().map_err(catalogue_error)).await?;
    Ok(albums.into_iter().map(|a| album_dto(&state, a)).collect())
}

/// A new album named `name`, holding the photos at `paths` (which may be none).
#[tauri::command]
pub async fn create_album(
    state: State<'_, AppState>,
    name: String,
    paths: Vec<String>,
) -> IpcResult<AlbumDto> {
    let name = clean_album_name(&name)?;
    let files = granted_files(&state, &paths)?;
    let catalogue = Arc::clone(&state.catalogue);
    let id = blocking(move || {
        let photos = files
            .iter()
            .map(|(file, root)| library_photo(&catalogue, file, root))
            .collect::<Result<Vec<_>, _>>()
            .map_err(catalogue_error)?;
        let id = catalogue.add_album(&name).map_err(catalogue_error)?;
        catalogue
            .add_to_album(id, &photos)
            .map_err(catalogue_error)?;
        Ok(id)
    })
    .await?;
    find(&state, &state.catalogue, id)
}

#[tauri::command]
pub async fn rename_album(
    state: State<'_, AppState>,
    id: i64,
    name: String,
) -> IpcResult<AlbumDto> {
    let name = clean_album_name(&name)?;
    let catalogue = Arc::clone(&state.catalogue);
    let renamed = blocking(move || {
        catalogue
            .rename_album(AlbumId(id), &name)
            .map_err(catalogue_error)
    })
    .await?;
    if !renamed {
        return Err(app_core::album_not_found().into());
    }
    find(&state, &state.catalogue, AlbumId(id))
}

/// Deletes an album; its photos are untouched.
#[tauri::command]
pub async fn delete_album(state: State<'_, AppState>, id: i64) -> IpcResult<()> {
    let catalogue = Arc::clone(&state.catalogue);
    blocking(move || catalogue.delete_album(AlbumId(id)).map_err(catalogue_error)).await?;
    Ok(())
}

/// Adds the photos at `paths` to an album.
#[tauri::command]
pub async fn add_to_album(
    state: State<'_, AppState>,
    id: i64,
    paths: Vec<String>,
) -> IpcResult<AlbumDto> {
    let files = granted_files(&state, &paths)?;
    let catalogue = Arc::clone(&state.catalogue);
    blocking(move || {
        let photos = files
            .iter()
            .map(|(file, root)| library_photo(&catalogue, file, root))
            .collect::<Result<Vec<_>, _>>()
            .map_err(catalogue_error)?;
        catalogue
            .add_to_album(AlbumId(id), &photos)
            .map_err(catalogue_error)
    })
    .await?;
    find(&state, &state.catalogue, AlbumId(id))
}

/// Takes the photos at `paths` out of an album; the photos are untouched.
#[tauri::command]
pub async fn remove_from_album(
    state: State<'_, AppState>,
    id: i64,
    paths: Vec<String>,
) -> IpcResult<AlbumDto> {
    let files = granted_files(&state, &paths)?;
    let catalogue = Arc::clone(&state.catalogue);
    blocking(move || {
        let mut photos = Vec::with_capacity(files.len());
        for (file, _) in &files {
            if let Some(photo) = catalogue.photo_at(file).map_err(catalogue_error)? {
                photos.push(photo);
            }
        }
        catalogue
            .remove_from_album(AlbumId(id), &photos)
            .map_err(catalogue_error)
    })
    .await?;
    find(&state, &state.catalogue, AlbumId(id))
}

/// An album and its present photos inside granted folders.
#[tauri::command]
pub async fn album_photos(state: State<'_, AppState>, id: i64) -> IpcResult<AlbumListingDto> {
    let album = find(&state, &state.catalogue, AlbumId(id))?;
    let catalogue = Arc::clone(&state.catalogue);
    let entries =
        blocking(move || catalogue.album_photos(AlbumId(id)).map_err(catalogue_error)).await?;
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
                e.edited,
            )
        })
        .collect();
    Ok(AlbumListingDto { album, photos })
}
