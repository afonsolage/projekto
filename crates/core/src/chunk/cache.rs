//! Generic per-chunk cache for [`ChunkStorage`] values.

use std::collections::{BTreeMap, HashMap};

use crate::{
    chunk::{ChunkStorage, ChunkStorageType},
    coords::Chunk,
};

/// Default number of chunks kept by [`ChunkStorageCache`].
pub const DEFAULT_CHUNK_STORAGE_CACHE_CAPACITY: usize = 32;

/// Cumulative cache counters, useful for tuning the cache size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
}

struct Entry<T: ChunkStorageType> {
    storage: ChunkStorage<T>,
    last_used: u64,
    pinned: bool,
}

/// A generic LRU cache of [`ChunkStorage`] values, keyed by [`Chunk`].
///
/// The cache is intentionally unaware of the data semantics: it does not know when an entry is
/// stale. Callers must [`invalidate`](Self::invalidate) chunks whose source data (and their
/// neighbors) changed. This keeps the cache usable for different derived layers, such as soft
/// light, occlusion or future water/fire propagation data, for instance.
///
/// Pinned entries are never evicted. If every entry is pinned the cache may temporarily exceed
/// its configured capacity instead of evicting pinned data.
pub struct ChunkStorageCache<T: ChunkStorageType> {
    entries: HashMap<Chunk, Entry<T>>,
    lru: BTreeMap<u64, Chunk>,
    tick: u64,
    capacity: usize,
    stats: CacheStats,
}

impl<T: ChunkStorageType> Default for ChunkStorageCache<T> {
    fn default() -> Self {
        Self::new(DEFAULT_CHUNK_STORAGE_CACHE_CAPACITY)
    }
}

impl<T: ChunkStorageType> ChunkStorageCache<T> {
    /// Creates a cache that keeps at most `capacity` entries. A capacity of zero is treated as
    /// one so [`get_or_insert_with`](Self::get_or_insert_with) can always return a reference.
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: HashMap::new(),
            lru: BTreeMap::new(),
            tick: 0,
            capacity: capacity.max(1),
            stats: CacheStats::default(),
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn contains(&self, chunk: Chunk) -> bool {
        self.entries.contains_key(&chunk)
    }

    pub fn stats(&self) -> CacheStats {
        self.stats
    }

    pub fn reset_stats(&mut self) {
        self.stats = CacheStats::default();
    }

    pub fn iter(&self) -> impl Iterator<Item = (Chunk, &ChunkStorage<T>)> {
        self.entries
            .iter()
            .map(|(chunk, entry)| (*chunk, &entry.storage))
    }

    /// Returns a cached storage and marks it as recently used.
    pub fn get(&mut self, chunk: Chunk) -> Option<&ChunkStorage<T>> {
        if !self.entries.contains_key(&chunk) {
            self.stats.misses += 1;
            return None;
        }

        self.stats.hits += 1;
        self.touch(chunk);
        self.entries.get(&chunk).map(|entry| &entry.storage)
    }

    /// Returns a mutable cached storage and marks it as recently used.
    pub fn get_mut(&mut self, chunk: Chunk) -> Option<&mut ChunkStorage<T>> {
        if !self.entries.contains_key(&chunk) {
            self.stats.misses += 1;
            return None;
        }

        self.stats.hits += 1;
        self.touch(chunk);
        self.entries.get_mut(&chunk).map(|entry| &mut entry.storage)
    }

    /// Inserts or replaces an entry, returning the previous value when present.
    pub fn insert(&mut self, chunk: Chunk, storage: ChunkStorage<T>) -> Option<ChunkStorage<T>> {
        let previous = self.remove_entry(chunk);

        self.evict_until_capacity();

        let tick = self.next_tick();
        self.entries.insert(
            chunk,
            Entry {
                storage,
                last_used: tick,
                pinned: false,
            },
        );
        self.lru.insert(tick, chunk);

        previous
    }

    /// Returns the cached storage, computing and inserting it on a miss.
    ///
    /// `compute` runs before eviction, so a panic cannot discard a previously cached entry.
    pub fn get_or_insert_with(
        &mut self,
        chunk: Chunk,
        compute: impl FnOnce() -> ChunkStorage<T>,
    ) -> &ChunkStorage<T> {
        if self.entries.contains_key(&chunk) {
            self.stats.hits += 1;
            self.touch(chunk);
        } else {
            self.stats.misses += 1;
            let storage = compute();
            self.insert(chunk, storage);
        }

        &self.entries[&chunk].storage
    }

    /// Mutable variant of [`get_or_insert_with`](Self::get_or_insert_with).
    pub fn get_mut_or_insert_with(
        &mut self,
        chunk: Chunk,
        compute: impl FnOnce() -> ChunkStorage<T>,
    ) -> &mut ChunkStorage<T> {
        if self.entries.contains_key(&chunk) {
            self.stats.hits += 1;
            self.touch(chunk);
        } else {
            self.stats.misses += 1;
            let storage = compute();
            self.insert(chunk, storage);
        }

        &mut self
            .entries
            .get_mut(&chunk)
            .expect("entry was inserted above")
            .storage
    }

