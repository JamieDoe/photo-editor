use crate::EditRecipe;
use crate::Look;
use crate::ops::colour_mixer::HslShift;
use crate::ops::detail::DetailParams;
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
    /// Highlights, shadows, whites and blacks (ADR 0023). Not a pure point operation:
    /// highlights and shadows read an edge-aware map of the surroundings' brightness,
    /// which backends build once per render from the stages before this one.
    Tone { params: ToneParams },
    /// Texture and clarity (ADR 0026): local contrast measured on the image entering
    /// the stage's neighbourhood; backends read neighbouring rows of the source.
    Detail { params: DetailParams },
    /// Tone S-curve around mid grey, applied per channel in a perceptual domain.
    Contrast { gamma: f32 },
    /// The Standard base look's tone curve, per channel (ADR 0022).
    BaseCurve,
    /// Hue, saturation and luminance per colour band (ADR 0025), in band order.
    ColourMixer { bands: [HslShift; 8] },
    /// Chroma boost weighted towards muted colours, sparing skin (-1..1).
    Vibrance { amount: f32 },
    /// Blend towards/away from Rec.709 luminance.
    Saturation { factor: f32 },
}

impl Stage {
    pub fn name(&self) -> &'static str {
        match self {
            Self::WhiteBalance { .. } => "white_balance",
            Self::Exposure { .. } => "exposure",
            Self::Tone { .. } => "tone",
            Self::Detail { .. } => "detail",
            Self::Contrast { .. } => "contrast",
            Self::BaseCurve => "base_curve",
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
    pub stages: Vec<Stage>,
    pub output: OutputTransform,
}

impl RenderPlan {
    pub fn new(stages: Vec<Stage>) -> Self {
        Self {
            stages,
            output: OutputTransform::Srgb8,
        }
    }

    /// Builds the plan for a recipe on a source whose as-shot light is `as_shot_white`
    /// (from the decoder; `None` for display-referred sources, whose white is D65).
    /// Identity stages are omitted.
    ///
    /// Order: white balance -> exposure -> tone (highlights, shadows, whites, blacks)
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
        let tone = r.tone();
        if !tone.is_identity() {
            stages.push(Stage::Tone { params: tone });
        }
        let detail = r.detail();
        if !detail.is_identity() {
            stages.push(Stage::Detail { params: detail });
        }
        if r.contrast != 0.0 {
            stages.push(Stage::Contrast {
                gamma: contrast::gamma_for(r.contrast),
            });
        }
        if r.look == Look::Standard {
            stages.push(Stage::BaseCurve);
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
        Self::new(stages)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_recipe_is_just_the_standard_look() {
        assert_eq!(
            RenderPlan::from_recipe(&EditRecipe::default(), None).stages,
            vec![Stage::BaseCurve]
        );
        let flat = EditRecipe {
            look: Look::Flat,
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
            clarity: 15.0,
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
                "tone",
                "detail",
                "contrast",
                "base_curve",
                "colour_mixer",
                "vibrance",
                "saturation"
            ]
        );
    }

    #[test]
    fn white_balance_is_relative_to_the_as_shot_light() {
        let tint_only = EditRecipe {
            tint: 30.0,
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
            look: Look::Flat,
            ..Default::default()
        };
        assert_eq!(
            RenderPlan::from_recipe(&r, None).stages,
            vec![Stage::Exposure { multiplier: 2.0 }]
        );
    }
}
