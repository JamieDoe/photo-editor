//! Loading and saving photos' edit recipes (ADR 0019): the bridge between the
//! catalogue (which stores recipes as opaque JSON) and the renderer (which owns the
//! recipe schema and its migrations).

use catalogue::{Catalogue, PhotoId, StoredEdit};
use renderer::{EditRecipe, RECIPE_VERSION};

use crate::{EngineError, ErrorKind};

/// A photo's saved edit, as far as this version of the app understands it.
#[derive(Debug, Clone, PartialEq)]
pub enum SavedEdit {
    /// No edit: the photo looks as shot.
    None,
    Recipe(EditRecipe),
    /// Written by a newer version of the app. Shown unedited here, and never
    /// overwritten, so opening the library in an older version loses nothing.
    TooNew {
        version: u32,
    },
}

impl SavedEdit {
    /// Interprets a stored recipe, migrating older schema versions.
    pub fn from_stored(stored: Option<&StoredEdit>) -> Self {
        let Some(stored) = stored else {
            return Self::None;
        };
        if stored.recipe_version > RECIPE_VERSION {
            return Self::TooNew {
                version: stored.recipe_version,
            };
        }
        match EditRecipe::from_json(&stored.json) {
            Ok(r) if r.is_identity() => Self::None,
            Ok(r) => Self::Recipe(r),
            Err(renderer::RecipeError::UnsupportedVersion(version)) => Self::TooNew { version },
            Err(e) => {
                // Unreadable: treat as unedited; the next save replaces it.
                log::warn!("ignoring an unreadable saved edit: {e}");
                Self::None
            }
        }
    }

    /// The recipe to render, if the photo is edited (and readable).
    pub fn recipe(&self) -> Option<EditRecipe> {
        match self {
            Self::Recipe(r) => Some(*r),
            Self::None | Self::TooNew { .. } => None,
        }
    }
}

/// Loads `photo`'s edit.
pub fn load_edit(catalogue: &Catalogue, photo: PhotoId) -> Result<SavedEdit, EngineError> {
    Ok(SavedEdit::from_stored(catalogue.edit_of(photo)?.as_ref()))
}

/// Saves `recipe` as `photo`'s edit; an identity recipe removes the edit. Returns
/// whether the photo is now edited. Refuses to overwrite an edit from a newer version.
pub fn save_edit(
    catalogue: &Catalogue,
    photo: PhotoId,
    recipe: &EditRecipe,
) -> Result<bool, EngineError> {
    if let Some(existing) = catalogue.edit_of(photo)?
        && existing.recipe_version > RECIPE_VERSION
    {
        return Err(EngineError::new(
            ErrorKind::Unsupported,
            "This photo was edited in a newer version of the app, so changes made here aren’t saved.",
            format!(
                "stored recipe version {} > supported {RECIPE_VERSION}",
                existing.recipe_version
            ),
        ));
    }
    let r = recipe.sanitized();
    if r.is_identity() {
        catalogue.set_edit(photo, None)?;
        return Ok(false);
    }
    catalogue.set_edit(photo, Some((RECIPE_VERSION, &r.to_json())))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stored(version: u32, json: &str) -> StoredEdit {
        StoredEdit {
            recipe_version: version,
            json: json.to_owned(),
            updated_at_ms: 0,
        }
    }

    #[test]
    fn stored_recipes_are_interpreted_safely() {
        assert_eq!(SavedEdit::from_stored(None), SavedEdit::None);
        let r = SavedEdit::from_stored(Some(&stored(1, r#"{"version":1,"exposure":1.5}"#)));
        assert_eq!(r.recipe().map(|r| r.exposure), Some(1.5));
        // An identity recipe is no edit.
        assert_eq!(
            SavedEdit::from_stored(Some(&stored(1, r#"{"version":1}"#))),
            SavedEdit::None
        );
        // Newer schema, by column or by the JSON's own version: never interpreted.
        assert_eq!(
            SavedEdit::from_stored(Some(&stored(RECIPE_VERSION + 1, "{}"))),
            SavedEdit::TooNew {
                version: RECIPE_VERSION + 1
            }
        );
        assert!(matches!(
            SavedEdit::from_stored(Some(&stored(1, r#"{"version":99}"#))),
            SavedEdit::TooNew { version: 99 }
        ));
        assert_eq!(
            SavedEdit::from_stored(Some(&stored(1, "not json"))),
            SavedEdit::None
        );
    }
}
