//! Presets (ADR 0046): the design's built-in looks and the photographer's own, which
//! the catalogue stores as opaque recipe JSON. A preset holds only a look
//! ([`EditRecipe::look_only`]); the editor applies it with
//! [`EditRecipe::with_look_of`].

use std::path::Path;

use catalogue::{Catalogue, PresetId, StoredPreset};
use renderer::presets::PresetFileError;
use renderer::{EditRecipe, RECIPE_VERSION};

use crate::lightroom::{self, LightroomError};

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

/// The extension of the app's preset files (ADR 0047).
pub const PRESET_FILE_EXTENSION: &str = "preset";
/// Largest preset file read: presets are a few kilobytes; anything far larger is not
/// one.
const MAX_PRESET_FILE_BYTES: u64 = 2 * 1024 * 1024;

/// A preset brought in from a file.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedPreset {
    pub preset: Preset,
    /// Read from a Lightroom `.xmp` preset.
    pub from_lightroom: bool,
    /// Lightroom settings with no counterpart here, by Lightroom's names.
    pub left_out: Vec<&'static str>,
}

/// Writes preset `id` to `dest` as a preset file (written whole or not at all).
pub fn export_preset_file(
    catalogue: &Catalogue,
    id: &PresetRef,
    dest: &Path,
) -> Result<(), EngineError> {
    let preset = list_presets(catalogue)?
        .into_iter()
        .find(|p| &p.id == id)
        .ok_or_else(not_found)?;
    let text = renderer::presets::to_file(&preset.name, &preset.recipe);
    export::write_atomic(dest, text.as_bytes())?;
    Ok(())
}

/// A file name for preset `name`: its name without characters file systems refuse.
pub fn preset_file_name(name: &str) -> String {
    let clean: String = name
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() {
                '-'
            } else {
                c
            }
        })
        .collect();
    let clean = clean.trim().trim_start_matches('.');
    format!(
        "{}.{PRESET_FILE_EXTENSION}",
        if clean.is_empty() { "Preset" } else { clean }
    )
}

