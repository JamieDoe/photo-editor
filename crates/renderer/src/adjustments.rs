//! Parameter ranges for recipe adjustments.
//!
//! The UI builds its controls from [`specs`] (sent over IPC) so ranges and defaults
//! are defined once, here.

use serde::Serialize;

/// Range and presentation hints for one adjustment.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AdjustmentSpec {
    /// Field name in `EditRecipe`.
    pub key: &'static str,
    pub label: &'static str,
    pub group: &'static str,
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub default: f32,
    /// Shown behind the section's "More controls" (as in the design).
    pub more: bool,
    /// Unit shown after the value ("EV"), or empty.
    pub unit: &'static str,
}

impl AdjustmentSpec {
    /// Clamps to range; non-finite values and -0.0 become the default.
    pub fn clamp(&self, v: f32) -> f32 {
        if !v.is_finite() {
            return self.default;
        }
        let c = v.clamp(self.min, self.max);
        if c == 0.0 { 0.0 } else { c }
    }
}

pub const EXPOSURE: AdjustmentSpec = AdjustmentSpec {
    key: "exposure",
    label: "Exposure",
    group: "Light",
    min: -5.0,
    max: 5.0,
    step: 0.01,
    default: 0.0,
    more: false,
    unit: "EV",
};
pub const CONTRAST: AdjustmentSpec = AdjustmentSpec {
    key: "contrast",
    label: "Contrast",
    group: "Light",
    min: -100.0,
    max: 100.0,
    step: 1.0,
    default: 0.0,
    more: false,
    unit: "",
};
pub const HIGHLIGHTS: AdjustmentSpec = AdjustmentSpec {
    key: "highlights",
    label: "Highlights",
    group: "Light",
    min: -100.0,
    max: 100.0,
    step: 1.0,
    default: 0.0,
    more: false,
    unit: "",
};
pub const SHADOWS: AdjustmentSpec = AdjustmentSpec {
    key: "shadows",
    label: "Shadows",
    group: "Light",
    min: -100.0,
    max: 100.0,
    step: 1.0,
    default: 0.0,
    more: false,
    unit: "",
};
pub const WHITES: AdjustmentSpec = AdjustmentSpec {
    key: "whites",
    label: "Whites",
    group: "Light",
    min: -100.0,
    max: 100.0,
    step: 1.0,
    default: 0.0,
    more: true,
    unit: "",
};
pub const BLACKS: AdjustmentSpec = AdjustmentSpec {
    key: "blacks",
    label: "Blacks",
    group: "Light",
    min: -100.0,
    max: 100.0,
    step: 1.0,
    default: 0.0,
    more: true,
    unit: "",
};
/// Behind the Light section's "More controls", after Whites and Blacks, as in the
/// design (ADR 0028).
pub const DEHAZE: AdjustmentSpec = AdjustmentSpec {
    key: "dehaze",
    label: "Dehaze",
    ..BLACKS
};
pub const TEMPERATURE: AdjustmentSpec = AdjustmentSpec {
    key: "temperature",
    label: "Temperature",
    group: "Colour",
    min: -100.0,
    max: 100.0,
    step: 1.0,
    default: 0.0,
    more: false,
    unit: "",
};
pub const TINT: AdjustmentSpec = AdjustmentSpec {
    key: "tint",
    label: "Tint",
    group: "Colour",
    min: -100.0,
    max: 100.0,
    step: 1.0,
    default: 0.0,
    more: false,
    unit: "",
};
pub const VIBRANCE: AdjustmentSpec = AdjustmentSpec {
    key: "vibrance",
    label: "Vibrance",
    group: "Colour",
    min: -100.0,
    max: 100.0,
    step: 1.0,
    default: 0.0,
    more: false,
    unit: "",
};
pub const SATURATION: AdjustmentSpec = AdjustmentSpec {
    key: "saturation",
    label: "Saturation",
    group: "Colour",
    min: -100.0,
    max: 100.0,
    step: 1.0,
    default: 0.0,
    more: false,
    unit: "",
};

