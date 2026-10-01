use std::fmt;

use serde::{Deserialize, Serialize};

use crate::Look;
use crate::adjustments::{
    BLACKS, CLARITY, CONTRAST, DEHAZE, EXPOSURE, GRAIN, HIGHLIGHTS, NOISE_REDUCTION, SATURATION,
    SHADOWS, SHARPENING, TEMPERATURE, TEXTURE, TINT, VIBRANCE, VIGNETTE, WHITES,
};
use crate::ops::colour_mixer::ColourMixer;

/// Current edit recipe schema version.
///
/// - 1: exposure, contrast, temperature, saturation on a flat base (no tone curve).
/// - 2: adds `look` (ADR 0022). Version 1 recipes migrate to `Look::Flat`, so edits
///   made before keep their exact look; new recipes default to `Look::Standard`.
/// - 3: adds highlights, shadows, whites and blacks (ADR 0023). Older recipes read them
///   as 0, which renders exactly as before.
/// - 4: adds tint and vibrance (ADR 0024); older recipes read them as 0.
/// - 5: adds the colour mixer (ADR 0025), written only when used; older recipes have
///   none.
/// - 6: adds texture and clarity (ADR 0026); older recipes read them as 0.
/// - 7: adds sharpening (ADR 0027). New recipes default to 40, as in the design;
///   older recipes had none and keep none, so they render as they did.
/// - 8: adds dehaze (ADR 0028); older recipes read it as 0.
/// - 9: adds noise reduction (ADR 0030); older recipes read it as 0.
/// - 10: adds vignette and grain (ADR 0031); older recipes read them as 0.
/// - 11: adds crop and straighten (ADR 0032), written only when used; older recipes
///   have none.
/// - 12: adds vertical and horizontal perspective to the geometry (ADR 0034); older
///   geometry reads them as 0.
/// - 13: adds chromatic aberration removal (ADR 0035), written only when on; older
///   recipes have none.
/// - 14: adds the tone curve's points (ADR 0037), written only when shaped; older
///   recipes have none.
/// - 15: adds red, green and blue tone curves (ADR 0038), written only when shaped;
///   older recipes have none.
/// - 16: adds quarter turns and a flip to the geometry (ADR 0039); older geometry is
///   upright.
/// - 17: adds masks (ADR 0040), written only when there are some; older recipes have
///   none.
/// - 18: adds radial masks, and inverted and hidden masks (ADR 0041).
/// - 19: adds brush masks (ADR 0042).
/// - 20: adds masks of several shapes, and mask density (ADR 0043); older masks are
///   one shape at full density.
/// - 21: adds white balance set as a light and the parametric tone curve (ADR 0051),
///   written only when set.
/// - 22: adds colour grading (ADR 0052), written only when set.
/// - 23: adds calibration (ADR 0053), written only when set.
pub const RECIPE_VERSION: u32 = 23;