/// Reads the preset file at `path` (the app's own, or a Lightroom `.xmp`) and saves
/// its look as a new preset, named as in the file (or after the file).
pub fn import_preset_file(
    catalogue: &Catalogue,
    path: &Path,
) -> Result<ImportedPreset, EngineError> {
    let file = path.file_name().map_or_else(
        || "The file".to_owned(),
        |f| f.to_string_lossy().into_owned(),
    );
    let unreadable = |detail: String| {
        EngineError::new(
            ErrorKind::InvalidInput,
            format!("“{file}” isn’t a preset this app can read."),
            detail,
        )
    };
    let size = std::fs::metadata(path)
        .map_err(|e| unreadable(format!("{}: {e}", path.display())))?
        .len();
    if size > MAX_PRESET_FILE_BYTES {
        return Err(unreadable(format!("{} is {size} bytes", path.display())));
    }
    let bytes = std::fs::read(path).map_err(|e| unreadable(format!("{}: {e}", path.display())))?;
    let text = String::from_utf8_lossy(&bytes);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let extension = |ext: &str| {
        path.extension()
            .is_some_and(|e| e.eq_ignore_ascii_case(ext))
    };
    let is_xmp = extension("xmp") || text.contains("camera-raw-settings");
    // Lightroom before 7.3 wrote presets as a Lua table (ADR 0051).
    let is_lrtemplate =
        !is_xmp && (extension("lrtemplate") || text.trim_start().starts_with("s = {"));
    let (name, look, left_out) = if is_xmp || is_lrtemplate {
        let read = if is_xmp {
            lightroom::read_xmp(&text)
        } else {
            lightroom::read_lrtemplate(&text)
        };
        match read {
            Ok(p) => (p.name.unwrap_or(stem), p.recipe, p.left_out),
            Err(LightroomError::NotAPreset) => {
                return Err(unreadable(format!(
                    "{}: not a Camera Raw preset",
                    path.display()
                )));
            }
            Err(LightroomError::NothingUsable) => {
                return Err(EngineError::new(
                    ErrorKind::InvalidInput,
                    format!("“{file}” has no settings this app can use."),
                    format!("{}: no mapped settings", path.display()),
                ));
            }
        }
    } else {
        match renderer::presets::from_file(&text) {
            Ok((name, look)) => (
                if name.trim().is_empty() { stem } else { name },
                look,
                Vec::new(),
            ),
            Err(PresetFileError::NotAPreset) => {
                return Err(unreadable(format!("{}: not a preset file", path.display())));
            }
            Err(PresetFileError::TooNew) => {
                return Err(EngineError::new(
                    ErrorKind::Unsupported,
                    format!("“{file}” was made by a newer version of the app."),
                    format!("{}: newer preset file", path.display()),
                ));
            }
        }
    };
    let preset = create_preset(catalogue, &name, &look)?;
    Ok(ImportedPreset {
        preset,
        from_lightroom: is_xmp || is_lrtemplate,
        left_out,
    })
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
    fn presets_go_out_to_files_and_come_back() {
        let dir = fixtures::TempDir::new("preset-files");
        let cat = Catalogue::open_in_memory().unwrap();
        let p = create_preset(&cat, "Soft / warm", &edited()).unwrap();
        let file = dir.path().join(preset_file_name(&p.name));
        assert_eq!(file.file_name().unwrap(), "Soft - warm.preset");
        export_preset_file(&cat, &p.id, &file).unwrap();
        let back = import_preset_file(&cat, &file).unwrap();
        assert!(!back.from_lightroom && back.left_out.is_empty());
        assert_eq!(
            (back.preset.name.as_str(), &back.preset.recipe),
            ("Soft / warm", &p.recipe)
        );
        assert_ne!(back.preset.id, p.id);
        // Built-in presets can be exported too.
        let mono = dir.path().join("mono.preset");
        export_preset_file(&cat, &PresetRef::BuiltIn("mono".into()), &mono).unwrap();
        assert_eq!(
            import_preset_file(&cat, &mono)
                .unwrap()
                .preset
                .recipe
                .saturation,
            -100.0
        );
    }

    #[test]
    fn lightroom_presets_are_imported_with_what_was_left_out() {
        let cat = Catalogue::open_in_memory().unwrap();
        let sample = std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/presets/lightroom-sample.xmp"
        ));
        let got = import_preset_file(&cat, sample).unwrap();
        assert!(got.from_lightroom);
        assert_eq!(got.preset.name, "Soft & Warm");
        assert_eq!(got.preset.recipe.contrast, 18.0);
        assert_eq!(got.left_out, ["Color Grading", "Masks and healing"]);
        assert_eq!(list_presets(&cat).unwrap().len(), 7);
    }

    #[test]
    fn older_lightroom_presets_are_imported_too() {
        let cat = Catalogue::open_in_memory().unwrap();
        let sample = std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/presets/lightroom-sample.lrtemplate"
        ));
        let got = import_preset_file(&cat, sample).unwrap();
        assert!(got.from_lightroom);
        assert_eq!(got.preset.name, "Faded \"Film\"");
        assert_eq!(got.preset.recipe.contrast, -15.0);
    }

    #[test]
    fn files_that_are_not_presets_are_refused_clearly() {
        let dir = fixtures::TempDir::new("preset-refused");
        let cat = Catalogue::open_in_memory().unwrap();
        let junk = dir.path().join("notes.preset");
        std::fs::write(&junk, "shopping list").unwrap();
        let e = import_preset_file(&cat, &junk).unwrap_err();
        assert_eq!(e.kind, ErrorKind::InvalidInput);
        assert_eq!(
            e.message,
            "“notes.preset” isn’t a preset this app can read."
        );
        let big = dir.path().join("big.xmp");
        std::fs::write(&big, vec![b' '; (MAX_PRESET_FILE_BYTES + 1) as usize]).unwrap();
        assert_eq!(
            import_preset_file(&cat, &big).unwrap_err().kind,
            ErrorKind::InvalidInput
        );
        let missing = import_preset_file(&cat, &dir.path().join("gone.xmp")).unwrap_err();
        assert_eq!(missing.kind, ErrorKind::InvalidInput);
        assert_eq!(list_presets(&cat).unwrap().len(), 6);
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
