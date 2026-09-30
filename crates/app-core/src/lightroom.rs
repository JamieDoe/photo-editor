//! Lightroom presets (ADR 0047): reading a Lightroom Classic / Lightroom `.xmp` preset
//! into a look, for photographers bringing their presets across.
//!
//! A preset is XMP with Camera Raw settings (`crs:` names), written as attributes of
//! its `rdf:Description` or as elements (the name, tone curves). Settings with a
//! counterpart here map by name and range: the Basic panel, presence, HSL, the point
//! curves, detail and effects. The rest are left out and named, so the photographer
//! knows. The two apps process differently, so a preset comes close to its Lightroom
//! look rather than matching it.

use std::collections::HashMap;

use quick_xml::events::Event;
use quick_xml::{Reader, XmlVersion};
use renderer::EditRecipe;
use renderer::ops::colour_mixer::{ColourMixer, HslShift};
use renderer::ops::point_curve::{ChannelCurves, PointCurve};

/// A Lightroom preset read as a look.
#[derive(Debug, Clone, PartialEq)]
pub struct LightroomPreset {
    /// The preset's own name, if it has one.
    pub name: Option<String>,
    pub recipe: EditRecipe,
    /// What the preset sets that this app has no counterpart for, by Lightroom's
    /// panel names ("Color Grading", "Masks").
    pub left_out: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LightroomError {
    /// Not XMP with Camera Raw settings (or not well-formed).
    NotAPreset,
    /// A Lightroom preset, but nothing in it has a counterpart here.
    NothingUsable,
}

/// The Camera Raw settings of an XMP document: single values, and lists (tone curve
/// points, the name's languages).
#[derive(Debug, Default)]
struct Settings {
    values: HashMap<String, String>,
    lists: HashMap<String, Vec<String>>,
}

impl Settings {
    fn num(&self, key: &str) -> Option<f32> {
        self.values
            .get(key)?
            .trim()
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite())
    }

    fn set(&self, key: &str) -> bool {
        self.num(key).is_some_and(|v| v != 0.0)
    }

    fn is(&self, key: &str, value: &str) -> bool {
        self.values
            .get(key)
            .is_some_and(|v| v.trim().eq_ignore_ascii_case(value))
    }
}

