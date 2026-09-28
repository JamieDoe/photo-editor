use crate::EditRecipe;
use crate::ops::{contrast, saturation, white_balance};

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
    /// Tone S-curve around mid grey, applied per channel in a perceptual domain.
    Contrast { gamma: f32 },
    /// Blend towards/away from Rec.709 luminance.
    Saturation { factor: f32 },
}

impl Stage {
    pub fn name(&self) -> &'static str {
        match self {
            Self::WhiteBalance { .. } => "white_balance",
            Self::Exposure { .. } => "exposure",
            Self::Contrast { .. } => "contrast",
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

    /// Builds the plan for a recipe. Identity stages are omitted.
    ///
    /// Order: white balance -> exposure -> tone (contrast) -> colour (saturation) ->
    /// output transform, matching the conceptual pipeline in CLAUDE.md.
    pub fn from_recipe(recipe: &EditRecipe) -> Self {
        let r = recipe.sanitized();
        let mut stages = Vec::new();
        if r.temperature != 0.0 {
            stages.push(Stage::WhiteBalance {
                gains: white_balance::temperature_gains(r.temperature),
            });
        }
        if r.exposure != 0.0 {
            stages.push(Stage::Exposure {
                multiplier: r.exposure.exp2(),
            });
        }
        if r.contrast != 0.0 {
            stages.push(Stage::Contrast {
                gamma: contrast::gamma_for(r.contrast),
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
    fn identity_recipe_has_no_stages() {
        assert!(
            RenderPlan::from_recipe(&EditRecipe::default())
                .stages
                .is_empty()
        );
    }

    #[test]
    fn stages_are_in_pipeline_order() {
        let r = EditRecipe {
            exposure: 1.0,
            contrast: 10.0,
            temperature: 10.0,
            saturation: 10.0,
            ..Default::default()
        };
        let names: Vec<_> = RenderPlan::from_recipe(&r)
            .stages
            .iter()
            .map(Stage::name)
            .collect();
        assert_eq!(
            names,
            ["white_balance", "exposure", "contrast", "saturation"]
        );
    }

    #[test]
    fn exposure_resolves_to_multiplier() {
        let r = EditRecipe {
            exposure: 1.0,
            ..Default::default()
        };
        assert_eq!(
            RenderPlan::from_recipe(&r).stages,
            vec![Stage::Exposure { multiplier: 2.0 }]
        );
    }
}
