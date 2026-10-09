//! Application engine.
//!
//! [`Engine`] is the single entry point the desktop shell (or a benchmark) talks to.
//! Every operation returns a [`jobs::JobHandle`] immediately; the work runs on the
//! job system, never on the caller's thread.

mod albums;
mod config;
mod edits;
mod engine;
mod error;
mod lens;
mod library;
mod lightroom;
mod mask_store;
mod presets;
mod previews;
mod session;
pub mod sidecars;
mod thumbnails;
mod types;

pub use ai::{Coverage, MaskKind};
pub use albums::{MAX_ALBUM_NAME, album_not_found, clean_album_name};
pub use catalogue::{
    Album, AlbumId, BackupInfo, BackupKind, BackupStore, Catalogue, CatalogueError, Collection,
    CollectionCounts, CollectionEntry, ColourLabel, FileStatus, Flag, MarkChange, Marks,
    PhotoDetails, PhotoId, PresetId, Rating, SourceIdentity, StoredEdit, newest_valid,
};
pub use catalogue::{SCHEMA_VERSION, schema_version_of};
pub use config::EngineConfig;
pub use edits::{SavedEdit, load_edit, paste_onto, save_edit};
pub use engine::{Engine, generated_kind, judgements, mask_kind};
pub use error::{EngineError, ErrorKind};
pub use library::{IndexProgress, IndexStage, IndexSummary};
pub use presets::{
    ImportedPreset, MAX_PRESET_NAME, PRESET_FILE_EXTENSION, Preset, PresetRef, create_preset,
    delete_preset, export_preset_file, import_preset_file, list_presets, preset_file_name,
    rename_preset, update_preset,
};
pub use thumbnails::{
    BatchSummary, Pregenerated, THUMBNAIL_LONG_EDGE, Thumbnail, ThumbnailBatch, ThumbnailSource,
};
pub use types::{
    EmbeddedFrame, EngineInfo, ExportEstimate, ExportProgress, ExportRequest, ExportStage,
    ExportSummary, FileExport, GeneratedMask, ImageId, ImageSummary, PreviewFrame, PreviewRequest,
    PreviewSlot,
};

// Re-exported so shells depend on one crate for the engine API.
pub use cache::DiskCacheStats;
pub use export::ExportFormat;
pub use export::colour::ExportColourSpace;
pub use export::metadata::{Judgements, MetadataChoice};
pub use export::sharpen::OutputSharpening;
pub use export::watermark::{Position as WatermarkPosition, Size as WatermarkSize, Watermark};
pub use jobs::{CancelToken, JobError, JobHandle};
pub use raw::SourceKind;
pub use renderer::{EditRecipe, Look, PreviewQuality, RECIPE_VERSION};
