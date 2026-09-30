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

impl EditRecipe {
    /// This recipe's look alone, as a preset holds it: without the exposure,
    /// geometry, lens corrections and masks, which belong to the photo.
    pub fn look_only(&self) -> EditRecipe {
        EditRecipe {
            exposure: 0.0,
            geometry: None,
            chromatic_aberration: None,
            masks: Vec::new(),
            ..self.clone()
        }
    }

    /// This photo's recipe with `preset`'s look: the preset's settings, keeping this
    /// recipe's exposure, geometry, lens corrections and masks.
    pub fn with_look_of(&self, preset: &EditRecipe) -> EditRecipe {
        EditRecipe {
            version: self.version,
            exposure: self.exposure,
            geometry: self.geometry,
            chromatic_aberration: self.chromatic_aberration,
            masks: self.masks.clone(),
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
        assert!(look.geometry.is_none() && look.masks.is_empty());
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
        // Applying twice changes nothing more.
        assert_eq!(r.with_look_of(&mono.recipe), r);
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
