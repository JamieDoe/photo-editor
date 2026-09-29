use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;

/// Hit/miss/eviction counters for diagnostics and benchmarks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub entries: usize,
    pub used_bytes: usize,
    pub budget_bytes: usize,
}

struct Entry<V> {
    value: Arc<V>,
    bytes: usize,
    last_used: u64,
}

/// Least-recently-used cache bounded by a byte budget.
///
/// Eviction scans for the oldest entry (O(n)). Preview caches hold tens of entries,
/// so this is cheaper and simpler than maintaining an intrusive list. Revisit if a
/// cache ever holds thousands of entries (e.g. thumbnails).
pub struct ByteLru<K, V> {
    budget: usize,
    used: usize,
    tick: u64,
    map: HashMap<K, Entry<V>>,
    stats: CacheStats,
}

impl<K: Eq + Hash + Clone, V> ByteLru<K, V> {
    pub fn new(budget_bytes: usize) -> Self {
        Self {
            budget: budget_bytes,
            used: 0,
            tick: 0,
            map: HashMap::new(),
            stats: CacheStats {
                budget_bytes,
                ..CacheStats::default()
            },
        }
    }

    /// Whether `key` is present, without touching recency or statistics.
    pub fn contains(&self, key: &K) -> bool {
        self.map.contains_key(key)
    }

    pub fn get(&mut self, key: &K) -> Option<Arc<V>> {
        self.tick += 1;
        match self.map.get_mut(key) {
            Some(entry) => {
                entry.last_used = self.tick;
                self.stats.hits += 1;
                Some(Arc::clone(&entry.value))
            }
            None => {
                self.stats.misses += 1;
                None
            }
        }
    }

    /// Inserts a value of `bytes` size, evicting old entries to stay within budget.
    /// Values larger than the whole budget are not cached. Returns whether it was stored.
    pub fn insert(&mut self, key: K, value: Arc<V>, bytes: usize) -> bool {
        if bytes > self.budget {
            return false;
        }
        if let Some(old) = self.map.remove(&key) {
            self.used -= old.bytes;
        }
        while self.used + bytes > self.budget {
            if !self.evict_oldest() {
                break;
            }
        }
        self.tick += 1;
        self.used += bytes;
        self.map.insert(
            key,
            Entry {
                value,
                bytes,
                last_used: self.tick,
            },
        );
        true
    }

    pub fn remove(&mut self, key: &K) -> Option<Arc<V>> {
        self.map.remove(key).map(|e| {
            self.used -= e.bytes;
            e.value
        })
    }

    /// Removes all entries matching a predicate (e.g. every render of a closed image).
    pub fn retain(&mut self, mut keep: impl FnMut(&K) -> bool) {
        let used = &mut self.used;
        self.map.retain(|k, e| {
            let kept = keep(k);
            if !kept {
                *used -= e.bytes;
            }
            kept
        });
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.used = 0;
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn used_bytes(&self) -> usize {
        self.used
    }

    pub fn stats(&self) -> CacheStats {
        CacheStats {
            entries: self.map.len(),
            used_bytes: self.used,
            ..self.stats
        }
    }

    /// Changes the byte budget, evicting least recently used entries to fit.
    pub fn set_budget(&mut self, budget_bytes: usize) {
        self.budget = budget_bytes;
        self.stats.budget_bytes = budget_bytes;
        while self.used > self.budget {
            if !self.evict_oldest() {
                break;
            }
        }
    }

    fn evict_oldest(&mut self) -> bool {
        let oldest = self
            .map
            .iter()
            .min_by_key(|(_, e)| e.last_used)
            .map(|(k, _)| k.clone());
        match oldest {
            Some(k) => {
                if let Some(e) = self.map.remove(&k) {
                    self.used -= e.bytes;
                    self.stats.evictions += 1;
                }
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_returns_inserted_value() {
        let mut c = ByteLru::new(100);
        c.insert("a", Arc::new(1), 10);
        assert_eq!(c.get(&"a").as_deref(), Some(&1));
        assert_eq!(c.get(&"b"), None);
        let s = c.stats();
        assert_eq!((s.hits, s.misses, s.entries, s.used_bytes), (1, 1, 1, 10));
    }

    #[test]
    fn evicts_least_recently_used_within_budget() {
        let mut c = ByteLru::new(30);
        c.insert("a", Arc::new(1), 10);
        c.insert("b", Arc::new(2), 10);
        c.insert("c", Arc::new(3), 10);
        c.get(&"a"); // "b" is now the least recently used
        c.insert("d", Arc::new(4), 10);
        assert!(c.get(&"b").is_none());
        assert!(c.get(&"a").is_some());
        assert!(c.get(&"c").is_some());
        assert!(c.get(&"d").is_some());
        assert_eq!(c.used_bytes(), 30);
        assert_eq!(c.stats().evictions, 1);
    }

    #[test]
    fn never_exceeds_budget() {
        let mut c = ByteLru::new(50);
        for i in 0..100 {
            c.insert(i, Arc::new(i), 7 + (i % 5));
            assert!(c.used_bytes() <= 50);
        }
    }

    #[test]
    fn contains_does_not_affect_recency_or_stats() {
        let mut c = ByteLru::new(20);
        c.insert("a", Arc::new(1), 10);
        c.insert("b", Arc::new(2), 10);
        assert!(c.contains(&"a"));
        c.insert("c", Arc::new(3), 10); // "a" is still the oldest
        assert!(!c.contains(&"a"));
        assert_eq!((c.stats().hits, c.stats().misses), (0, 0));
    }

    #[test]
    fn shrinking_budget_evicts_oldest_first() {
        let mut c = ByteLru::new(100);
        for k in 0..5 {
            c.insert(k, Arc::new(()), 20);
        }
        c.get(&0); // most recently used survives
        c.set_budget(40);
        assert_eq!(c.used_bytes(), 40);
        assert!(c.contains(&0) && c.contains(&4));
        assert_eq!(c.stats().budget_bytes, 40);
    }

    #[test]
    fn oversized_value_is_not_cached() {
        let mut c = ByteLru::new(10);
        c.insert("small", Arc::new(0), 5);
        assert!(!c.insert("big", Arc::new(1), 11));
        assert!(c.get(&"small").is_some(), "oversized insert must not evict");
    }

    #[test]
    fn reinsert_replaces_and_accounts_bytes() {
        let mut c = ByteLru::new(100);
        c.insert("a", Arc::new(1), 40);
        c.insert("a", Arc::new(2), 10);
        assert_eq!(c.used_bytes(), 10);
        assert_eq!(c.get(&"a").as_deref(), Some(&2));
    }

    #[test]
    fn retain_updates_accounting() {
        let mut c = ByteLru::new(100);
        c.insert(1, Arc::new(()), 10);
        c.insert(2, Arc::new(()), 20);
        c.retain(|k| *k != 2);
        assert_eq!((c.len(), c.used_bytes()), (1, 10));
    }
}
