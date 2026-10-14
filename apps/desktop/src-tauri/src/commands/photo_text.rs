//! A photo's words (ADR 0089): its title and caption, and its own creator and
//! copyright. Read and set for several photos at once; only photos inside granted
//! folders. Exports write them, with the settings' creator and copyright for photos
//! without their own.

use std::sync::Arc;

use app_core::{PhotoText, TextField};
use tauri::State;

use super::IpcResult;
use super::library::library_photo;
use super::marks::granted_files;
use crate::AppState;
use crate::ipc::{IpcError, PhotoTextDto, SharedTextDto, TextFieldDto};

/// The words of the photos at `paths`, field by field: shared, or mixed.
#[tauri::command]
pub async fn photo_text(state: State<'_, AppState>, paths: Vec<String>) -> IpcResult<PhotoTextDto> {
    change(&state, paths, None).await
}

/// Sets `field` of the photos at `paths` to `text` (empty clears it); their words
/// afterwards.
#[tauri::command]
pub async fn set_photo_text(
    state: State<'_, AppState>,
    paths: Vec<String>,
    field: TextFieldDto,
    text: String,
) -> IpcResult<PhotoTextDto> {
    let field = TextField::from(field);
    // Longer than any field keeps: the catalogue cuts it to the field's length.
    if text.chars().count() > field.max_chars() * 2 {
        return Err(IpcError::internal(format!(
            "{} characters; at most {}",
            text.chars().count(),
            field.max_chars()
        )));
    }
    change(&state, paths, Some((field, text))).await
}

async fn change(
    state: &AppState,
    paths: Vec<String>,
    set: Option<(TextField, String)>,
) -> IpcResult<PhotoTextDto> {
    let files = granted_files(state, &paths)?;
    let catalogue = Arc::clone(&state.catalogue);
    let texts = tauri::async_runtime::spawn_blocking(move || {
        let mut photos = Vec::with_capacity(files.len());
        for (file, root) in &files {
            photos.push(library_photo(&catalogue, file, root)?);
        }
        if let Some((field, text)) = &set {
            catalogue.set_photo_text(&photos, *field, text)?;
        }
        catalogue.photo_text(&photos)
    })
    .await
    .map_err(IpcError::internal)?
    .map_err(IpcError::internal)?;
    let export = state.settings.get().export;
    let given = |s: &str| Some(s.trim().to_owned()).filter(|s| !s.is_empty());
    Ok(PhotoTextDto {
        title: shared(&texts, TextField::Title),
        caption: shared(&texts, TextField::Caption),
        creator: shared(&texts, TextField::Creator),
        copyright: shared(&texts, TextField::Copyright),
        default_creator: given(&export.creator),
        default_copyright: given(&export.copyright),
    })
}

/// `field` across `texts`: the one value they share, or mixed.
fn shared(texts: &[PhotoText], field: TextField) -> SharedTextDto {
    let first = texts.first().and_then(|t| t.get(field));
    if texts.iter().all(|t| t.get(field) == first) {
        SharedTextDto {
            value: first.map(str::to_owned),
            mixed: false,
        }
    } else {
        SharedTextDto {
            value: None,
            mixed: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_field_is_shared_or_mixed() {
        let text = |title: Option<&str>| PhotoText {
            title: title.map(str::to_owned),
            ..Default::default()
        };
        let same = [text(Some("Heron")), text(Some("Heron"))];
        assert_eq!(
            shared(&same, TextField::Title),
            SharedTextDto {
                value: Some("Heron".into()),
                mixed: false
            }
        );
        assert!(shared(&[text(Some("Heron")), text(None)], TextField::Title).mixed);
        assert_eq!(
            shared(&[text(None), text(None)], TextField::Title),
            SharedTextDto {
                value: None,
                mixed: false
            }
        );
        assert!(!shared(&[], TextField::Caption).mixed);
    }
}
