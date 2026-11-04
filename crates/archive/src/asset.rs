use projekto_core::{chunk::ChunkStorage, coords::Chunk, voxel};
use serde::{Deserialize, Serialize};

/// Bumped whenever the generated mesh representation changes, so cached meshes are remeshed.
pub const CHUNK_MESH_VERSION: u8 = 3;

/// Persisted chunk data.
///
/// This is the on-disk schema of the region archive. Changing the fields or their order
/// requires bumping [`VERSION_MAGIC`](crate::VERSION_MAGIC).
#[derive(Default, Debug, Serialize, Deserialize)]
pub struct ChunkAsset {
    pub chunk: Chunk,
    pub kind: ChunkStorage<voxel::Kind>,
    pub light: ChunkStorage<voxel::Light>,
    pub vertex: Vec<voxel::Vertex>,
    pub lod: u8,
    pub mesh_version: u8,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_serde() {
        let asset = ChunkAsset {
            lod: 1,
            mesh_version: CHUNK_MESH_VERSION,
            ..Default::default()
        };

        let bytes = bincode::serde::encode_to_vec(&asset, bincode::config::standard()).unwrap();

        assert!(!bytes.is_empty());

        let (serde_asset, _): (ChunkAsset, usize) =
            bincode::serde::decode_from_slice(&bytes, bincode::config::standard()).unwrap();

        assert_eq!(asset.chunk, serde_asset.chunk);
        assert_eq!(asset.kind, serde_asset.kind);
        assert_eq!(asset.light, serde_asset.light);
        assert_eq!(asset.vertex, serde_asset.vertex);
        assert_eq!(asset.lod, serde_asset.lod);
        assert_eq!(asset.mesh_version, serde_asset.mesh_version);
    }

    #[test]
    fn asset_serde_pallet() {
        let mut kind = ChunkStorage::<voxel::Kind>::default();
        let mut light = ChunkStorage::<voxel::Light>::default();

        for (i, voxel) in projekto_core::chunk::voxels().take(4096).enumerate() {
            kind.set(voxel, voxel::Kind::id((i % 2) as u16));
            light.set(voxel, voxel::Light::from((i % 2) as u8));
        }

        kind.pack();
        light.pack();

        let asset = ChunkAsset {
            chunk: Chunk::default(),
            kind,
            light,
            vertex: Vec::new(),
            lod: 0,
            mesh_version: CHUNK_MESH_VERSION,
        };

        let bytes = bincode::serde::encode_to_vec(&asset, bincode::config::standard()).unwrap();
        let (serde_asset, _): (ChunkAsset, usize) =
            bincode::serde::decode_from_slice(&bytes, bincode::config::standard()).unwrap();

        assert_eq!(asset.chunk, serde_asset.chunk);
        assert_eq!(asset.kind, serde_asset.kind);
        assert_eq!(asset.light, serde_asset.light);
        assert_eq!(asset.vertex, serde_asset.vertex);
        assert_eq!(asset.lod, serde_asset.lod);
    }
}