/// Reads the Camera Raw settings. Settings nested in another setting (such as those
/// of Lightroom's masks) are not the preset's own and are skipped; the outer setting
/// is recorded as present.
fn read_settings(xml: &str) -> Result<Settings, LightroomError> {
    let mut reader = Reader::from_str(xml);
    let mut s = Settings::default();
    // Open elements, with the setting each is (`crs:Contrast2012` is `Contrast2012`).
    let mut stack: Vec<Option<String>> = Vec::new();
    // Text of the innermost element (pieces arrive around entity references).
    let mut text = String::new();
    let mut found = false;
    let settings_open = |stack: &[Option<String>]| stack.iter().flatten().count();
    loop {
        let event = reader
            .read_event()
            .map_err(|_| LightroomError::NotAPreset)?;
        match event {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let top_level = settings_open(&stack) == 0;
                if top_level {
                    for attr in e.attributes() {
                        let attr = attr.map_err(|_| LightroomError::NotAPreset)?;
                        if let Some(key) = attr.key.as_ref().strip_prefix("crs:") {
                            let value = attr
                                .normalized_value(XmlVersion::Implicit1_0)
                                .map_err(|_| LightroomError::NotAPreset)?;
                            s.values.insert(key.to_owned(), value.into_owned());
                            found = true;
                        }
                    }
                }
                let setting = e.name().as_ref().strip_prefix("crs:").map(str::to_owned);
                if let Some(setting) = &setting
                    && top_level
                {
                    // Present, even when it only holds structure (masks, a profile).
                    s.lists.entry(setting.clone()).or_default();
                    found = true;
                }
                if matches!(event, Event::Start(_)) {
                    stack.push(setting);
                }
                text.clear();
            }
            Event::Text(ref t) => text.push_str(&t.xml10_content()),
            Event::CData(ref t) => text.push_str(&t.xml10_content()),
            Event::GeneralRef(ref r) => match r.resolve_char_ref() {
                Ok(Some(c)) => text.push(c),
                _ => text.push_str(match r.xml10_content().as_ref() {
                    "amp" => "&",
                    "lt" => "<",
                    "gt" => ">",
                    "quot" => "\"",
                    "apos" => "'",
                    _ => "",
                }),
            },
            Event::End(ref e) => {
                let setting = stack.pop().ok_or(LightroomError::NotAPreset)?;
                let value = std::mem::take(&mut text).trim().to_owned();
                match setting {
                    // A top-level setting written as an element with a value.
                    Some(name) if settings_open(&stack) == 0 && !value.is_empty() => {
                        s.values.entry(name).or_insert(value);
                    }
                    // An item of a top-level setting's list (curve points, the name).
                    None if e.name().as_ref() == "rdf:li" && settings_open(&stack) == 1 => {
                        if let Some(name) = stack.iter().flatten().next() {
                            s.lists.entry(name.clone()).or_default().push(value);
                        }
                    }
                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if found {
        Ok(s)
    } else {
        Err(LightroomError::NotAPreset)
    }
}

/// A Lightroom point curve ("x, y" pairs, 0..255) as a curve here; `None` for the
/// straight line (or anything unreadable).
fn curve(points: Option<&Vec<String>>) -> Option<PointCurve> {
    let points: Vec<[f32; 2]> = points?
        .iter()
        .filter_map(|p| {
            let (x, y) = p.split_once(',')?;
            Some([
                x.trim().parse::<f32>().ok()? / 255.0,
                y.trim().parse::<f32>().ok()? / 255.0,
            ])
        })
        .collect();
    if points.len() < 2 {
        return None;
    }
    let c = PointCurve::new(&points);
    (c != PointCurve::default()).then_some(c)
}

/// Reads a Lightroom `.xmp` preset.
pub fn read_xmp(xml: &str) -> Result<LightroomPreset, LightroomError> {
    let s = read_settings(xml.trim_start_matches('\u{feff}'))?;
    let mut r = EditRecipe::default();
    let mut used = false;
    let mut take = |key: &str, field: &mut f32| {
        if let Some(v) = s.num(key) {
            *field = v;
            used = true;
        }
    };
    take("Exposure2012", &mut r.exposure);
    take("Contrast2012", &mut r.contrast);
    take("Highlights2012", &mut r.highlights);
    take("Shadows2012", &mut r.shadows);
    take("Whites2012", &mut r.whites);
    take("Blacks2012", &mut r.blacks);
    take("Texture", &mut r.texture);
    take("Clarity2012", &mut r.clarity);
    take("Dehaze", &mut r.dehaze);
    take("Vibrance", &mut r.vibrance);
    take("Saturation", &mut r.saturation);
    // White balance as a shift from the photo's own; Lightroom writes this for
    // presets made on JPEGs.
    take("IncrementalTemperature", &mut r.temperature);
    take("IncrementalTint", &mut r.tint);
    take("Sharpness", &mut r.sharpening);
    take("LuminanceSmoothing", &mut r.noise_reduction);
    take("PostCropVignetteAmount", &mut r.vignette);
    take("GrainAmount", &mut r.grain);

    // Presets made on raw files set the light itself, in kelvin (ADR 0051): each photo
    // is balanced to it from its own as-shot light.
    if let Some(kelvin) = s.num("Temperature")
        && !s.is("WhiteBalance", "As Shot")
        && !s.values.contains_key("IncrementalTemperature")
    {
        r.white_balance = Some(renderer::ops::white_balance::AbsoluteWhiteBalance {
            kelvin,
            tint: s.num("Tint").unwrap_or(0.0),
        });
        used = true;
    }

    // HSL: Lightroom's eight bands are this app's.
    let mut mixer = ColourMixer::default();
    let bands: [(&str, &mut HslShift); 8] = [
        ("Red", &mut mixer.red),
        ("Orange", &mut mixer.orange),
        ("Yellow", &mut mixer.yellow),
        ("Green", &mut mixer.green),
        ("Aqua", &mut mixer.aqua),
        ("Blue", &mut mixer.blue),
        ("Purple", &mut mixer.purple),
        ("Magenta", &mut mixer.magenta),
    ];
    for (band, shift) in bands {
        for (kind, field) in [
            ("Hue", &mut shift.hue),
            ("Saturation", &mut shift.saturation),
            ("Luminance", &mut shift.luminance),
        ] {
            if let Some(v) = s.num(&format!("{kind}Adjustment{band}")) {
                *field = v;
                used = true;
            }
        }
    }
    // Black and white: Lightroom hides HSL and mixes grey from each colour's
    // brightness (B&W mix). The mixer's luminance before Saturation -100 does the same
    // here (ADR 0051), so the B&W mix becomes it, and HSL is dropped as Lightroom
    // ignores it.
    let grayscale = s.is("ConvertToGrayscale", "True");
    if grayscale {
        mixer = ColourMixer::default();
        for (band, shift) in [
            ("Red", &mut mixer.red),
            ("Orange", &mut mixer.orange),
            ("Yellow", &mut mixer.yellow),
            ("Green", &mut mixer.green),
            ("Aqua", &mut mixer.aqua),
            ("Blue", &mut mixer.blue),
            ("Purple", &mut mixer.purple),
            ("Magenta", &mut mixer.magenta),
        ] {
            if let Some(v) = s.num(&format!("GrayMixer{band}")) {
                shift.luminance = v;
            }
        }
    }
    if mixer != ColourMixer::default() {
        r.mixer = Some(mixer);
    }

    if s.is("ConvertToGrayscale", "True") {
        r.saturation = -100.0;
        used = true;
    }

    r.point_curve = curve(s.lists.get("ToneCurvePV2012"));
    // The region sliders and their splits (ADR 0051).
    let parametric = renderer::ops::parametric_curve::ParametricCurve {
        shadows: s.num("ParametricShadows").unwrap_or(0.0),
        darks: s.num("ParametricDarks").unwrap_or(0.0),
        lights: s.num("ParametricLights").unwrap_or(0.0),
        highlights: s.num("ParametricHighlights").unwrap_or(0.0),
        shadow_split: s.num("ParametricShadowSplit").unwrap_or(25.0),
        midtone_split: s.num("ParametricMidtoneSplit").unwrap_or(50.0),
        highlight_split: s.num("ParametricHighlightSplit").unwrap_or(75.0),
    };
    if !parametric.is_identity() {
        r.parametric_curve = Some(parametric);
        used = true;
    }
    let channels = ChannelCurves {
        red: curve(s.lists.get("ToneCurvePV2012Red")),
        green: curve(s.lists.get("ToneCurvePV2012Green")),
        blue: curve(s.lists.get("ToneCurvePV2012Blue")),
    };
    if channels != ChannelCurves::default() {
        r.channel_curves = Some(channels);
    }
    used |= r.point_curve.is_some()
        || r.channel_curves.is_some()
        || s.lists.contains_key("ToneCurvePV2012");

    let mut left_out = Vec::new();
    let mut note = |present: bool, what: &'static str| {
        if present {
            left_out.push(what);
        }
    };
    note(
        [
            "SplitToningShadowSaturation",
            "SplitToningHighlightSaturation",
            "ColorGradeShadowSat",
            "ColorGradeMidtoneSat",
            "ColorGradeHighlightSat",
            "ColorGradeGlobalSat",
        ]
        .iter()
        .any(|k| s.set(k)),
        "Color Grading",
    );
    note(
        [
            "RedHue",
            "RedSaturation",
            "GreenHue",
            "GreenSaturation",
            "BlueHue",
            "BlueSaturation",
            "ShadowTint",
        ]
        .iter()
        .any(|k| s.set(k)),
        "Calibration",
    );
    note(
        [
            "MaskGroupBasedCorrections",
            "GradientBasedCorrections",
            "CircularGradientBasedCorrections",
            "PaintBasedCorrections",
            "RetouchAreas",
        ]
        .iter()
        .any(|k| s.lists.contains_key(*k)),
        "Masks and healing",
    );
    note(
        s.is("LensProfileEnable", "1")
            || s.is("AutoLateralCA", "1")
            || s.set("LensManualDistortionAmount")
            || s.set("DefringePurpleAmount")
            || s.set("DefringeGreenAmount"),
        "Lens Corrections",
    );
    note(
        s.is("HasCrop", "True")
            || [
                "PerspectiveVertical",
                "PerspectiveHorizontal",
                "PerspectiveRotate",
                "PerspectiveScale",
            ]
            .iter()
            .any(|k| s.set(k)),
        "Crop and Transform",
    );
    note(
        s.values.contains_key("CameraProfile") || s.lists.contains_key("Look"),
        "Profile",
    );
    note(
        s.set("ColorNoiseReduction") && s.num("ColorNoiseReduction") != Some(25.0),
        "Color noise reduction",
    );

    if !used {
        return Err(LightroomError::NothingUsable);
    }
    let name = s
        .lists
        .get("Name")
        .and_then(|l| l.first())
        .or_else(|| s.values.get("Name"))
        .map(|n| n.trim().to_owned())
        .filter(|n| !n.is_empty());
    Ok(LightroomPreset {
        name,
        recipe: r.sanitized().look_only(),
        left_out,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> String {
        std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/presets/lightroom-sample.xmp"
        ))
        .unwrap()
    }

    #[test]
    fn reads_a_lightroom_classic_preset() {
        let p = read_xmp(&sample()).unwrap();
        assert_eq!(p.name.as_deref(), Some("Soft & Warm"));
        let r = &p.recipe;
        // A preset's look: its exposure belongs to each photo (ADR 0046).
        assert_eq!(r.exposure, 0.0);
        assert_eq!(
            (r.contrast, r.highlights, r.shadows, r.whites, r.blacks),
            (18.0, -42.0, 30.0, 8.0, -12.0)
        );
        assert_eq!((r.texture, r.clarity, r.dehaze), (10.0, 14.0, 6.0));
        assert_eq!((r.vibrance, r.saturation), (22.0, -5.0));
        assert_eq!((r.temperature, r.tint), (12.0, -4.0));
        assert_eq!((r.sharpening, r.noise_reduction), (55.0, 20.0));
        assert_eq!((r.vignette, r.grain), (-18.0, 15.0));
        let mixer = r.mixer.unwrap();
        assert_eq!(
            (
                mixer.orange.hue,
                mixer.orange.saturation,
                mixer.orange.luminance
            ),
            (-6.0, -12.0, 8.0)
        );
        assert_eq!((mixer.blue.hue, mixer.blue.saturation), (-10.0, 15.0));
        assert_eq!(mixer.aqua.luminance, -20.0);
        // The tone curve, in 0..1; the straight red curve is no curve.
        let curve = r.point_curve.unwrap();
        assert_eq!(
            curve.points()[0],
            [0.0, (18.0f32 / 255.0 * 1e4).round() / 1e4]
        );
        let channels = r.channel_curves.unwrap();
        assert!(channels.red.is_none() && channels.green.is_none() && channels.blue.is_some());
        // What has no counterpart is named; the mask's own exposure was not taken.
        assert_eq!(p.left_out, ["Color Grading", "Masks and healing"]);
        // The region sliders come across (ADR 0051).
        assert_eq!(r.parametric_curve.map(|c| c.darks), Some(-6.0));
    }

    #[test]
    fn black_and_white_presets_desaturate() {
        let xmp = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
            <rdf:Description xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
              crs:ConvertToGrayscale="True" crs:Contrast2012="+25" crs:GrayMixerRed="+20"
              crs:Temperature="5500" crs:WhiteBalance="Custom" crs:CameraProfile="Adobe Monochrome"/>
            </rdf:RDF></x:xmpmeta>"#;
        let p = read_xmp(xmp).unwrap();
        assert_eq!((p.recipe.saturation, p.recipe.contrast), (-100.0, 25.0));
        assert_eq!(p.name, None);
        assert_eq!(p.left_out, ["Profile"]);
        // The B&W mix is the mixer's luminance (ADR 0051).
        assert_eq!(p.recipe.mixer.unwrap().red.luminance, 20.0);
        // The light in kelvin comes across as set (ADR 0051).
        let wb = p.recipe.white_balance.unwrap();
        assert_eq!((wb.kelvin, wb.tint), (5500.0, 0.0));
    }

    #[test]
    fn settings_written_as_elements_are_read_too() {
        let xmp = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
            <rdf:Description xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/">
              <crs:Contrast2012>-20</crs:Contrast2012>
              <crs:Name><rdf:Alt><rdf:li xml:lang="x-default">Faded &#8212; cool</rdf:li></rdf:Alt></crs:Name>
            </rdf:Description></rdf:RDF></x:xmpmeta>"#;
        let p = read_xmp(xmp).unwrap();
        assert_eq!(p.recipe.contrast, -20.0);
        assert_eq!(p.name.as_deref(), Some("Faded \u{2014} cool"));
    }

    #[test]
    fn other_files_are_refused() {
        assert_eq!(
            read_xmp("not xml at all <"),
            Err(LightroomError::NotAPreset)
        );
        assert_eq!(
            read_xmp(r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF/></x:xmpmeta>"#),
            Err(LightroomError::NotAPreset)
        );
        let only_profile = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
            <rdf:Description xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/" crs:CameraProfile="Adobe Standard"/>
            </rdf:RDF></x:xmpmeta>"#;
        assert_eq!(read_xmp(only_profile), Err(LightroomError::NothingUsable));
    }
}
