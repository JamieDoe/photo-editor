//! Presets (ADR 0046): edit recipe templates.
//!
//! A preset is a recipe holding only a look: tone, colour, detail, effects, curves and
//! the colour mixer. What belongs to one photo stays with it when a preset is applied,
//! as in the design: its exposure (each photo needs its own), its crop and geometry,
//! its lens corrections (measured on that photo) and its masks (drawn on it).

use crate::EditRecipe;

/// One of the presets every copy of the app has, as the design lists them.
#[derive(Debug, Clone, PartialEq)]
pub struct BuiltinPreset {
    /// Stable across versions: `"natural"`, `"vivid"`, ...
    pub id: &'static str,
    pub name: &'static str,
    pub recipe: EditRecipe,
}

/// The design's presets, in its order.
pub fn builtin() -> Vec<BuiltinPreset> {
    let look = |id, name, recipe: EditRecipe| BuiltinPreset { id, name, recipe };
    let base = EditRecipe::default;
    vec![
        look(
            "natural",
            "Natural",
            EditRecipe {
                vibrance: 12.0,
                contrast: 8.0,
                shadows: 10.0,
                ..base()
            },
        ),
        look(
            "vivid",
            "Vivid",
            EditRecipe {
                contrast: 25.0,
                vibrance: 35.0,
                saturation: 10.0,
                clarity: 15.0,
                ..base()
            },
        ),
        look(
            "warm-film",
            "Warm film",
            EditRecipe {
                temperature: 30.0,
                contrast: -10.0,
                blacks: 25.0,
                saturation: -10.0,
                vignette: -25.0,
                highlights: -20.0,
                ..base()
            },
        ),
        look(
            "matte",
            "Matte",
            EditRecipe {
                contrast: -25.0,
                blacks: 40.0,
                saturation: -15.0,
                ..base()
            },
        ),
        look(
            "mono",
            "Mono",
            EditRecipe {
                saturation: -100.0,
                contrast: 30.0,
                clarity: 20.0,
                ..base()
            },
        ),
        look(
            "cool-fade",
            "Cool fade",
            EditRecipe {
                temperature: -30.0,
                tint: 6.0,
                blacks: 30.0,
                contrast: -15.0,
                ..base()
            },
        ),
    ]
}

/// What a preset file says it is (ADR 0047).
const FILE_FORMAT: &str = "photo-editor-preset";
/// The preset file layout this version writes and reads.
const FILE_VERSION: u32 = 1;

/// Why a file is not a preset this version can read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresetFileError {
    /// Not a preset file at all.
    NotAPreset,
    /// Written by a newer version of the app.
    TooNew,
}

/// A preset as a file: its name and look, as JSON the app reads back with
/// [`from_file`]. The recipe keeps its own schema version, so older files are
/// migrated like saved edits.
pub fn to_file(name: &str, recipe: &EditRecipe) -> String {
    let recipe: serde_json::Value =
        serde_json::from_str(&recipe.look_only().to_json()).unwrap_or(serde_json::Value::Null);
    let file = serde_json::json!({
        "format": FILE_FORMAT,
        "formatVersion": FILE_VERSION,
        "name": name,
        "recipe": recipe,
    });
    serde_json::to_string_pretty(&file).unwrap_or_default() + "\n"
}

/// Reads a preset file written by [`to_file`]: its name and look (sanitised).
pub fn from_file(text: &str) -> Result<(String, EditRecipe), PresetFileError> {
    let v: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|_| PresetFileError::NotAPreset)?;
    if v.get("format").and_then(|f| f.as_str()) != Some(FILE_FORMAT) {
        return Err(PresetFileError::NotAPreset);
    }
    match v.get("formatVersion").and_then(|n| n.as_u64()) {
        Some(n) if n <= u64::from(FILE_VERSION) => {}
        Some(_) => return Err(PresetFileError::TooNew),
        None => return Err(PresetFileError::NotAPreset),
    }
    let name = v
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or_default()
        .to_owned();
    let recipe = v.get("recipe").ok_or(PresetFileError::NotAPreset)?;
    let recipe = EditRecipe::from_json(&recipe.to_string()).map_err(|e| match e {
        crate::RecipeError::UnsupportedVersion(_) => PresetFileError::TooNew,
        _ => PresetFileError::NotAPreset,
    })?;
    Ok((name, recipe.sanitized().look_only()))
}

impl EditRecipe {
    /// This recipe's look alone, as a preset holds it: without the exposure,
    /// geometry, lens corrections, masks, spots, removals and red-eye corrections, which
    /// belong to the photo.
    pub fn look_only(&self) -> EditRecipe {
        EditRecipe {
            exposure: 0.0,
            geometry: None,
            chromatic_aberration: None,
            profile_corrections: true,
            masks: Vec::new(),
            spots: Vec::new(),
            removals: Vec::new(),
            red_eyes: Vec::new(),
            ..self.clone()
        }
    }

