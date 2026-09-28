use std::fmt;

use image_core::{Cancellation, LinearImage, OutputImage, PixelFormat};

use crate::RenderPlan;

#[derive(Debug, Clone, PartialEq)]
pub enum RenderError {
    Cancelled,
    Backend(String),
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => write!(f, "render cancelled"),
            Self::Backend(m) => write!(f, "render backend error: {m}"),
        }
    }
}

impl std::error::Error for RenderError {}

/// Executes a [`RenderPlan`]. Implementations must produce the same result for the
/// same plan (within quantisation tolerance) so backends are interchangeable.
pub trait RenderBackend: Send + Sync {
    fn name(&self) -> &'static str;

    fn render(
        &self,
        plan: &RenderPlan,
        source: &LinearImage,
        format: PixelFormat,
        cancel: &dyn Cancellation,
    ) -> Result<OutputImage, RenderError>;
}
