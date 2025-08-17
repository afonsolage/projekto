use bevy::prelude::*;
use projekto_core::{
    chunk::{Chunk, ChunkStorage},
    voxel,
};
use serde::{Deserialize, Serialize};

#[derive(Asset, Component, Default, Debug, TypePath, Serialize, Deserialize)]
pub struct ChunkAsset {
    pub chunk: Chunk,
    pub kind: ChunkStorage<voxel::Kind>,
    pub light: ChunkStorage<voxel::Light>,
    pub occlusion: ChunkStorage<voxel::FacesOcclusion>,
    pub soft_light: ChunkStorage<voxel::FacesSoftLight>,
    pub vertex: Vec<voxel::Vertex>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_serde() {
        let asset = ChunkAsset::default();

        let bytes = bincode::serde::encode_to_vec(&asset, bincode::config::standard()).unwrap();

        assert!(!bytes.is_empty());

        let (serde_asset, _): (ChunkAsset, usize) =
            bincode::serde::decode_from_slice(&bytes, bincode::config::standard()).unwrap();

        assert_eq!(asset.chunk, serde_asset.chunk);
        assert_eq!(asset.kind, serde_asset.kind);
        assert_eq!(asset.light, serde_asset.light);
        assert_eq!(asset.vertex, serde_asset.vertex);
    }
}
