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
