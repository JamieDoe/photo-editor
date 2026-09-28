//! Application engine.
//!
//! [`Engine`] is the single entry point the desktop shell (or a benchmark) talks to.
//! Every operation returns a [`jobs::JobHandle`] immediately; the work runs on the
//! job system, never on the caller's thread.

mod config;
mod engine;
mod error;
mod identity;
mod previews;
mod session;
mod types;

pub use config::EngineConfig;
pub use engine::Engine;
pub use error::{EngineError, ErrorKind};
pub use identity::SourceIdentity;
pub use types::{
    EmbeddedFrame, EngineInfo, ExportProgress, ExportRequest, ExportStage, ExportSummary, ImageId,
    ImageSummary, PreviewFrame, PreviewRequest,
};

// Re-exported so shells depend on one crate for the engine API.
pub use export::ExportFormat;
pub use jobs::{CancelToken, JobError, JobHandle};
pub use raw::SourceKind;
pub use renderer::{EditRecipe, PreviewQuality};