/// A non-destructive edit: parameters only, never pixels.
///
/// Serialised form is the persisted edit and the input to cache keys, so it must stay
/// deterministic and free of UI state. Unknown/missing fields take defaults so older
/// recipes remain readable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct EditRecipe {
    /// Recipe schema version (see [`RECIPE_VERSION`]).
    pub version: u32,
    /// Exposure in stops (EV).
    pub exposure: f32,
    /// Contrast, -100..100. Zero is neutral.
    pub contrast: f32,
    /// Highlights, -100..100: darkens (recovers) or brightens bright areas.
    pub highlights: f32,
    /// Shadows, -100..100: lifts or deepens dark areas, keeping their texture.
    pub shadows: f32,
    /// Whites, -100..100: moves the white end of the tonal range.
    pub whites: f32,
    /// Blacks, -100..100: moves the black end of the tonal range.
    pub blacks: f32,
    /// Dehaze, -100 (adds haze) .. 100 (removes it).
    pub dehaze: f32,
    /// Warm/cool shift relative to the as-shot white balance, -100..100 (±120 mired).
    pub temperature: f32,
    /// Green/magenta shift relative to the as-shot white balance, -100..100.
    pub tint: f32,
    /// White balance set as the light itself (ADR 0051), as Lightroom presets made on
    /// raw files set it: each photo is balanced from its own as-shot light to this
    /// one, and Temperature and Tint are ignored. `None` (and omitted from the JSON)
    /// when white balance is relative.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub white_balance: Option<crate::ops::white_balance::AbsoluteWhiteBalance>,
    /// Saturation that favours muted colours and spares skin tones, -100..100.
    pub vibrance: f32,
    /// Colour saturation, -100 (monochrome) .. 100.
    pub saturation: f32,
    /// Fine detail, -100 (smoother) .. 100 (crisper).
    pub texture: f32,
    /// Medium-scale local contrast, -100 (softer) .. 100 (punchier).
    pub clarity: f32,
    /// Capture sharpening, 0..150 (default 40).
    pub sharpening: f32,
    /// Noise reduction, 0..100.
    pub noise_reduction: f32,
    /// Vignette, -100 (darker corners) .. 100 (lighter corners).
    pub vignette: f32,
    /// Film grain, 0..100.
    pub grain: f32,
    /// Hue, saturation and luminance per colour band. `None` (and omitted from the
    /// JSON) when unused, so recipes without it read and hash as before.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub mixer: Option<ColourMixer>,
    /// Crop and straighten. `None` (and omitted from the JSON) when unused.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub geometry: Option<crate::geometry::Geometry>,
    /// Remove chromatic aberration: the measured correction while the toggle is on.
    /// `None` (and omitted from the JSON) when off.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub chromatic_aberration: Option<crate::chromatic::ChromaticAberration>,
    /// The tone curve's points, `[input, output]` display tones. `None` (and omitted
    /// from the JSON) while it is the diagonal.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional, type = "Array<[number, number]>"))]
    pub point_curve: Option<crate::ops::point_curve::PointCurve>,
    /// The parametric tone curve (ADR 0051): Lightroom's region sliders, before the
    /// point curve. `None` (and omitted from the JSON) while its sliders are at zero.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub parametric_curve: Option<crate::ops::parametric_curve::ParametricCurve>,
    /// Colour grading (ADR 0052): tints for the shadows, midtones, highlights and the
    /// whole picture. `None` (and omitted from the JSON) while no wheel is set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub colour_grading: Option<crate::ops::colour_grading::ColourGrading>,
    /// Calibration (ADR 0053): the primaries' hue and saturation, and the shadows'
    /// tint. `None` (and omitted from the JSON) while all are 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub calibration: Option<crate::ops::calibration::Calibration>,
    /// Red, green and blue tone curves, after the RGB one. `None` (and omitted from
    /// the JSON) while all three are the diagonal.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub channel_curves: Option<crate::ops::point_curve::ChannelCurves>,
    /// Masks: adjustments to part of the photo (ADR 0040), in the order made. Empty
    /// (and omitted from the JSON) without any.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(as = "Option<Vec<crate::masks::Mask>>", optional))]
    pub masks: Vec<crate::masks::Mask>,
    /// The base look the adjustments start from.
    pub look: Look,
}

