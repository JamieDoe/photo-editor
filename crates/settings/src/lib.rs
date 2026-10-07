//! Application settings (PRODUCT.md §44).
//!
//! Only settings that change behaviour today exist; nothing speculative. Settings are
//! user preferences, not catalogue data (that belongs in SQLite from Phase 2) and not
//! edit recipes.
//!
//! Loading never fails: a missing file gives defaults, a corrupt file is backed up and
//! replaced with defaults, and every value is clamped to its valid range.

mod model;
mod store;

pub use model::{
    BackgroundIntensity, BackupSettings, ExportColourSpace, ExportFileFormat, ExportSettings,
    GeneralSettings, LibrarySettings, OutputSharpening, PerformanceSettings, SETTINGS_VERSION,
    Settings, Theme,
};
pub use store::{LoadOutcome, SettingsStore};