    /// This photo's recipe with `preset`'s look: the preset's settings, keeping this
    /// recipe's exposure, geometry, lens corrections, masks, spots, removals and red-eye
    /// corrections.
    pub fn with_look_of(&self, preset: &EditRecipe) -> EditRecipe {
        EditRecipe {
            version: self.version,
            exposure: self.exposure,
            geometry: self.geometry,
            chromatic_aberration: self.chromatic_aberration,
            profile_corrections: self.profile_corrections,
            masks: self.masks.clone(),
            spots: self.spots.clone(),
            removals: self.removals.clone(),
            red_eyes: self.red_eyes.clone(),
            ..preset.look_only()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::masks::{LocalAdjustments, Mask, MaskShape};

    fn photo() -> EditRecipe {
        EditRecipe {
            exposure: 0.7,
            contrast: 40.0,
            clarity: -20.0,
            geometry: Some(crate::Geometry {
                straighten: 2.0,
                ..Default::default()
            }),
            masks: vec![Mask::new(
                1,
                MaskShape::Linear {
                    start: [0.5, 0.0],
                    end: [0.5, 0.5],
                },
                LocalAdjustments {
                    exposure: -1.0,
                    ..Default::default()
                },
            )],
            spots: vec![crate::retouch::Spot {
                source_x: 0.7,
                ..Default::default()
            }],
            removals: vec![crate::remove::Removal {
                strokes: vec![crate::masks::brush::Stroke {
                    erase: false,
                    size: 0.01,
                    feather: 0.0,
                    flow: 100.0,
                    points: vec![[0.2, 0.3]],
                }],
            }],
            ..Default::default()
        }
    }

    #[test]
    fn a_preset_holds_only_the_look() {
        let look = photo().look_only();
        assert_eq!(
            (look.exposure, look.contrast, look.clarity),
            (0.0, 40.0, -20.0)
        );
        assert!(look.geometry.is_none() && look.masks.is_empty() && look.spots.is_empty());
        assert!(look.removals.is_empty());
    }

    #[test]
    fn applying_replaces_the_look_and_keeps_the_photo() {
        let mono = builtin().into_iter().find(|p| p.id == "mono").unwrap();
        let r = photo().with_look_of(&mono.recipe);
        // The preset's look replaces the photo's, clarity included...
        assert_eq!((r.saturation, r.contrast, r.clarity), (-100.0, 30.0, 20.0));
        // ...and the photo keeps its exposure, geometry and masks.
        assert_eq!(r.exposure, 0.7);
        assert_eq!(r.geometry, photo().geometry);
        assert_eq!(r.masks, photo().masks);
        assert_eq!(r.spots, photo().spots);
        assert_eq!(r.removals, photo().removals);
        // Applying twice changes nothing more.
        assert_eq!(r.with_look_of(&mono.recipe), r);
    }

    #[test]
    fn preset_files_round_trip_the_look_only() {
        let text = to_file("Warm & soft", &photo());
        assert!(
            text.contains(r#""format": "photo-editor-preset""#),
            "{text}"
        );
        let (name, look) = from_file(&text).unwrap();
        assert_eq!(name, "Warm & soft");
        assert_eq!(look, photo().look_only());
        // Anything else is refused, and newer files say so.
        assert_eq!(from_file("{}"), Err(PresetFileError::NotAPreset));
        assert_eq!(from_file("not json"), Err(PresetFileError::NotAPreset));
        let newer = text.replace(r#""formatVersion": 1"#, r#""formatVersion": 2"#);
        assert_eq!(from_file(&newer), Err(PresetFileError::TooNew));
        let future_recipe = text.replace(
            &format!(r#""version": {}"#, crate::RECIPE_VERSION),
            &format!(r#""version": {}"#, crate::RECIPE_VERSION + 1),
        );
        assert_ne!(future_recipe, text);
        assert_eq!(from_file(&future_recipe), Err(PresetFileError::TooNew));
    }

    #[test]
    fn built_in_presets_are_distinct_valid_looks() {
        let all = builtin();
        let ids: std::collections::HashSet<_> = all.iter().map(|p| p.id).collect();
        assert_eq!(ids.len(), 6);
        for p in &all {
            assert_eq!(p.recipe.sanitized(), p.recipe, "{} is out of range", p.id);
            assert_eq!(
                p.recipe.look_only(),
                p.recipe,
                "{} holds more than a look",
                p.id
            );
            assert!(!p.recipe.is_identity(), "{} changes nothing", p.id);
        }
    }
}
