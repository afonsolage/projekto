use projekto_core::{
    chunk::{Chunk, ChunkStorage},
    voxel,
};

pub struct ChunkCache {
    pub chunk: Chunk,
    pub kind: ChunkStorage<voxel::Kind>,
    pub light: ChunkStorage<voxel::Light>,
    pub occlusion: ChunkStorage<voxel::FacesOcclusion>,
    pub soft_light: ChunkStorage<voxel::FacesSoftLight>,
    pub vertex: Vec<voxel::Vertex>,
}

impl ChunkCache {
    pub fn load(_chunk: Chunk) -> Self {
        todo!();
    }

    pub fn save(self) {
        todo!();
    }
}
