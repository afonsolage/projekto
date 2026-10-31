use bevy::{platform::collections::HashSet, prelude::*};
use projekto_core::{chunk::ChunkStorageCache, coords::Chunk, voxel};

/// Number of derived chunk layers kept by the server's LRU caches.
pub const DEFAULT_DERIVED_CACHE_CAPACITY: usize =
    projekto_core::chunk::DEFAULT_CHUNK_STORAGE_CACHE_CAPACITY;

/// Non-persistent derived data caches.
///
/// `Kind` and `Light` stay on the chunk entities; occlusion and soft light are recomputed on
/// demand and kept here for recently meshed chunks. The cache is intentionally separate from
/// the entity/archive data so it can be evicted without despawning a chunk.
#[derive(Resource)]
pub struct DerivedCaches {
    pub occlusion: ChunkStorageCache<voxel::FacesOcclusion>,
    pub soft_light: ChunkStorageCache<voxel::FacesSoftLight>,
}

impl DerivedCaches {
    pub fn new(capacity: usize) -> Self {
        Self {
            occlusion: ChunkStorageCache::new(capacity),
            soft_light: ChunkStorageCache::new(capacity),
        }
    }

    /// Invalidates all derived layers for `chunk`.
    pub fn invalidate(&mut self, chunk: Chunk) {
        let _ = self.occlusion.invalidate(chunk);
        let _ = self.soft_light.invalidate(chunk);
    }

    /// Invalidates only the light-dependent layers for `chunk`.
    ///
    /// Occlusion depends on `Kind`, so it survives a light-only update.
    pub fn invalidate_light(&mut self, chunk: Chunk) {
        let _ = self.soft_light.invalidate(chunk);
    }
}

/// Returns `chunk` and its eight surrounding chunks.
///
/// Soft light samples diagonal chunks at voxel corners, so a change in any of the eight
/// neighbors can invalidate the center chunk's derived data.
pub fn affected_chunks(chunk: Chunk) -> impl Iterator<Item = Chunk> {
    (-1..=1).flat_map(move |dx| (-1..=1).map(move |dz| Chunk::new(chunk.x + dx, chunk.z + dz)))
}

impl Default for DerivedCaches {
    fn default() -> Self {
        Self::new(DEFAULT_DERIVED_CACHE_CAPACITY)
    }
}

/// Chunks whose derived data and/or vertices must be rebuilt on the next meshing tick.
#[derive(Resource, Default)]
pub struct DirtyChunks(pub HashSet<Chunk>);
