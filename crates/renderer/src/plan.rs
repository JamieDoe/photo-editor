use crate::EditRecipe;
use crate::Look;
use crate::chromatic::ChromaticAberration;
use crate::geometry::Geometry;
use crate::masks::LocalMask;
use crate::ops::colour_mixer::HslShift;
use crate::ops::detail::DetailParams;
use crate::ops::point_curve::PointCurve;
use crate::ops::tone::ToneParams;
use crate::ops::{contrast, saturation, white_balance};
use image_core::Chromaticity;

/// One processing stage, in pipeline order. Parameters are resolved from the recipe
/// (e.g. temperature -> channel gains) so backends only execute arithmetic.
///
/// Every stage so far is a *point operation* (output pixel depends only on the same
/// input pixel), which lets backends fuse the whole plan into one pass. Future
/// neighbourhood stages (sharpening, local contrast) will split fused segments; see
/// docs/RENDERING.md.
#[derive(Debug, Clone, PartialEq)]
pub enum Stage {
    /// Per-channel gains relative to the as-shot white balance.
    WhiteBalance { gains: [f32; 3] },
    /// Scene-linear multiplier (2^EV).
    Exposure { multiplier: f32 },
    /// Masks' adjustments (ADR 0040): Exposure and Warmth as gains here, and read by
    /// the tone stage (its surroundings) and the detail stage (Clarity).
    Local { masks: Box<[LocalMask]> },
    /// Removes or adds haze (ADR 0028), -100..100. Estimated on the scene map from
    /// the source after the gains before it; stages after it measure the dehazed
    /// scene.
    Dehaze { amount: f32 },
    /// Highlights, shadows, whites and blacks (ADR 0023). Not a pure point operation:
    /// highlights and shadows read an edge-aware map of the surroundings' brightness,
    /// which backends build once per render from the stages before this one.
    Tone { params: ToneParams },
    /// Texture and clarity (ADR 0026): local contrast measured on the image entering
    /// the stage's neighbourhood; backends read neighbouring rows of the source.
    Detail { params: DetailParams },
    /// Vignette (ADR 0031), -100..100: a gain towards the corners.
    Vignette { amount: f32 },
    /// Tone S-curve around mid grey, applied per channel in a perceptual domain.
    Contrast { gamma: f32 },
    /// The Standard base look's tone curve, per channel (ADR 0022).
    BaseCurve,
    /// The photographer's tone curve (ADR 0037) and then each channel's (ADR 0038),
    /// on display tones, after the base look. Diagonals where unset.
    /// Boxed: four curves are much larger than any other stage.
    PointCurve {
        rgb: Box<PointCurve>,
        channels: Box<[PointCurve; 3]>,
    },
    /// Hue, saturation and luminance per colour band (ADR 0025), in band order.
    ColourMixer { bands: [HslShift; 8] },
    /// Chroma boost weighted towards muted colours, sparing skin (-1..1).
    Vibrance { amount: f32 },
    /// Blend towards/away from Rec.709 luminance.
    Saturation { factor: f32 },
    /// Film grain (ADR 0031), 0..100, on the finished image.
    Grain { amount: f32 },
}

impl Stage {
    pub fn name(&self) -> &'static str {
        match self {
            Self::WhiteBalance { .. } => "white_balance",
            Self::Exposure { .. } => "exposure",
            Self::Local { .. } => "local",
            Self::Dehaze { .. } => "dehaze",
            Self::Tone { .. } => "tone",
            Self::Detail { .. } => "detail",
            Self::Vignette { .. } => "vignette",
            Self::Grain { .. } => "grain",
            Self::Contrast { .. } => "contrast",
            Self::BaseCurve => "base_curve",
            Self::PointCurve { .. } => "point_curve",
            Self::ColourMixer { .. } => "colour_mixer",
            Self::Vibrance { .. } => "vibrance",
            Self::Saturation { .. } => "saturation",
        }
    }
}

