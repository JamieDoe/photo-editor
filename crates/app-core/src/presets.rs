//! Presets (ADR 0046): the design's built-in looks and the photographer's own, which
//! the catalogue stores as opaque recipe JSON. A preset holds only a look
//! ([`EditRecipe::look_only`]); the editor applies it with
//! [`EditRecipe::with_look_of`].

use catalogue::{Catalogue, PresetId, StoredPreset};
use renderer::{EditRecipe, RECIPE_VERSION};

use crate::{EngineError, ErrorKind};

/// Which preset: one of the built-in looks, or one the photographer saved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresetRef {
    BuiltIn(String),
    User(PresetId),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Preset {
    pub id: PresetRef,
    pub name: String,
    pub recipe: EditRecipe,
}

/// Longest preset name, in characters.
pub const MAX_PRESET_NAME: usize = 60;

/// The built-in presets, then the photographer's, oldest first. A saved preset this
/// version cannot read (written by a newer one, or damaged) is left out, not lost.
pub fn list_presets(catalogue: &Catalogue) -> Result<Vec<Preset>, EngineError> {
    let builtin = renderer::presets::builtin().into_iter().map(|p| Preset {
        id: PresetRef::BuiltIn(p.id.to_owned()),
        name: p.name.to_owned(),
        recipe: p.recipe,
    });
    let saved = catalogue
        .presets()?
        .into_iter()
        .filter_map(|p| match read(&p) {
            Ok(recipe) => Some(Preset {
                id: PresetRef::User(p.id),
                name: p.name,
                recipe,
            }),
            Err(e) => {
                log::warn!("leaving out preset {:?}: {e}", p.id);
                None
            }
        });
    Ok(builtin.chain(saved).collect())
}

/// Saves the look of `recipe` as a new preset called `name`.
pub fn create_preset(
    catalogue: &Catalogue,
    name: &str,
    recipe: &EditRecipe,
) -> Result<Preset, EngineError> {
    let name = clean_name(name)?;
    let look = recipe.sanitized().look_only();
    let id = catalogue.add_preset(&name, RECIPE_VERSION, &look.to_json())?;
    Ok(Preset {
        id: PresetRef::User(id),
        name,
        recipe: look,
    })
}

pub fn rename_preset(catalogue: &Catalogue, id: &PresetRef, name: &str) -> Result<(), EngineError> {
    let name = clean_name(name)?;
    let found = catalogue.rename_preset(user(id)?, &name)?;
    if found { Ok(()) } else { Err(not_found()) }
}

/// Replaces a saved preset's look with that of `recipe`.
pub fn update_preset(
    catalogue: &Catalogue,
    id: &PresetRef,
    recipe: &EditRecipe,
) -> Result<EditRecipe, EngineError> {
    let look = recipe.sanitized().look_only();
    let found = catalogue.set_preset_recipe(user(id)?, RECIPE_VERSION, &look.to_json())?;
    if found { Ok(look) } else { Err(not_found()) }
}

pub fn delete_preset(catalogue: &Catalogue, id: &PresetRef) -> Result<(), EngineError> {
    let found = catalogue.delete_preset(user(id)?)?;
    if found { Ok(()) } else { Err(not_found()) }
}

fn read(p: &StoredPreset) -> Result<EditRecipe, renderer::RecipeError> {
    if p.recipe_version > RECIPE_VERSION {
        return Err(renderer::RecipeError::UnsupportedVersion(p.recipe_version));
    }
    Ok(EditRecipe::from_json(&p.json)?.look_only())
}

/// A saved preset's id; built-in presets cannot be changed.
fn user(id: &PresetRef) -> Result<PresetId, EngineError> {
    match id {
        PresetRef::User(id) => Ok(*id),
        PresetRef::BuiltIn(_) => Err(EngineError::new(
            ErrorKind::Unsupported,
            "Built-in presets can’t be changed. Save your own to change it.",
            "attempt to change a built-in preset",
        )),
    }
}

fn clean_name(name: &str) -> Result<String, EngineError> {
    let name: String = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        return Err(EngineError::new(
            ErrorKind::InvalidInput,
            "Give the preset a name.",
            "empty preset name",
        ));
    }
    Ok(name.chars().take(MAX_PRESET_NAME).collect())
}

fn not_found() -> EngineError {
    EngineError::new(
        ErrorKind::NotFound,
        "That preset no longer exists.",
        "preset not found",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edited() -> EditRecipe {
        EditRecipe {
            exposure: 1.0,
            contrast: 20.0,
            vibrance: 15.0,
            geometry: Some(renderer::Geometry {
                straighten: 3.0,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn lists_built_ins_then_saved_presets() {
        let cat = Catalogue::open_in_memory().unwrap();
        let before = list_presets(&cat).unwrap();
        assert_eq!(before.len(), 6);
        assert_eq!(before[0].id, PresetRef::BuiltIn("natural".into()));
        let saved = create_preset(&cat, "  My   look ", &edited()).unwrap();
        assert_eq!(saved.name, "My look");
        // Only the look is kept.
        assert_eq!((saved.recipe.exposure, saved.recipe.contrast), (0.0, 20.0));
        assert!(saved.recipe.geometry.is_none());
        let all = list_presets(&cat).unwrap();
        assert_eq!(all.len(), 7);
        assert_eq!(all[6], saved);
    }

    #[test]
    fn saved_presets_are_renamed_updated_and_deleted_built_ins_are_not() {
        let cat = Catalogue::open_in_memory().unwrap();
        let p = create_preset(&cat, "Soft", &edited()).unwrap();
        rename_preset(&cat, &p.id, "Softer").unwrap();
        let look = update_preset(
            &cat,
            &p.id,
            &EditRecipe {
                clarity: 30.0,
                ..edited()
            },
        )
        .unwrap();
        assert_eq!(look.clarity, 30.0);
        let listed = list_presets(&cat).unwrap().pop().unwrap();
        assert_eq!(
            (listed.name.as_str(), listed.recipe.clarity),
            ("Softer", 30.0)
        );
        delete_preset(&cat, &p.id).unwrap();
        assert_eq!(list_presets(&cat).unwrap().len(), 6);
        assert_eq!(
            delete_preset(&cat, &p.id).unwrap_err().kind,
            ErrorKind::NotFound
        );

        let mono = PresetRef::BuiltIn("mono".into());
        assert_eq!(
            delete_preset(&cat, &mono).unwrap_err().kind,
            ErrorKind::Unsupported
        );
        assert_eq!(
            rename_preset(&cat, &mono, "B&W").unwrap_err().kind,
            ErrorKind::Unsupported
        );
        assert_eq!(
            create_preset(&cat, "   ", &edited()).unwrap_err().kind,
            ErrorKind::InvalidInput
        );
    }

    #[test]
    fn unreadable_saved_presets_are_left_out() {
        let cat = Catalogue::open_in_memory().unwrap();
        cat.add_preset("Future", RECIPE_VERSION + 1, "{}").unwrap();
        cat.add_preset("Broken", RECIPE_VERSION, "not json")
            .unwrap();
        create_preset(&cat, "Fine", &edited()).unwrap();
        let names: Vec<_> = list_presets(&cat)
            .unwrap()
            .into_iter()
            .skip(6)
            .map(|p| p.name)
            .collect();
        assert_eq!(names, ["Fine"]);
    }
}
