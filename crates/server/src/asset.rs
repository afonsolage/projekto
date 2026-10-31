use bevy::prelude::*;
use projekto_core::{chunk::ChunkStorage, coords::Chunk, voxel};
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
        assert_eq!(asset.occlusion, serde_asset.occlusion);
        assert_eq!(asset.soft_light, serde_asset.soft_light);
        assert_eq!(asset.vertex, serde_asset.vertex);
    }

    #[test]
    fn asset_serde_pallet() {
        let mut kind = ChunkStorage::<voxel::Kind>::default();
        let mut light = ChunkStorage::<voxel::Light>::default();
        let mut occlusion = ChunkStorage::<voxel::FacesOcclusion>::default();
        let mut soft_light = ChunkStorage::<voxel::FacesSoftLight>::default();

        for (i, voxel) in projekto_core::chunk::voxels().take(4096).enumerate() {
            kind.set(voxel, voxel::Kind::id((i % 2) as u16));
            light.set(voxel, voxel::Light::from((i % 2) as u8));
            occlusion.set(
                voxel,
                if i % 2 == 0 {
                    voxel::FacesOcclusion::fully_occluded()
                } else {
                    voxel::FacesOcclusion::default()
                },
            );
            soft_light.set(voxel, voxel::FacesSoftLight::with_intensity((i % 2) as u8));
        }

        kind.pack();
        light.pack();
        occlusion.pack();
        soft_light.pack();

        let asset = ChunkAsset {
            chunk: Chunk::default(),
            kind,
            light,
            occlusion,
            soft_light,
            vertex: Vec::new(),
        };

        let bytes = bincode::serde::encode_to_vec(&asset, bincode::config::standard()).unwrap();
        let (serde_asset, _): (ChunkAsset, usize) =
            bincode::serde::decode_from_slice(&bytes, bincode::config::standard()).unwrap();

        assert_eq!(asset.chunk, serde_asset.chunk);
        assert_eq!(asset.kind, serde_asset.kind);
        assert_eq!(asset.light, serde_asset.light);
        assert_eq!(asset.occlusion, serde_asset.occlusion);
        assert_eq!(asset.soft_light, serde_asset.soft_light);
        assert_eq!(asset.vertex, serde_asset.vertex);
    }
}
