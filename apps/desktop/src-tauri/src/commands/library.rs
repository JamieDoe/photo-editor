//! Library commands: choosing folders (native dialog) and browsing within them.

use std::path::{Path, PathBuf};

use folders::FolderListing;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

use super::IpcResult;
use crate::AppState;
use crate::ipc::{
    FolderCrumbDto, FolderListingDto, INDEX_EVENT, IndexEvent, IpcError, IpcErrorKind,
    LibraryStatusDto, PhotoDetailsDto, PhotoEntryDto, SettingsViewDto,
};

/// Error for a folder that is not (or no longer) available: moved, deleted, on an
/// unplugged drive, or never granted.
fn folder_unavailable(path: &str) -> IpcError {
    let reference = crate::logging::new_reference();
    log::warn!("[{reference}] folder unavailable or not granted: {path}");
    IpcError {
        kind: IpcErrorKind::NotFound,
        message: "This folder isn’t available. It may have been moved, or its drive disconnected. Choose it again with “Choose folder…”.".into(),
        reference: Some(reference),
    }
}

/// Shows the native folder picker, grants the chosen folder, remembers it and lists it.
/// Returns `None` if the user cancelled.
#[tauri::command]
pub async fn choose_folder(
    app: AppHandle,
    state: State<'_, AppState>,
) -> IpcResult<Option<FolderListingDto>> {
    let picked =
        tauri::async_runtime::spawn_blocking(move || app.dialog().file().blocking_pick_folder())
            .await
            .map_err(IpcError::internal)?;
    let Some(folder) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    let granted = state
        .folders
        .grant(&folder)
        .map_err(|_| folder_unavailable(&folder.display().to_string()))?;
    let as_string = granted.display().to_string();
    if let Err(e) = state.settings.modify(|s| s.with_recent_folder(&as_string)) {
        log::warn!("could not remember recent folder: {e}");
    }
    log::info!("folder granted: {as_string}");
    list(&state, granted).await.map(Some)
}

/// Lists a folder inside a granted folder.
#[tauri::command]
pub async fn list_folder(state: State<'_, AppState>, path: String) -> IpcResult<FolderListingDto> {
    let dir = state
        .folders
        .check(Path::new(&path))
        .ok_or_else(|| folder_unavailable(&path))?;
    list(&state, dir).await
}

/// Makes a granted folder the Library's default at start-up.
#[tauri::command]
pub fn set_default_folder(state: State<'_, AppState>, path: String) -> IpcResult<SettingsViewDto> {
    let dir = state
        .folders
        .check(Path::new(&path))
        .ok_or_else(|| folder_unavailable(&path))?;
    let dir = dir.display().to_string();
    state
        .settings
        .modify(|mut s| {
            s.library.default_folder = Some(dir);
            s
        })
        .map_err(IpcError::internal)?;
    Ok(super::settings::view(&state))
}

async fn list(state: &AppState, dir: PathBuf) -> IpcResult<FolderListingDto> {
    let extensions: Vec<&'static str> = state.engine.info().extensions;
    let raw_extensions = raw_extensions(&extensions);
    let root = state.folders.root_of(&dir).unwrap_or_else(|| dir.clone());
    let target = dir.clone();
    let catalogue = std::sync::Arc::clone(&state.catalogue);
    let (listing, details) = tauri::async_runtime::spawn_blocking(move || {
        let listing = folders::list_folder(&target, &extensions);
        // Details exist once the folder has been indexed; a catalogue problem must not
        // stop browsing, so it only costs the extra columns.
        let details = catalogue.details_in_dir(&target).unwrap_or_else(|e| {
            log::warn!(
                "catalogue details unavailable for {}: {e}",
                target.display()
            );
            Vec::new()
        });
        (listing, details)
    })
    .await
    .map_err(IpcError::internal)?;
    let listing = listing.map_err(|_| folder_unavailable(&dir.display().to_string()))?;
    let details: std::collections::HashMap<PathBuf, app_core::PhotoDetails> =
        details.into_iter().collect();
    Ok(to_dto(listing, &root, &raw_extensions, &details))
}

