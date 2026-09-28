//! Preview cache with an admission policy.

use std::sync::Arc;

use cache::{ByteLru, CacheStats, RenderKey};
use image_core::OutputImage;
use renderer::PreviewQuality;

/// Fraction of the preview budget reserved for interactive (drag) frames.
const TRANSIENT_SHARE: usize = 8;

/// Two LRU pools sharing one byte budget:
///
/// - `transient` holds interactive frames. A drag produces dozens of them, and nearly
///   all are never requested again, so they get a small pool (1/8 of the budget).
/// - `settled` holds thumbnail and detail renders: the states the user stopped on,
///   which before/after toggles and undo will revisit.
///
/// A long drag therefore cannot evict settled renders.
pub(crate) struct PreviewCache {
    settled: ByteLru<RenderKey, OutputImage>,
    transient: ByteLru<RenderKey, OutputImage>,
}

impl PreviewCache {
    pub fn new(budget_bytes: usize) -> Self {
        let transient = budget_bytes / TRANSIENT_SHARE;
        Self {
            settled: ByteLru::new(budget_bytes - transient),
            transient: ByteLru::new(transient),
        }
    }

    pub fn get(&mut self, key: &RenderKey) -> Option<Arc<OutputImage>> {
        // Probe `settled` without recording a miss, so the combined statistics count
        // one hit or one miss per lookup.
        if self.settled.contains(key) {
            return self.settled.get(key);
        }
        self.transient.get(key)
    }

    pub fn insert(&mut self, key: RenderKey, image: Arc<OutputImage>, quality: PreviewQuality) {
        let bytes = image.byte_size();
        match quality {
            PreviewQuality::Interactive => self.transient.insert(key, image, bytes),
            PreviewQuality::Thumbnail | PreviewQuality::Detail => {
                // A settled copy supersedes any transient one.
                self.transient.remove(&key);
                self.settled.insert(key, image, bytes)
            }
        };
    }

    pub fn retain(&mut self, keep: impl Fn(&RenderKey) -> bool) {
        self.settled.retain(&keep);
        self.transient.retain(&keep);
    }

    pub fn stats(&self) -> CacheStats {
        let (s, t) = (self.settled.stats(), self.transient.stats());
        CacheStats {
            hits: s.hits + t.hits,
            misses: s.misses + t.misses,
            evictions: s.evictions + t.evictions,
            entries: s.entries + t.entries,
            used_bytes: s.used_bytes + t.used_bytes,
            budget_bytes: s.budget_bytes + t.budget_bytes,
        }
    }
}
