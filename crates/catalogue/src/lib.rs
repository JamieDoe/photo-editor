//! The local catalogue (PRODUCT.md §18, §19).
//!
//! SQLite holds application state about photographs (never the photographs). Everything
//! here can be rebuilt by re-indexing the library folders, so a corrupt catalogue is
//! detected on open and can be replaced.
//!
//! A *photo* is separate from the *file* that currently holds it, so ratings and edits
//! follow a photograph when it is moved or renamed ([`Catalogue::record_file`]).

mod albums;
mod backup;
mod catalogue;
mod details;
mod edits;
mod error;
mod identity;
mod marks;
mod presets;
mod schema;
mod search;

pub use albums::{Album, AlbumId};
pub use backup::{BackupInfo, BackupKind, BackupStore, newest_valid, schema_version_of, to_prune};
pub use catalogue::{Catalogue, FileRecord, FileStatus, LibraryFolder, RecordOutcome, ScanId};
pub use details::{METADATA_VERSION, PhotoDetails, camera_name};
pub use edits::StoredEdit;
pub use error::CatalogueError;
pub use identity::SourceIdentity;
pub use marks::{
    Collection, CollectionCounts, CollectionEntry, ColourLabel, Flag, MarkChange, Marks,
    RECENT_DAYS, Rating,
};
pub use presets::{PresetId, StoredPreset};
pub use schema::SCHEMA_VERSION;

/// Row id of a photo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PhotoId(pub i64);

/// Row id of a library folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FolderId(pub i64);
