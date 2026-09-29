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
/// Shadows; Whites and Blacks behind "More controls"), then Colour (Temperature, Tint,
/// Vibrance, Saturation).
pub fn specs() -> Vec<AdjustmentSpec> {
    vec![
        EXPOSURE,
        CONTRAST,
        HIGHLIGHTS,
        SHADOWS,
        WHITES,
        BLACKS,
        TEMPERATURE,
        TINT,
        VIBRANCE,
        SATURATION,
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
