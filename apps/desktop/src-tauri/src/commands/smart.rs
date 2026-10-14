//! Smart collections (ADR 0084): named rules kept in the catalogue, whose photos are
//! looked up each time. Only photos inside granted folders are listed.

use std::sync::Arc;

use app_core::{CatalogueError, SmartCollectionId, SmartRules};
use tauri::State;

use super::IpcResult;
use super::library::raw_extensions;
use super::marks::entry_dto;
use crate::AppState;
use crate::ipc::{IpcError, SmartCollectionDto, SmartCollectionListingDto, SmartRulesDto};

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

fn dto(collection: app_core::SmartCollection) -> SmartCollectionDto {
    SmartCollectionDto {
        id: collection.id.0,
        name: collection.name,
        rules: collection.rules.into(),
        count: collection.count,
    }
}

async fn find(state: &AppState, id: i64) -> IpcResult<SmartCollectionDto> {
    let catalogue = Arc::clone(&state.catalogue);
    match blocking(move || {
        catalogue
            .smart_collection(SmartCollectionId(id))
            .map_err(catalogue_error)
    })
    .await?
    {
        Some(collection) => Ok(dto(collection)),
        None => Err(app_core::smart_collection_not_found().into()),
    }
}

/// Every smart collection, by name, with its count.
#[tauri::command]
pub async fn list_smart_collections(
    state: State<'_, AppState>,
) -> IpcResult<Vec<SmartCollectionDto>> {
    let catalogue = Arc::clone(&state.catalogue);
    let all = blocking(move || catalogue.smart_collections().map_err(catalogue_error)).await?;
    Ok(all.into_iter().map(dto).collect())
}

/// A new smart collection named `name`, matching `rules`.
#[tauri::command]
pub async fn create_smart_collection(
    state: State<'_, AppState>,
    name: String,
    rules: SmartRulesDto,
) -> IpcResult<SmartCollectionDto> {
    let name = app_core::clean_smart_collection_name(&name)?;
    let rules = app_core::checked_rules(rules.into())?;
    let catalogue = Arc::clone(&state.catalogue);
    let id = blocking(move || {
        catalogue
            .add_smart_collection(&name, &rules)
            .map_err(catalogue_error)
    })
    .await?;
    find(&state, id.0).await
}

/// Renames a smart collection and replaces its rules.
#[tauri::command]
pub async fn update_smart_collection(
    state: State<'_, AppState>,
    id: i64,
    name: String,
    rules: SmartRulesDto,
) -> IpcResult<SmartCollectionDto> {
    let name = app_core::clean_smart_collection_name(&name)?;
    let rules = app_core::checked_rules(rules.into())?;
    let catalogue = Arc::clone(&state.catalogue);
    let updated = blocking(move || {
        catalogue
            .update_smart_collection(SmartCollectionId(id), &name, &rules)
            .map_err(catalogue_error)
    })
    .await?;
    if !updated {
        return Err(app_core::smart_collection_not_found().into());
    }
    find(&state, id).await
}

/// Deletes a smart collection; its photos are untouched.
#[tauri::command]
pub async fn delete_smart_collection(state: State<'_, AppState>, id: i64) -> IpcResult<()> {
    let catalogue = Arc::clone(&state.catalogue);
    blocking(move || {
        catalogue
            .delete_smart_collection(SmartCollectionId(id))
            .map_err(catalogue_error)
    })
    .await?;
    Ok(())
}

/// How many present photos `rules` match, while they are being chosen.
#[tauri::command]
pub async fn count_smart_rules(
    state: State<'_, AppState>,
    rules: SmartRulesDto,
) -> IpcResult<usize> {
    let rules: SmartRules = rules.into();
    let catalogue = Arc::clone(&state.catalogue);
    blocking(move || catalogue.count_matching(&rules).map_err(catalogue_error)).await
}

/// A smart collection and its matching photos inside granted folders.
#[tauri::command]
pub async fn smart_collection_photos(
    state: State<'_, AppState>,
    id: i64,
) -> IpcResult<SmartCollectionListingDto> {
    let collection = find(&state, id).await?;
    let rules: SmartRules = collection.rules.clone().into();
    let catalogue = Arc::clone(&state.catalogue);
    let entries = blocking(move || catalogue.matching(&rules).map_err(catalogue_error)).await?;
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
    Ok(SmartCollectionListingDto { collection, photos })
}
