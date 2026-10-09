//! Setting groups (ADR 0048): the recipe's fields by the panel section they belong to,
//! for copying an edit onto other photos a section at a time. Fields are named as in
//! the recipe's JSON.

use serde::Serialize;

/// A group of settings that is copied, or not, as a whole.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SettingGroup {
    /// Stable, for remembering choices: `"light"`, `"masks"`, ...
    pub id: &'static str,
    pub label: &'static str,
    /// The recipe fields it holds (JSON names).
    pub fields: Vec<&'static str>,
    /// Copied unless the photographer leaves it out. The crop and masks are drawn for
    /// one photo, so they are left out unless chosen (as in Lightroom).
    pub copied_by_default: bool,
}

/// Every recipe field but `version`, in exactly one group, in the panel's order.
pub fn setting_groups() -> Vec<SettingGroup> {
    let group = |id, label, fields: &[&'static str], copied_by_default| SettingGroup {
        id,
        label,
        fields: fields.to_vec(),
        copied_by_default,
    };
    vec![
        group("exposure", "Exposure", &["exposure"], true),
        group(
            "light",
            "Light and tone curve",
            &[
                "look",
                "contrast",
                "highlights",
                "shadows",
                "whites",
                // Where Whites acts (ADR 0073) goes with it.
                "whitesFromSensor",
                "blacks",
                "dehaze",
                "pointCurve",
                "parametricCurve",
                "channelCurves",
            ],
            true,
        ),
        group(
            "whiteBalance",
            "White balance",
            &["temperature", "tint", "whiteBalance"],
            true,
        ),
        group(
            "colour",
            "Colour and grading",
            &["vibrance", "saturation", "mixer", "colourGrading"],
            true,
        ),
        group("calibration", "Calibration", &["calibration"], true),
        group(
            "detail",
            "Detail and effects",
            &[
                "texture",
                "clarity",
                "sharpening",
                "noiseReduction",
                "vignette",
                "grain",
            ],
            true,
        ),
        group(
            "geometry",
            "Crop, geometry and lens",
            &[
                "geometry",
                "chromaticAberration",
                "profileCorrections",
                "unoriented",
            ],
            false,
        ),
        group("masks", "Masks", &["masks"], false),
        group("retouch", "Retouch", &["spots", "removals"], false),
    ]
}

/// `target` with the settings of the groups named in `groups` taken from `source`
/// (ADR 0048): a field `source` does not have (no crop, no masks) is removed from
/// `target` too. Unknown group ids are ignored. The editor's `pasteEdits` does the same
/// for the open photo.
pub fn paste_groups(
    target: &crate::EditRecipe,
    source: &crate::EditRecipe,
    groups: &[String],
) -> crate::EditRecipe {
    let as_map =
        |r: &crate::EditRecipe| match serde_json::from_str::<serde_json::Value>(&r.to_json()) {
            Ok(serde_json::Value::Object(m)) => m,
            _ => serde_json::Map::new(),
        };
    let (mut next, from) = (as_map(target), as_map(source));
    for group in setting_groups()
        .iter()
        .filter(|g| groups.iter().any(|id| id == g.id))
    {
        for field in &group.fields {
            match from.get(*field) {
                Some(v) => next.insert((*field).to_owned(), v.clone()),
                None => next.remove(*field),
            };
        }
    }
    crate::EditRecipe::from_json(&serde_json::Value::Object(next).to_string())
        .map(|r| r.sanitized())
        .unwrap_or_else(|_| target.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EditRecipe;

    #[test]
    fn pasting_takes_only_the_chosen_groups() {
        let source = EditRecipe {
            exposure: 0.8,
            contrast: 25.0,
            temperature: 12.0,
            masks: vec![crate::masks::Mask::new(
                1,
                crate::masks::MaskShape::Brush {
                    strokes: Vec::new(),
                },
                Default::default(),
            )],
            ..Default::default()
        };
        let target = EditRecipe {
            exposure: -0.3,
            tint: 7.0,
            geometry: Some(crate::Geometry {
                straighten: 2.0,
                ..Default::default()
            }),
            ..Default::default()
        };
        let ids = |ids: &[&str]| ids.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let r = paste_groups(
            &target,
            &source,
            &ids(&["exposure", "light", "whiteBalance", "nonsense"]),
        );
        assert_eq!(
            (r.exposure, r.contrast, r.temperature, r.tint),
            (0.8, 25.0, 12.0, 0.0)
        );
        // Not chosen: the target keeps its crop and gets no masks.
        assert_eq!(r.geometry, target.geometry);
        assert!(r.masks.is_empty());
        // Chosen but absent from the source: removed from the target.
        let r = paste_groups(&target, &source, &ids(&["geometry", "masks"]));
        assert!(r.geometry.is_none());
        assert_eq!(r.masks, source.masks);
        assert_eq!(r.exposure, -0.3);
    }

    #[test]
    fn every_setting_is_in_exactly_one_group() {
        // A recipe with every optional part present, so every field is written.
        let r = EditRecipe {
            white_balance: Some(crate::ops::white_balance::AbsoluteWhiteBalance {
                kelvin: 5500.0,
                tint: 10.0,
            }),
            mixer: Some(crate::ColourMixer {
                red: crate::HslShift {
                    hue: 5.0,
                    ..Default::default()
                },
                ..Default::default()
            }),
            geometry: Some(crate::Geometry {
                straighten: 1.0,
                ..Default::default()
            }),
            chromatic_aberration: Some(Default::default()),
            profile_corrections: false,
            unoriented: true,
            colour_grading: Some(crate::ops::colour_grading::ColourGrading {
                global: crate::ops::colour_grading::GradeWheel {
                    hue: 40.0,
                    saturation: 10.0,
                    luminance: 0.0,
                },
                ..Default::default()
            }),
            calibration: Some(crate::ops::calibration::Calibration {
                red_hue: 10.0,
                ..Default::default()
            }),
            parametric_curve: Some(crate::ops::parametric_curve::ParametricCurve {
                darks: 20.0,
                ..Default::default()
            }),
            point_curve: Some(crate::ops::point_curve::PointCurve::new(&[
                [0.0, 0.1],
                [1.0, 1.0],
            ])),
            channel_curves: Some(crate::ops::point_curve::ChannelCurves {
                red: Some(crate::ops::point_curve::PointCurve::new(&[
                    [0.0, 0.1],
                    [1.0, 1.0],
                ])),
                ..Default::default()
            }),
            masks: vec![crate::masks::Mask::new(
                1,
                crate::masks::MaskShape::Brush {
                    strokes: Vec::new(),
                },
                Default::default(),
            )],
            spots: vec![crate::retouch::Spot::default()],
            whites_from_sensor: true,
            removals: vec![crate::remove::Removal {
                strokes: vec![crate::masks::brush::Stroke {
                    erase: false,
                    size: 0.02,
                    feather: 0.0,
                    flow: 100.0,
                    points: vec![[0.5, 0.5]],
                }],
            }],
            ..Default::default()
        };
        let json: serde_json::Value = serde_json::from_str(&r.to_json()).unwrap();
        let mut fields: Vec<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        fields.retain(|f| *f != "version");
        fields.sort();
        let mut grouped: Vec<&str> = setting_groups()
            .iter()
            .flat_map(|g| g.fields.clone())
            .collect();
        grouped.sort();
        assert_eq!(
            grouped, fields,
            "a setting is missing from the groups, or in two"
        );
    }
}
