use std::fmt;

use serde::{Deserialize, Serialize};

use crate::adjustments::{CONTRAST, EXPOSURE, SATURATION, TEMPERATURE};

/// Current edit recipe schema version.
pub const RECIPE_VERSION: u32 = 1;

/// A non-destructive edit: parameters only, never pixels.
///
/// Serialised form is the persisted edit and the input to cache keys, so it must stay
/// deterministic and free of UI state. Unknown/missing fields take defaults so older
/// recipes remain readable.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct EditRecipe {
    /// Recipe schema version (see [`RECIPE_VERSION`]).
    pub version: u32,
    /// Exposure in stops (EV).
    pub exposure: f32,
    /// Contrast, -100..100. Zero is neutral.
    pub contrast: f32,
    /// Warm/cool shift relative to the as-shot white balance, -100..100.
    pub temperature: f32,
    /// Colour saturation, -100 (monochrome) .. 100.
    pub saturation: f32,
}

impl Default for EditRecipe {
    fn default() -> Self {
        Self {
            version: RECIPE_VERSION,
            exposure: 0.0,
            contrast: 0.0,
            temperature: 0.0,
            saturation: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RecipeError {
    /// The recipe was written by a newer application version.
    UnsupportedVersion(u32),
    Invalid(String),
}

impl fmt::Display for RecipeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedVersion(v) => {
                write!(
                    f,
                    "edit recipe version {v} is newer than supported ({RECIPE_VERSION})"
                )
            }
            Self::Invalid(m) => write!(f, "invalid edit recipe: {m}"),
        }
    }
}

impl std::error::Error for RecipeError {}

impl EditRecipe {
    /// Parses a persisted recipe, applying migrations for older versions.
    pub fn from_json(json: &str) -> Result<Self, RecipeError> {
        let recipe: Self =
            serde_json::from_str(json).map_err(|e| RecipeError::Invalid(e.to_string()))?;
        match recipe.version {
            // Version 0 never shipped; treat a missing/zero version as the first schema.
            0 | 1 => Ok(Self {
                version: RECIPE_VERSION,
                ..recipe
            }
            .sanitized()),
            v => Err(RecipeError::UnsupportedVersion(v)),
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(&self.sanitized()).expect("recipe serialisation cannot fail")
    }

    /// Clamps every parameter to its valid range and replaces non-finite values with
    /// defaults, so rendering and hashing never see NaN, -0.0 or out-of-range input.
    pub fn sanitized(&self) -> Self {
        Self {
            version: self.version,
            exposure: EXPOSURE.clamp(self.exposure),
            contrast: CONTRAST.clamp(self.contrast),
            temperature: TEMPERATURE.clamp(self.temperature),
            saturation: SATURATION.clamp(self.saturation),
        }
    }

    /// Deterministic bytes identifying the rendered result of this recipe.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        self.to_json().into_bytes()
    }

    pub fn is_identity(&self) -> bool {
        let s = self.sanitized();
        s.exposure == 0.0 && s.contrast == 0.0 && s.temperature == 0.0 && s.saturation == 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_identity() {
        assert!(EditRecipe::default().is_identity());
    }

    #[test]
    fn json_round_trip() {
        let r = EditRecipe {
            exposure: 0.7,
            contrast: 12.0,
            temperature: -30.0,
            saturation: 14.0,
            ..Default::default()
        };
        assert_eq!(EditRecipe::from_json(&r.to_json()).unwrap(), r);
    }

    #[test]
    fn serialised_form_is_stable() {
        let r = EditRecipe {
            exposure: 0.5,
            ..Default::default()
        };
        assert_eq!(
            r.to_json(),
            r#"{"version":1,"exposure":0.5,"contrast":0.0,"temperature":0.0,"saturation":0.0}"#
        );
    }

    #[test]
    fn missing_fields_take_defaults() {
        let r = EditRecipe::from_json(r#"{"version":1,"exposure":1.0}"#).unwrap();
        assert_eq!(
            r,
            EditRecipe {
                exposure: 1.0,
                ..Default::default()
            }
        );
    }

    #[test]
    fn rejects_newer_versions_and_unknown_fields() {
        assert_eq!(
            EditRecipe::from_json(r#"{"version":99}"#),
            Err(RecipeError::UnsupportedVersion(99))
        );
        assert!(matches!(
            EditRecipe::from_json(r#"{"clarity":5}"#),
            Err(RecipeError::Invalid(_))
        ));
    }

    #[test]
    fn sanitize_clamps_and_removes_non_finite() {
        let r = EditRecipe {
            exposure: 99.0,
            contrast: f32::NAN,
            temperature: -0.0,
            saturation: -500.0,
            ..Default::default()
        }
        .sanitized();
        assert_eq!(r.exposure, 5.0);
        assert_eq!(r.contrast, 0.0);
        assert!(
            r.temperature.to_bits() == 0.0f32.to_bits(),
            "negative zero normalised"
        );
        assert_eq!(r.saturation, -100.0);
    }

    #[test]
    fn canonical_bytes_ignore_negative_zero() {
        let a = EditRecipe {
            temperature: -0.0,
            ..Default::default()
        };
        assert_eq!(a.canonical_bytes(), EditRecipe::default().canonical_bytes());
    }
}