/// Extensions of camera RAW formats (everything the registry accepts except JPEG).
fn raw_extensions(all: &[&'static str]) -> Vec<&'static str> {
    all.iter()
        .copied()
        .filter(|e| !matches!(*e, "jpg" | "jpeg"))
        .collect()
}

fn to_dto(
    listing: FolderListing,
    root: &Path,
    raw_extensions: &[&str],
    details: &std::collections::HashMap<PathBuf, app_core::PhotoDetails>,
) -> FolderListingDto {
    let crumb = |p: &Path| FolderCrumbDto {
        name: p.file_name().map_or_else(
            || p.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        ),
        path: p.display().to_string(),
    };
    // Breadcrumbs stop at the granted root: the user cannot navigate above it.
    let mut breadcrumbs: Vec<FolderCrumbDto> = listing
        .path
        .ancestors()
        .take_while(|p| p.starts_with(root))
        .map(crumb)
        .collect();
    breadcrumbs.reverse();
    FolderListingDto {
        name: crumb(&listing.path).name,
        path: listing.path.display().to_string(),
        breadcrumbs,
        folders: listing.folders.iter().map(|f| crumb(&f.path)).collect(),
        photos: listing
            .photos
            .into_iter()
            .map(|p| PhotoEntryDto {
                raw: raw_extensions.contains(&p.extension.as_str()),
                details: details.get(&p.path).map(PhotoDetailsDto::from),
                name: p.name,
                path: p.path.display().to_string(),
                size_bytes: p.size_bytes,
                modified_ms: p.modified_ms,
            })
            .collect(),
        skipped: listing.skipped as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breadcrumbs_start_at_the_granted_root() {
        let dir = fixtures::TempDir::new("library-crumbs");
        let root = dir.path().join("Photos");
        let deep = root.join("2024/Trip");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("A.NEF"), b"x").unwrap();
        std::fs::write(deep.join("b.jpg"), b"x").unwrap();
        let listing = folders::list_folder(&deep, &["nef", "jpg"]).unwrap();
        let dto = to_dto(
            listing,
            &root.canonicalize().unwrap(),
            &["nef"],
            &Default::default(),
        );
        let names: Vec<_> = dto.breadcrumbs.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Photos", "2024", "Trip"]);
        assert_eq!(dto.name, "Trip");
        assert_eq!(
            dto.photos
                .iter()
                .map(|p| (p.name.as_str(), p.raw))
                .collect::<Vec<_>>(),
            [("A.NEF", true), ("b.jpg", false)]
        );
    }

    #[test]
    fn raw_extensions_exclude_jpeg() {
        assert_eq!(
            raw_extensions(&["jpg", "jpeg", "nef", "cr3"]),
            ["nef", "cr3"]
        );
    }
}

/// Indexes the granted library folder containing `path` (the whole tree, in the
/// background). Progress and the result arrive as `library://index` events.
#[tauri::command]
pub fn index_library_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> IpcResult<()> {
    let root = state
        .folders
        .root_of(Path::new(&path))
        .ok_or_else(|| folder_unavailable(&path))?;
    let root_str = root.display().to_string();
    let root_path = root.clone();
    let progress_app = app.clone();
    let progress_root = root_str.clone();
    let handle =
        state
            .engine
            .index_folder(std::sync::Arc::clone(&state.catalogue), root, move |p| {
                let _ = progress_app.emit(
                    INDEX_EVENT,
                    IndexEvent::Progress {
                        root: progress_root.clone(),
                        stage: p.stage.into(),
                        total: p.total as u32,
                        processed: p.processed as u32,
                    },
                );
            });
    state
        .indexing
        .lock()
        .expect("indexing lock")
        .insert(root_str.clone(), handle.token().clone());

    tauri::async_runtime::spawn(async move {
        let result = super::wait(handle).await;
        if let Some(state) = app.try_state::<AppState>() {
            state
                .indexing
                .lock()
                .expect("indexing lock")
                .remove(&root_str);
        }
        let event = match result {
            Ok(s) => {
                pregenerate_thumbnails(&app, root_path, root_str.clone());
                IndexEvent::Finished {
                    root: root_str,
                    found: s.found as u32,
                    new: s.new as u32,
                    changed: s.changed as u32,
                    moved: s.moved as u32,
                    missing: s.missing as u32,
                    skipped: s.skipped as u32,
                    details_read: s.details_read as u32,
                    total_ms: s.total_ms,
                }
            }
            // Superseded by a newer pass of the same folder: nothing to report.
            Err(e) if e.kind == crate::ipc::IpcErrorKind::Cancelled => return,
            Err(error) => IndexEvent::Failed {
                root: root_str,
                error,
            },
        };
        let _ = app.emit(INDEX_EVENT, event);
    });
    Ok(())
}

/// The thumbnail of a photo inside a granted folder, as JPEG bytes. Cached on disk;
/// otherwise made on the browse lane, so it never waits for indexing or export.
#[tauri::command]
pub async fn library_thumbnail(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> IpcResult<tauri::ipc::Response> {
    let file = state
        .folders
        .check(Path::new(&path))
        .ok_or_else(|| folder_unavailable(&path))?;
    // A cache hit is a small file read; keep it off the async runtime's threads too.
    let handle = tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppState>().engine.thumbnail(file)
    })
    .await
    .map_err(IpcError::internal)?;
    let thumbnail = super::wait(handle).await?;
    Ok(tauri::ipc::Response::new(thumbnail.jpeg))
}

/// Cancels a pending thumbnail request (its row scrolled out of view).
#[tauri::command]
pub fn cancel_thumbnail(state: State<'_, AppState>, path: String) {
    if let Some(file) = state.folders.check(Path::new(&path)) {
        state.engine.cancel_thumbnail(&file);
    }
}

/// After a successful index, makes the library folder's missing thumbnails in the
/// background (idle priority), so browsing finds them ready. Re-indexing the folder
/// replaces the batch.
fn pregenerate_thumbnails(app: &AppHandle, root: PathBuf, key: String) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(state) = app.try_state::<AppState>() else {
            return;
        };
        match state.catalogue.files_in(&root, true) {
            Ok(files) => {
                let paths: Vec<PathBuf> = files
                    .into_iter()
                    .filter(|f| f.status == app_core::FileStatus::Present)
                    .map(|f| f.path)
                    .collect();
                let batch = state.engine.pregenerate_thumbnails(&key, paths);
                log::info!("queued {} thumbnails for {key}", batch.len());
            }
            Err(e) => log::warn!("thumbnail pre-generation skipped for {key}: {e}"),
        }
    });
}

/// Library totals and any catalogue notice.
#[tauri::command]
pub async fn library_status(state: State<'_, AppState>) -> IpcResult<LibraryStatusDto> {
    let catalogue = std::sync::Arc::clone(&state.catalogue);
    let (photos, folders) = tauri::async_runtime::spawn_blocking(move || {
        Ok::<_, app_core::CatalogueError>((catalogue.photo_count()?, catalogue.folders()?))
    })
    .await
    .map_err(IpcError::internal)?
    .map_err(IpcError::internal)?;
    Ok(LibraryStatusDto {
        photos: photos as u64,
        folders: folders
            .into_iter()
            .map(|f| f.path.display().to_string())
            .collect(),
        notice: state.catalogue_notice.clone(),
    })
}
