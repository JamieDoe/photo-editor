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
pub mod cpu;
pub mod ops;
mod plan;
mod quality;
mod recipe;

pub use backend::{RenderBackend, RenderError};
pub use cpu::CpuRenderer;
pub use ops::look::Look;
pub use plan::{OutputTransform, RenderPlan, Stage};
pub use quality::{PreviewQuality, QualityLimits};
pub use recipe::{EditRecipe, RECIPE_VERSION, RecipeError};

/// Version of the rendering algorithms. Bump whenever the same recipe would produce
/// different pixels, so caches are invalidated and old edits can be migrated.
pub const RENDERER_VERSION: u32 = 2;
