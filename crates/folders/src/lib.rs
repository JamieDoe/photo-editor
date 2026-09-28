//! Folder browsing for the Library (Phase 1).
//!
//! Lists one folder level ([`list_folder`]) or walks a tree ([`walk_photos`]) for
//! indexing. Reads directory metadata only. Access is limited to folders the user
//! explicitly granted ([`FolderAccess`]).

mod access;
mod listing;
mod natural;
mod walk;

pub use access::FolderAccess;
pub use listing::{FolderEntry, FolderListing, PhotoEntry, list_folder};
pub use natural::natural_cmp;
pub use walk::{WalkStats, walk_photos};
