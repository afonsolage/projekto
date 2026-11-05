use bevy::{math::IVec2, platform::collections::HashSet, prelude::*};
use projekto_core::{chunk, chunk::ChunkStorageCache, coords::Chunk, voxel};

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

// @TODO: Add unit tests for this.
/// Returns `chunk` and its four direct horizontal neighbors.
///
/// Geometry seams only exist on shared faces. Diagonal chunks can affect corner soft light,
/// but they do not share a face with `chunk`, so dirtying them for an LOD or seam change only
/// produces redundant remeshes.
pub fn adjacent_chunks(chunk: Chunk) -> impl Iterator<Item = Chunk> {
    std::iter::once(chunk).chain(
        chunk::SIDES
            .iter()
            .map(move |side| chunk.neighbor(side.dir())),
    )
}

/// Returns `chunk` and its eight surrounding chunks.
///
/// Soft light samples diagonal chunks at voxel corners, so a change in any of the eight
/// neighbors can invalidate the center chunk's derived data.
pub fn affected_chunks(chunk: Chunk) -> impl Iterator<Item = Chunk> {
    (-1..=1).flat_map(move |dx| (-1..=1).map(move |dz| Chunk::new(chunk.x + dx, chunk.z + dz)))
}

/// Squared distance between a chunk and a landscape centre, used for nearest-first work.
pub fn distance_sq(chunk: Chunk, center: IVec2) -> i64 {
    let dx = chunk.x as i64 - center.x as i64;
    let dz = chunk.z as i64 - center.y as i64;
    dx * dx + dz * dz
}

/// Sorts chunks nearest-first for streaming and meshing work.
pub fn sort_nearest_first(chunks: &mut [Chunk], center: IVec2) {
    chunks.sort_by_key(|chunk| distance_sq(*chunk, center));
}

impl Default for DerivedCaches {
    fn default() -> Self {
        Self::new(DEFAULT_DERIVED_CACHE_CAPACITY)
    }
}

/// Chunks whose derived data and/or vertices must be rebuilt on the next meshing tick.
#[derive(Resource, Default)]
pub struct DirtyChunks(pub HashSet<Chunk>);

#[cfg(test)]
mod distance_tests {
    use bevy::math::IVec2;

    use super::*;

    #[test]
    fn sorts_nearest_first() {
        let mut chunks = vec![
            Chunk::new(2, 0),
            Chunk::new(-1, 0),
            Chunk::new(0, 0),
            Chunk::new(1, 1),
        ];

        sort_nearest_first(&mut chunks, IVec2::ZERO);

        assert_eq!(
            chunks,
            vec![
                Chunk::new(0, 0),
                Chunk::new(-1, 0),
                Chunk::new(1, 1),
                Chunk::new(2, 0),
            ]
        );
    }
}