pub const TEXTURE: AdjustmentSpec = AdjustmentSpec {
    key: "texture",
    label: "Texture",
    group: "Detail",
    min: -100.0,
    max: 100.0,
    step: 1.0,
    default: 0.0,
    more: false,
    unit: "",
};
pub const CLARITY: AdjustmentSpec = AdjustmentSpec {
    key: "clarity",
    label: "Clarity",
    ..TEXTURE
};
/// Capture sharpening; on by default, as in the design (ADR 0027).
pub const SHARPENING: AdjustmentSpec = AdjustmentSpec {
    key: "sharpening",
    label: "Sharpening",
    min: 0.0,
    max: 150.0,
    default: 40.0,
    ..TEXTURE
};

/// Noise reduction (ADR 0030), after Sharpening as in the design.
pub const NOISE_REDUCTION: AdjustmentSpec = AdjustmentSpec {
    key: "noiseReduction",
    label: "Noise reduction",
    min: 0.0,
    max: 100.0,
    default: 0.0,
    ..TEXTURE
};

/// Behind the Detail section's "More controls", under "Finishing", as in the design
/// (ADR 0031).
pub const VIGNETTE: AdjustmentSpec = AdjustmentSpec {
    key: "vignette",
    label: "Vignette",
    more: true,
    ..TEXTURE
};
pub const GRAIN: AdjustmentSpec = AdjustmentSpec {
    key: "grain",
    label: "Grain",
    min: 0.0,
    more: true,
    ..TEXTURE
};

/// The Geometry section's Straighten slider (ADR 0032): degrees, stored in the
/// recipe's `geometry`, not as a top-level field.
pub const STRAIGHTEN: AdjustmentSpec = AdjustmentSpec {
    key: "straighten",
    label: "Straighten",
    group: "Geometry",
    min: -crate::geometry::MAX_STRAIGHTEN,
    max: crate::geometry::MAX_STRAIGHTEN,
    step: 0.1,
    default: 0.0,
    more: false,
    unit: "°",
};

/// The Geometry section's perspective sliders (ADR 0034), behind "More controls":
/// stored in the recipe's `geometry` under the same keys.
pub const MASK_EXPOSURE: AdjustmentSpec = AdjustmentSpec {
    key: "exposure",
    label: "Exposure",
    group: "Mask",
    min: -2.0,
    max: 2.0,
    step: 0.01,
    default: 0.0,
    more: false,
    unit: "EV",
};
pub const MASK_WARMTH: AdjustmentSpec = AdjustmentSpec {
    key: "warmth",
    label: "Warmth",
    min: -100.0,
    max: 100.0,
    step: 1.0,
    unit: "",
    ..MASK_EXPOSURE
};
pub const MASK_CLARITY: AdjustmentSpec = AdjustmentSpec {
    key: "clarity",
    label: "Clarity",
    ..MASK_WARMTH
};

/// A radial mask's Feather (ADR 0041): the share of its radius the fade takes.
pub const MASK_FEATHER: AdjustmentSpec = AdjustmentSpec {
    key: "feather",
    label: "Feather",
    min: 0.0,
    default: 50.0,
    ..MASK_WARMTH
};

/// A mask's controls (ADR 0040), as the design has them. Keys are
/// `LocalAdjustments` fields.
pub fn mask_specs() -> Vec<AdjustmentSpec> {
    vec![MASK_EXPOSURE, MASK_WARMTH, MASK_CLARITY]
}

pub const PERSPECTIVE: [AdjustmentSpec; 2] = [
    AdjustmentSpec {
        key: "vertical",
        label: "Vertical",
        group: "Geometry",
        min: -100.0,
        max: 100.0,
        step: 1.0,
        default: 0.0,
        more: true,
        unit: "",
    },
    AdjustmentSpec {
        key: "horizontal",
        label: "Horizontal",
        group: "Geometry",
        min: -100.0,
        max: 100.0,
        step: 1.0,
        default: 0.0,
        more: true,
        unit: "",
    },
];