    /// Removes and returns a cached entry.
    pub fn invalidate(&mut self, chunk: Chunk) -> Option<ChunkStorage<T>> {
        self.remove_entry(chunk)
    }

    /// Prevents an entry from being evicted until [`unpin`](Self::unpin) is called.
    pub fn pin(&mut self, chunk: Chunk) {
        let tick = self.next_tick();

        if let Some(entry) = self.entries.get_mut(&chunk)
            && !entry.pinned
        {
            let old_tick = entry.last_used;
            entry.pinned = true;
            entry.last_used = tick;
            self.lru.remove(&old_tick);
        }
    }

    /// Makes a pinned entry evictable again.
    pub fn unpin(&mut self, chunk: Chunk) {
        let tick = self.next_tick();

        if let Some(entry) = self.entries.get_mut(&chunk)
            && entry.pinned
        {
            entry.pinned = false;
            entry.last_used = tick;
            self.lru.insert(tick, chunk);
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.lru.clear();
    }

    fn next_tick(&mut self) -> u64 {
        self.tick = self.tick.wrapping_add(1);
        self.tick
    }

    fn touch(&mut self, chunk: Chunk) {
        let tick = self.next_tick();

        if let Some(entry) = self.entries.get_mut(&chunk) {
            let old_tick = entry.last_used;
            entry.last_used = tick;

            if !entry.pinned {
                self.lru.remove(&old_tick);
                self.lru.insert(tick, chunk);
            }
        }
    }

    fn remove_entry(&mut self, chunk: Chunk) -> Option<ChunkStorage<T>> {
        let entry = self.entries.remove(&chunk)?;

        if !entry.pinned {
            self.lru.remove(&entry.last_used);
        }

        Some(entry.storage)
    }

    fn evict_until_capacity(&mut self) {
        while self.entries.len() >= self.capacity {
            if !self.evict_one() {
                break;
            }
        }
    }

    fn evict_one(&mut self) -> bool {
        let Some((_, chunk)) = self.lru.pop_first() else {
            return false;
        };

        let entry = self
            .entries
            .remove(&chunk)
            .expect("LRU map must only reference cached entries");
        debug_assert!(!entry.pinned, "pinned entries must not be in the LRU map");

        self.stats.evictions += 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;
    use crate::{
        coords::{Chunk, ChunkVoxel},
        voxel::FacesSoftLight,
    };

    fn storage(value: u8) -> ChunkStorage<u8> {
        let mut storage = ChunkStorage::default();
        storage.set(ChunkVoxel::new(0, 0, 0), value);
        storage
    }

    fn cached_value(cache: &mut ChunkStorageCache<u8>, chunk: Chunk) -> Option<u8> {
        cache
            .get(chunk)
            .map(|storage| storage.get(ChunkVoxel::new(0, 0, 0)))
    }

    #[test]
    fn insert_get_and_contains() {
        let chunk = Chunk::new(1, 2);
        let mut cache = ChunkStorageCache::<u8>::new(2);

        assert!(cache.is_empty());
        assert_eq!(cache.insert(chunk, storage(7)), None);

        assert!(!cache.is_empty());
        assert_eq!(cache.len(), 1);
        assert!(cache.contains(chunk));
        assert_eq!(cached_value(&mut cache, chunk), Some(7));
    }

    #[test]
    fn insert_replaces_existing_entry() {
        let chunk = Chunk::new(1, 2);
        let mut cache = ChunkStorageCache::<u8>::new(2);

        assert_eq!(cache.insert(chunk, storage(7)), None);
        let old = cache.insert(chunk, storage(9));

        assert_eq!(
            old.map(|storage| storage.get(ChunkVoxel::new(0, 0, 0))),
            Some(7)
        );
        assert_eq!(cache.len(), 1);
        assert_eq!(cached_value(&mut cache, chunk), Some(9));
    }

    #[test]
    fn lru_evicts_least_recently_used() {
        let a = Chunk::new(0, 0);
        let b = Chunk::new(1, 0);
        let c = Chunk::new(2, 0);
        let mut cache = ChunkStorageCache::<u8>::new(2);

        cache.insert(a, storage(1));
        cache.insert(b, storage(2));

        // Touch `a`, so `b` becomes the least recently used entry.
        assert_eq!(cached_value(&mut cache, a), Some(1));

        cache.insert(c, storage(3));

        assert!(cache.contains(a), "recently used entry must survive");
        assert!(
            !cache.contains(b),
            "least recently used entry must be evicted"
        );
        assert!(cache.contains(c));
    }

    #[test]
    fn pinned_entry_survives_eviction() {
        let a = Chunk::new(0, 0);
        let b = Chunk::new(1, 0);
        let c = Chunk::new(2, 0);
        let mut cache = ChunkStorageCache::<u8>::new(2);

        cache.insert(a, storage(1));
        cache.pin(a);
        cache.insert(b, storage(2));
        cache.insert(c, storage(3));

        assert!(cache.contains(a), "pinned entry must not be evicted");
        assert!(!cache.contains(b));
        assert!(cache.contains(c));
    }

    #[test]
    fn unpin_allows_eviction() {
        let a = Chunk::new(0, 0);
        let b = Chunk::new(1, 0);
        let c = Chunk::new(2, 0);
        let mut cache = ChunkStorageCache::<u8>::new(1);

        cache.insert(a, storage(1));
        cache.pin(a);
        cache.insert(b, storage(2));
        cache.pin(b);

        assert_eq!(cache.len(), 2, "pinned entries may exceed capacity");

        cache.unpin(a);
        cache.insert(c, storage(3));

        assert!(!cache.contains(a), "unpinned entry must be evictable");
        assert!(cache.contains(b));
        assert!(cache.contains(c));
    }

    #[test]
    fn all_pinned_entries_may_exceed_capacity() {
        let a = Chunk::new(0, 0);
        let b = Chunk::new(1, 0);
        let mut cache = ChunkStorageCache::<u8>::new(1);

        cache.insert(a, storage(1));
        cache.pin(a);
        cache.insert(b, storage(2));
        cache.pin(b);

        assert_eq!(cache.len(), 2);
        assert_eq!(cache.capacity(), 1);
    }

    #[test]
    fn get_or_insert_computes_only_on_miss() {
        let chunk = Chunk::new(0, 0);
        let mut cache = ChunkStorageCache::<u8>::new(2);
        let computations = Cell::new(0);

        cache.get_or_insert_with(chunk, || {
            computations.set(computations.get() + 1);
            storage(7)
        });
        cache.get_or_insert_with(chunk, || {
            computations.set(computations.get() + 1);
            storage(9)
        });

        assert_eq!(computations.get(), 1);
        assert_eq!(cached_value(&mut cache, chunk), Some(7));
    }

    #[test]
    fn get_mut_or_insert_updates_value() {
        let chunk = Chunk::new(0, 0);
        let mut cache = ChunkStorageCache::<u8>::new(2);

        cache
            .get_mut_or_insert_with(chunk, || storage(1))
            .set(ChunkVoxel::new(0, 0, 0), 42);

        assert_eq!(cached_value(&mut cache, chunk), Some(42));
    }

    #[test]
    fn invalidate_removes_entry() {
        let chunk = Chunk::new(0, 0);
        let mut cache = ChunkStorageCache::<u8>::new(2);

        cache.insert(chunk, storage(7));

        let removed = cache.invalidate(chunk);

        assert_eq!(
            removed.map(|storage| storage.get(ChunkVoxel::new(0, 0, 0))),
            Some(7)
        );
        assert!(!cache.contains(chunk));
        assert!(cache.is_empty());
        assert_eq!(cache.invalidate(chunk), None);
    }

    #[test]
    fn clear_empties_cache() {
        let a = Chunk::new(0, 0);
        let b = Chunk::new(1, 0);
        let mut cache = ChunkStorageCache::<u8>::new(2);

        cache.insert(a, storage(1));
        cache.insert(b, storage(2));
        cache.clear();

        assert!(cache.is_empty());
        assert!(!cache.contains(a));
        assert!(!cache.contains(b));
    }

    #[test]
    fn zero_capacity_keeps_one_entry() {
        let a = Chunk::new(0, 0);
        let b = Chunk::new(1, 0);
        let mut cache = ChunkStorageCache::<u8>::new(0);

        assert_eq!(cache.capacity(), 1);

        cache.insert(a, storage(1));
        cache.insert(b, storage(2));

        assert!(!cache.contains(a));
        assert!(cache.contains(b));
    }

    #[test]
    fn stats_track_hits_misses_and_evictions() {
        let a = Chunk::new(0, 0);
        let b = Chunk::new(1, 0);
        let mut cache = ChunkStorageCache::<u8>::new(1);

        assert!(cache.get(a).is_none());
        cache.insert(a, storage(1));
        assert!(cache.get(a).is_some());
        cache.insert(b, storage(2));
        assert!(cache.get(a).is_none());

        let stats = cache.stats();
        assert_eq!(stats.misses, 2);
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.evictions, 1);
    }

    #[test]
    fn works_with_other_chunk_storage_types() {
        let chunk = Chunk::new(0, 0);
        let mut cache = ChunkStorageCache::<FacesSoftLight>::new(1);

        cache.insert(chunk, ChunkStorage::default());

        assert!(cache.contains(chunk));
    }
}
