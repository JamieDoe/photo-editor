//! Folder browsing for the Library (Phase 1).
//!
//! Lists one folder level: subfolders and supported photos. No indexing, database or
//! thumbnails; those are the Phase 2 catalogue. Access is limited to folders the user
//! explicitly granted ([`FolderAccess`]).

mod access;
mod listing;
mod natural;

pub use access::FolderAccess;
pub use listing::{FolderEntry, FolderListing, PhotoEntry, list_folder};
pub use natural::natural_cmp;