/// Conversion from the scene-linear working space to the output encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputTransform {
    /// Clip to [0, 1], sRGB transfer function, 8-bit quantisation.
    Srgb8,
}

/// Backend-agnostic description of a render.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderPlan {
    /// Crop, straighten and perspective (ADRs 0032, 0034), applied first: the source is
    /// resampled into the output frame, and the stages run on that. `None` renders the
    /// whole source.
    pub geometry: Option<Geometry>,
    /// Red and blue scaled to line up with green (ADR 0035), in the same resample.
    pub chromatic_aberration: Option<ChromaticAberration>,
    pub stages: Vec<Stage>,
    pub output: OutputTransform,
}

impl RenderPlan {
    pub fn new(stages: Vec<Stage>) -> Self {
        Self {
            geometry: None,
            chromatic_aberration: None,
            stages,
            output: OutputTransform::Srgb8,
        }
    }

    /// The rendered size for a source of `width` x `height`.
    pub fn output_size(&self, width: u32, height: u32) -> (u32, u32) {
        match &self.geometry {
            Some(g) => g.output_size(width, height),
            None => (width, height),
        }
    }

    /// Builds the plan for a recipe on a source whose as-shot light is `as_shot_white`
    /// (from the decoder; `None` for display-referred sources, whose white is D65).
    /// Identity stages are omitted.
    ///
    /// Order: white balance -> exposure -> dehaze -> tone (highlights, shadows, whites,
    /// blacks)
    /// -> detail (texture, clarity) -> contrast -> base look (scene to display tones) -> colour (mixer, vibrance,
    /// saturation) -> output transform, matching the conceptual pipeline in CLAUDE.md.
    /// Exposure and contrast act on scene-referred values, so the base look's shoulder
    /// still rolls off highlights they push up.
    pub fn from_recipe(recipe: &EditRecipe, as_shot_white: Option<Chromaticity>) -> Self {
        let r = recipe.sanitized();
        let mut stages = Vec::new();
        if r.temperature != 0.0 || r.tint != 0.0 {
            stages.push(Stage::WhiteBalance {
                gains: white_balance::gains(as_shot_white, r.temperature, r.tint),
            });
        }
        if r.exposure != 0.0 {
            stages.push(Stage::Exposure {
                multiplier: r.exposure.exp2(),
            });
        }
        let local: Vec<LocalMask> = r
            .masks
            .iter()
            .filter(|m| !m.adjustments.is_identity())
            .map(|m| {
                let a = m.adjustments;
                let warmth = if a.warmth == 0.0 {
                    [0.0; 3]
                } else {
                    white_balance::gains(as_shot_white, a.warmth, 0.0).map(f32::log2)
                };
                LocalMask {
                    shape: m.shape,
                    invert: m.invert,
                    stops: a.exposure,
                    warmth,
                    clarity: a.clarity,
                }
            })
            .collect();
        let local_clarity = local.iter().any(|m| m.clarity != 0.0);
        if !local.is_empty() {
            stages.push(Stage::Local {
                masks: local.into_boxed_slice(),
            });
        }
        if r.dehaze != 0.0 {
            stages.push(Stage::Dehaze { amount: r.dehaze });
        }
        let tone = r.tone();
        if !tone.is_identity() {
            stages.push(Stage::Tone { params: tone });
        }
        let detail = r.detail();
        // Masks' Clarity runs in the detail stage, even without global detail.
        if !detail.is_identity() || local_clarity {
            stages.push(Stage::Detail { params: detail });
        }
        if r.vignette != 0.0 {
            stages.push(Stage::Vignette { amount: r.vignette });
        }
        if r.contrast != 0.0 {
            stages.push(Stage::Contrast {
                gamma: contrast::gamma_for(r.contrast),
            });
        }
        if r.look == Look::Standard {
            stages.push(Stage::BaseCurve);
        }
        let rgb = r.point_curve.filter(|c| !c.is_identity());
        let channels = r.channel_curves.filter(|c| !c.is_identity());
        if rgb.is_some() || channels.is_some() {
            stages.push(Stage::PointCurve {
                rgb: Box::new(rgb.unwrap_or_default()),
                channels: Box::new(channels.unwrap_or_default().curves()),
            });
        }
        if let Some(mixer) = r.mixer {
            stages.push(Stage::ColourMixer {
                bands: mixer.bands(),
            });
        }
        if r.vibrance != 0.0 {
            stages.push(Stage::Vibrance {
                amount: r.vibrance / 100.0,
            });
        }
        if r.saturation != 0.0 {
            stages.push(Stage::Saturation {
                factor: saturation::factor_for(r.saturation),
            });
        }
        if r.grain != 0.0 {
            stages.push(Stage::Grain { amount: r.grain });
        }
        Self {
            geometry: r.geometry.filter(|g| !g.is_identity()),
            chromatic_aberration: r.chromatic_aberration.filter(|c| !c.is_identity()),
            ..Self::new(stages)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_recipe_is_capture_sharpening_and_the_standard_look() {
        let sharpening = Stage::Detail {
            params: DetailParams {
                texture: 0.0,
                clarity: 0.0,
                sharpening: 40.0,
                noise: 0.0,
            },
        };
        assert_eq!(
            RenderPlan::from_recipe(&EditRecipe::default(), None).stages,
            vec![sharpening.clone(), Stage::BaseCurve]
        );
        let flat = EditRecipe {
            look: Look::Flat,
            sharpening: 0.0,
            ..Default::default()
        };
        assert!(RenderPlan::from_recipe(&flat, None).stages.is_empty());
    }

    #[test]
    fn stages_are_in_pipeline_order() {
        let r = EditRecipe {
            exposure: 1.0,
            contrast: 10.0,
            shadows: 20.0,
            dehaze: 30.0,
            clarity: 15.0,
            vignette: -20.0,
            grain: 10.0,
            temperature: 10.0,
            vibrance: 10.0,
            saturation: 10.0,
            mixer: Some(crate::ops::colour_mixer::ColourMixer {
                red: HslShift {
                    hue: 10.0,
                    ..Default::default()
                },
                ..Default::default()
            }),
            ..Default::default()
        };
        let names: Vec<_> = RenderPlan::from_recipe(&r, None)
            .stages
            .iter()
            .map(Stage::name)
            .collect();
        assert_eq!(
            names,
            [
                "white_balance",
                "exposure",
                "dehaze",
                "tone",
                "detail",
                "vignette",
                "contrast",
                "base_curve",
                "colour_mixer",
                "vibrance",
                "saturation",
                "grain"
            ]
        );
    }

    #[test]
    fn white_balance_is_relative_to_the_as_shot_light() {
        let tint_only = EditRecipe {
            tint: 30.0,
            sharpening: 0.0,
            look: Look::Flat,
            ..Default::default()
        };
        let stages = RenderPlan::from_recipe(&tint_only, None).stages;
        assert!(matches!(stages[..], [Stage::WhiteBalance { .. }]));

        // The same warm-up is a different set of gains under tungsten than daylight.
        let warm = EditRecipe {
            temperature: 40.0,
            look: Look::Flat,
            ..Default::default()
        };
        let tungsten = Chromaticity {
            x: 0.447_58,
            y: 0.407_45,
        };
        assert_ne!(
            RenderPlan::from_recipe(&warm, None).stages,
            RenderPlan::from_recipe(&warm, Some(tungsten)).stages
        );
    }

    #[test]
    fn exposure_resolves_to_multiplier() {
        let r = EditRecipe {
            exposure: 1.0,
            sharpening: 0.0,
            look: Look::Flat,
            ..Default::default()
        };
        assert_eq!(
            RenderPlan::from_recipe(&r, None).stages,
            vec![Stage::Exposure { multiplier: 2.0 }]
        );
    }
}