impl Default for EditRecipe {
    fn default() -> Self {
        Self {
            version: RECIPE_VERSION,
            exposure: 0.0,
            contrast: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            whites: 0.0,
            blacks: 0.0,
            dehaze: 0.0,
            temperature: 0.0,
            tint: 0.0,
            white_balance: None,
            vibrance: 0.0,
            saturation: 0.0,
            texture: 0.0,
            clarity: 0.0,
            sharpening: SHARPENING.default,
            noise_reduction: 0.0,
            vignette: 0.0,
            grain: 0.0,
            mixer: None,
            geometry: None,
            chromatic_aberration: None,
            point_curve: None,
            parametric_curve: None,
            colour_grading: None,
            calibration: None,
            channel_curves: None,
            masks: Vec::new(),
            look: Look::Standard,
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
            // Version 0 never shipped; treat a zero version as the first schema. Version
            // 1 had no tone curve: keep that look so old edits render as they did.
            0 | 1 => Ok(Self {
                version: RECIPE_VERSION,
                look: Look::Flat,
                sharpening: 0.0,
                ..recipe
            }
            .sanitized()),
            // Later fields are missing from older versions. They read as 0, which is
            // exact, except sharpening: its default is 40, so it is set to the 0 these
            // recipes rendered with.
            2..=6 => Ok(Self {
                version: RECIPE_VERSION,
                sharpening: 0.0,
                ..recipe
            }
            .sanitized()),
            7..=23 => Ok(Self {
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
            highlights: HIGHLIGHTS.clamp(self.highlights),
            shadows: SHADOWS.clamp(self.shadows),
            whites: WHITES.clamp(self.whites),
            blacks: BLACKS.clamp(self.blacks),
            dehaze: DEHAZE.clamp(self.dehaze),
            temperature: TEMPERATURE.clamp(self.temperature),
            tint: TINT.clamp(self.tint),
            white_balance: self.white_balance.map(|w| w.sanitized()),
            vibrance: VIBRANCE.clamp(self.vibrance),
            saturation: SATURATION.clamp(self.saturation),
            texture: TEXTURE.clamp(self.texture),
            clarity: CLARITY.clamp(self.clarity),
            sharpening: SHARPENING.clamp(self.sharpening),
            noise_reduction: NOISE_REDUCTION.clamp(self.noise_reduction),
            vignette: VIGNETTE.clamp(self.vignette),
            grain: GRAIN.clamp(self.grain),
            mixer: self.mixer.map(sanitize_mixer).filter(|m| !m.is_identity()),
            geometry: self
                .geometry
                .map(|g| g.sanitized())
                .filter(|g| !g.is_identity()),
            // Kept while on, even if nothing was measured: the toggle stays on.
            chromatic_aberration: self.chromatic_aberration.map(|c| c.sanitized()),
            point_curve: self.point_curve.filter(|c| !c.is_identity()),
            parametric_curve: self
                .parametric_curve
                .map(|c| c.sanitized())
                .filter(|c| !c.is_identity()),
            colour_grading: self
                .colour_grading
                .map(|g| g.sanitized())
                .filter(|g| !g.is_identity()),
            calibration: self
                .calibration
                .map(|c| c.sanitized())
                .filter(|c| !c.is_identity()),
            channel_curves: self
                .channel_curves
                .map(|c| c.sanitized())
                .filter(|c| !c.is_identity()),
            masks: self
                .masks
                .iter()
                .map(crate::masks::Mask::sanitized)
                .collect(),
            look: self.look,
        }
    }

    /// The tone controls (highlights, shadows, whites, blacks).
    pub fn tone(&self) -> crate::ops::tone::ToneParams {
        crate::ops::tone::ToneParams {
            highlights: self.highlights,
            shadows: self.shadows,
            whites: self.whites,
            blacks: self.blacks,
        }
    }

    /// The detail controls (texture, clarity, sharpening).
    pub fn detail(&self) -> crate::ops::detail::DetailParams {
        crate::ops::detail::DetailParams {
            texture: self.texture,
            clarity: self.clarity,
            sharpening: self.sharpening,
            noise: self.noise_reduction,
        }
    }

    /// Deterministic bytes identifying the rendered result of this recipe.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        self.to_json().into_bytes()
    }

    /// Whether this is the default: no adjustments on the default look. A photo with an
    /// identity recipe has no saved edit.
    pub fn is_identity(&self) -> bool {
        let s = self.sanitized();
        s.exposure == 0.0
            && s.contrast == 0.0
            && s.tone().is_identity()
            && s.dehaze == 0.0
            && s.temperature == 0.0
            && s.tint == 0.0
            && s.white_balance.is_none()
            && s.vibrance == 0.0
            && s.saturation == 0.0
            && s.texture == 0.0
            && s.clarity == 0.0
            && s.sharpening == SHARPENING.default
            && s.noise_reduction == 0.0
            && s.vignette == 0.0
            && s.grain == 0.0
            && s.mixer.is_none()
            && s.geometry.is_none()
            && s.chromatic_aberration.is_none()
            && s.point_curve.is_none()
            && s.parametric_curve.is_none()
            && s.colour_grading.is_none()
            && s.calibration.is_none()
            && s.channel_curves.is_none()
            && s.masks.is_empty()
            && s.look == Look::default()
    }
}

fn sanitize_mixer(mut m: ColourMixer) -> ColourMixer {
    use crate::adjustments::{MIXER_HUE, MIXER_LUMINANCE, MIXER_SATURATION};
    for b in m.bands_mut() {
        b.hue = MIXER_HUE.clamp(b.hue);
        b.saturation = MIXER_SATURATION.clamp(b.saturation);
        b.luminance = MIXER_LUMINANCE.clamp(b.luminance);
    }
    m
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
            r#"{"version":23,"exposure":0.5,"contrast":0.0,"highlights":0.0,"shadows":0.0,"whites":0.0,"blacks":0.0,"dehaze":0.0,"temperature":0.0,"tint":0.0,"vibrance":0.0,"saturation":0.0,"texture":0.0,"clarity":0.0,"sharpening":40.0,"noiseReduction":0.0,"vignette":0.0,"grain":0.0,"look":"standard"}"#
        );
    }

    #[test]
    fn missing_fields_take_defaults() {
        let r = EditRecipe::from_json(r#"{"version":2,"exposure":1.0}"#).unwrap();
        assert_eq!(
            r,
            EditRecipe {
                exposure: 1.0,
                // Version 2 had no sharpening, and keeps none.
                sharpening: 0.0,
                ..Default::default()
            }
        );
    }

    #[test]
    fn version_1_recipes_keep_their_flat_look() {
        let r = EditRecipe::from_json(
            r#"{"version":1,"exposure":0.5,"contrast":10.0,"temperature":0.0,"saturation":0.0}"#,
        )
        .unwrap();
        assert_eq!(r.version, RECIPE_VERSION);
        assert_eq!(r.look, Look::Flat);
        assert_eq!((r.exposure, r.contrast), (0.5, 10.0));
        // Even an all-zero version 1 recipe is an edit now: it keeps the flat look.
        let zero = EditRecipe::from_json(r#"{"version":1}"#).unwrap();
        assert!(!zero.is_identity());
    }

    #[test]
    fn version_2_recipes_read_the_tone_controls_as_zero() {
        let r = EditRecipe::from_json(
            r#"{"version":2,"exposure":0.5,"contrast":0.0,"temperature":0.0,"saturation":0.0,"look":"flat"}"#,
        )
        .unwrap();
        assert_eq!(r.version, RECIPE_VERSION);
        assert_eq!(
            (r.highlights, r.shadows, r.whites, r.blacks),
            (0.0, 0.0, 0.0, 0.0)
        );
        assert_eq!((r.exposure, r.look), (0.5, Look::Flat));
    }

    #[test]
    fn version_3_recipes_read_tint_and_vibrance_as_zero() {
        let r =
            EditRecipe::from_json(r#"{"version":3,"temperature":20.0,"shadows":10.0}"#).unwrap();
        assert_eq!(r.version, RECIPE_VERSION);
        assert_eq!((r.tint, r.vibrance), (0.0, 0.0));
        assert_eq!((r.temperature, r.shadows), (20.0, 10.0));
    }

    #[test]
    fn sharpening_defaults_to_40_but_older_recipes_keep_none() {
        assert_eq!(EditRecipe::default().sharpening, 40.0);
        let new = EditRecipe::from_json(r#"{"version":7,"exposure":0.5}"#).unwrap();
        assert_eq!(new.sharpening, 40.0);
        for old in [
            r#"{"version":6,"texture":10.0}"#,
            r#"{"version":2}"#,
            r#"{"version":1}"#,
        ] {
            let r = EditRecipe::from_json(old).unwrap();
            assert_eq!(r.sharpening, 0.0, "{old}");
            // Without sharpening it differs from the default, so it stays an edit.
            assert!(!r.is_identity());
        }
    }

    #[test]
    fn geometry_is_written_only_when_used() {
        use crate::geometry::{AspectRatio, CropRect, Geometry};
        let unused = EditRecipe {
            geometry: Some(Geometry {
                aspect: AspectRatio::Square,
                ..Default::default()
            }),
            ..Default::default()
        };
        // No rotation and the whole frame: no edit, and the JSON is unchanged.
        assert!(unused.is_identity());
        assert!(!unused.to_json().contains("geometry"));
        let used = EditRecipe {
            geometry: Some(Geometry {
                straighten: 2.5,
                crop: CropRect {
                    x: 0.1,
                    y: 0.1,
                    w: 0.8,
                    h: 0.8,
                },
                aspect: AspectRatio::Free,
                vertical: 30.0,
                horizontal: -12.5,
                rotation: 3,
                flip: true,
            }),
            ..Default::default()
        };
        let back = EditRecipe::from_json(&used.to_json()).unwrap();
        assert_eq!(back.geometry, used.geometry);
        assert!(!back.is_identity());
        // Version 10 recipes have none.
        assert_eq!(
            EditRecipe::from_json(r#"{"version":10}"#).unwrap().geometry,
            None
        );
        // Version 16 recipes have no masks; masks round-trip in order.
        assert!(
            EditRecipe::from_json(r#"{"version":16}"#)
                .unwrap()
                .masks
                .is_empty()
        );
        let masked = EditRecipe {
            masks: vec![crate::masks::Mask {
                id: 7,
                hidden: false,
                parts: Vec::new(),
                density: 100.0,
                invert: true,
                shape: crate::masks::MaskShape::Linear {
                    start: [0.5, 0.1],
                    end: [0.5, 0.5],
                },
                adjustments: crate::masks::LocalAdjustments {
                    exposure: -0.7,
                    warmth: 12.0,
                    clarity: 0.0,
                },
            }],
            ..Default::default()
        };
        let back = EditRecipe::from_json(&masked.to_json()).unwrap();
        assert_eq!(back.masks, masked.masks);
        assert!(!back.is_identity());
        assert!(!EditRecipe::default().to_json().contains("masks"));
        // Brush strokes round-trip with their points.
        let brushed = EditRecipe {
            masks: vec![crate::masks::Mask {
                id: 2,
                hidden: false,
                parts: Vec::new(),
                density: 100.0,
                invert: false,
                shape: crate::masks::MaskShape::Brush {
                    strokes: vec![crate::masks::Stroke {
                        erase: true,
                        size: 0.05,
                        feather: 40.0,
                        flow: 70.0,
                        points: vec![[0.25, 0.5], [0.3, 0.55]],
                    }],
                },
                adjustments: Default::default(),
            }],
            ..Default::default()
        };
        assert!(brushed.to_json().contains(
            r#""shape":{"kind":"brush","strokes":[{"erase":true,"size":0.05,"feather":40.0,"flow":70.0,"points":[[0.25,0.5],[0.3,0.55]]}]}"#
        ));
        assert_eq!(
            EditRecipe::from_json(&brushed.to_json()).unwrap().masks,
            brushed.masks
        );
        // Version 14 recipes have no channel curves; a red curve round-trips, all
        // diagonals are dropped.
        assert_eq!(
            EditRecipe::from_json(r#"{"version":14}"#)
                .unwrap()
                .channel_curves,
            None
        );
        let red = crate::ops::point_curve::ChannelCurves {
            red: Some(crate::ops::point_curve::PointCurve::new(&[
                [0.0, 0.0],
                [0.5, 0.6],
                [1.0, 1.0],
            ])),
            ..Default::default()
        };
        let tinted = EditRecipe {
            channel_curves: Some(red),
            ..Default::default()
        };
        assert!(
            tinted
                .to_json()
                .contains(r#""channelCurves":{"red":[[0.0,0.0],[0.5,0.6],[1.0,1.0]]}"#)
        );
        assert_eq!(
            EditRecipe::from_json(&tinted.to_json())
                .unwrap()
                .channel_curves,
            Some(red)
        );
        assert!(!tinted.is_identity());
        let flat = EditRecipe {
            channel_curves: Some(Default::default()),
            ..Default::default()
        };
        assert!(flat.is_identity() && !flat.to_json().contains("channelCurves"));
        // Version 13 recipes have no tone curve; a shaped one round-trips, the
        // diagonal is dropped.
        assert_eq!(
            EditRecipe::from_json(r#"{"version":13}"#)
                .unwrap()
                .point_curve,
            None
        );
        let curve =
            crate::ops::point_curve::PointCurve::new(&[[0.0, 0.05], [0.5, 0.6], [1.0, 1.0]]);
        let shaped = EditRecipe {
            point_curve: Some(curve),
            ..Default::default()
        };
        assert!(
            shaped
                .to_json()
                .contains(r#""pointCurve":[[0.0,0.05],[0.5,0.6],[1.0,1.0]]"#)
        );
        let back = EditRecipe::from_json(&shaped.to_json()).unwrap();
        assert_eq!(back.point_curve, Some(curve));
        assert!(!back.is_identity());
        let diagonal = EditRecipe {
            point_curve: Some(Default::default()),
            ..Default::default()
        };
        assert!(diagonal.is_identity() && !diagonal.to_json().contains("pointCurve"));
        // Version 12 recipes have no chromatic aberration removal.
        assert_eq!(
            EditRecipe::from_json(r#"{"version":12}"#)
                .unwrap()
                .chromatic_aberration,
            None
        );
        let on = EditRecipe {
            chromatic_aberration: Some(crate::ChromaticAberration {
                red: [0.000512, -0.00002],
                blue: [-0.0003, 0.0],
            }),
            ..Default::default()
        };
        let back = EditRecipe::from_json(&on.to_json()).unwrap();
        assert_eq!(back.chromatic_aberration, on.chromatic_aberration);
        assert!(!back.is_identity());
        // On with nothing measured still counts as on.
        let zero = EditRecipe {
            chromatic_aberration: Some(crate::ChromaticAberration::default()),
            ..Default::default()
        };
        assert!(
            EditRecipe::from_json(&zero.to_json())
                .unwrap()
                .chromatic_aberration
                .is_some()
        );
        // Version 11 geometry has no perspective.
        let v11 = EditRecipe::from_json(
            r#"{"version":11,"geometry":{"straighten":1.0,"crop":{"x":0.0,"y":0.0,"w":1.0,"h":1.0},"aspect":"original"}}"#,
        )
        .unwrap()
        .geometry
        .unwrap();
        assert_eq!((v11.vertical, v11.horizontal), (0.0, 0.0));
    }

    #[test]
    fn the_mixer_is_written_only_when_used() {
        let mut r = EditRecipe {
            mixer: Some(ColourMixer::default()),
            ..Default::default()
        };
        // An unused mixer is no edit and leaves the JSON (and cache keys) unchanged.
        assert!(r.is_identity());
        assert_eq!(r.canonical_bytes(), EditRecipe::default().canonical_bytes());
        assert!(!r.to_json().contains("mixer"));

        r.mixer.as_mut().unwrap().blue.luminance = -30.0;
        r.mixer.as_mut().unwrap().red.hue = f32::NAN;
        r.mixer.as_mut().unwrap().green.saturation = 900.0;
        let json = r.to_json();
        assert!(json.contains(r#""blue":{"hue":0.0,"saturation":0.0,"luminance":-30.0}"#));
        let back = EditRecipe::from_json(&json).unwrap();
        let m = back.mixer.unwrap();
        assert_eq!((m.red.hue, m.green.saturation), (0.0, 100.0));
        assert_eq!(back, r.sanitized());

        // Version 4 recipes have no mixer.
        let old = EditRecipe::from_json(r#"{"version":4,"vibrance":10.0}"#).unwrap();
        assert_eq!(old.mixer, None);
        assert!(matches!(
            EditRecipe::from_json(r#"{"version":5,"mixer":{"teal":{}}}"#),
            Err(RecipeError::Invalid(_))
        ));
    }

    #[test]
    fn the_default_look_is_part_of_identity() {
        let flat = EditRecipe {
            look: Look::Flat,
            ..Default::default()
        };
        assert!(!flat.is_identity());
        assert_ne!(
            flat.canonical_bytes(),
            EditRecipe::default().canonical_bytes()
        );
    }

    #[test]
    fn rejects_newer_versions_and_unknown_fields() {
        assert_eq!(
            EditRecipe::from_json(r#"{"version":99}"#),
            Err(RecipeError::UnsupportedVersion(99))
        );
        assert!(matches!(
            EditRecipe::from_json(r#"{"lensProfile":5}"#),
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