/// The colour mixer's controls, per band (ADR 0025). Keys are `HslShift` fields.
pub const MIXER_HUE: AdjustmentSpec = AdjustmentSpec {
    key: "hue",
    label: "Hue",
    group: "Colour mixer",
    min: -100.0,
    max: 100.0,
    step: 1.0,
    default: 0.0,
    more: true,
    unit: "",
};
pub const MIXER_SATURATION: AdjustmentSpec = AdjustmentSpec {
    key: "saturation",
    label: "Saturation",
    ..MIXER_HUE
};
pub const MIXER_LUMINANCE: AdjustmentSpec = AdjustmentSpec {
    key: "luminance",
    label: "Luminance",
    ..MIXER_HUE
};

/// One colour mixer band. Keys are `ColourMixer` fields, in band order.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct MixerBandSpec {
    pub key: &'static str,
    pub label: &'static str,
}

/// The colour mixer as the UI builds it: bands, and the controls each band has.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct MixerSpec {
    pub bands: Vec<MixerBandSpec>,
    pub controls: Vec<AdjustmentSpec>,
}

pub fn mixer_spec() -> MixerSpec {
    let band = |key, label| MixerBandSpec { key, label };
    MixerSpec {
        bands: vec![
            band("red", "Reds"),
            band("orange", "Oranges"),
            band("yellow", "Yellows"),
            band("green", "Greens"),
            band("aqua", "Aquas"),
            band("blue", "Blues"),
            band("purple", "Purples"),
            band("magenta", "Magentas"),
        ],
        controls: vec![MIXER_HUE, MIXER_SATURATION, MIXER_LUMINANCE],
    }
}

/// In display order: the design's Light section (Exposure, Contrast, Highlights,
/// Shadows; Whites, Blacks and Dehaze behind "More controls"), then Colour (Temperature, Tint,
/// Vibrance, Saturation), then Detail (Texture, Clarity, Sharpening, Noise reduction;
/// Vignette and Grain behind "More controls").
pub fn specs() -> Vec<AdjustmentSpec> {
    vec![
        EXPOSURE,
        CONTRAST,
        HIGHLIGHTS,
        SHADOWS,
        WHITES,
        BLACKS,
        DEHAZE,
        TEMPERATURE,
        TINT,
        VIBRANCE,
        SATURATION,
        TEXTURE,
        CLARITY,
        SHARPENING,
        NOISE_REDUCTION,
        VIGNETTE,
        GRAIN,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixer_spec_covers_every_band_and_control() {
        let json = serde_json::to_value(crate::ops::colour_mixer::ColourMixer::default()).unwrap();
        let bands: Vec<_> = json.as_object().unwrap().keys().cloned().collect();
        let spec = mixer_spec();
        let keys: Vec<_> = spec.bands.iter().map(|b| b.key.to_owned()).collect();
        assert_eq!(bands.len(), keys.len());
        assert!(bands.iter().all(|b| keys.contains(b)));
        let controls: Vec<_> = json["red"].as_object().unwrap().keys().cloned().collect();
        let keys: Vec<_> = spec.controls.iter().map(|c| c.key.to_owned()).collect();
        assert_eq!(controls.len(), keys.len());
        assert!(controls.iter().all(|c| keys.contains(c)));
    }

    #[test]
    fn specs_cover_every_recipe_field() {
        let json = serde_json::to_value(crate::EditRecipe::default()).unwrap();
        let fields: Vec<_> = json
            .as_object()
            .unwrap()
            .keys()
            // Not sliders: the schema version, and the look (a choice of profile).
            .filter(|k| *k != "version" && *k != "look")
            .cloned()
            .collect();
        let keys: Vec<_> = specs().iter().map(|s| s.key.to_owned()).collect();
        assert_eq!(fields.len(), keys.len());
        for f in fields {
            assert!(keys.contains(&f), "no spec for recipe field {f}");
        }
    }

    #[test]
    fn defaults_lie_within_range() {
        for s in specs() {
            assert!(s.min <= s.default && s.default <= s.max, "{}", s.key);
        }
    }
}
