//! Rendering: edit recipe -> render plan -> backend.
//!
//! - [`EditRecipe`] is the persisted, versioned description of an edit (UI-agnostic).
//! - [`RenderPlan`] is the backend-agnostic list of processing stages derived from a
//!   recipe. It is plain data so a CPU backend, a GPU backend and tests can all
//!   consume the same plan.
//! - [`RenderBackend`] executes a plan against a [`image_core::LinearImage`].
//!   [`CpuRenderer`] is the reference (and currently only production) backend.

pub mod adjustments;
mod backend;
pub mod chromatic;
pub mod cpu;
pub mod geometry;
pub mod histogram;
pub mod masks;
pub mod ops;
mod plan;
pub mod presets;
mod quality;
mod recipe;
pub mod settings;

pub use backend::{RenderBackend, RenderError};
pub use chromatic::ChromaticAberration;
pub use cpu::CpuRenderer;
pub use geometry::{AspectRatio, CropRect, Geometry};
pub use histogram::Histogram;
pub use ops::colour_mixer::{ColourMixer, HslShift};
pub use ops::look::Look;
pub use ops::white_balance::TemperatureScale;
pub use plan::{OutputTransform, RenderPlan, Stage};
pub use quality::{PreviewQuality, QualityLimits};
pub use recipe::{EditRecipe, RECIPE_VERSION, RecipeError};

/// Version of the rendering algorithms. Bump whenever the same recipe would produce
/// different pixels, so caches are invalidated and old edits can be migrated.
///
/// - 2: the Standard base look (ADR 0022).
/// - 3: Temperature is relative to each photo's as-shot light instead of a fixed
///   6500 K reference (ADR 0024), so existing temperature edits shift slightly.
pub const RENDERER_VERSION: u32 = 3;
