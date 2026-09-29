//! Bounded, disposable caches.
//!
//! Nothing in this crate is permanent state: every entry can be recomputed from the
//! source file plus the edit recipe. Caches are bounded by a byte budget and evict the
//! least recently used entry first.

mod disk;
mod hash;
mod key;
mod lru;

pub use disk::{DiskCache, DiskCacheStats};
pub use hash::{Fnv64, fnv1a64};
pub use key::{RenderKey, SourceId};
pub use lru::{ByteLru, CacheStats};
