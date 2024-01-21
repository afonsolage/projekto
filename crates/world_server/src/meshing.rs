use projekto_core::{
    chunk::{self, ChunkStorage},
    voxel,
};

pub(super) fn faces_occlusion(
    kind: &ChunkStorage<voxel::Kind>,
    faces_occlusion: &mut ChunkStorage<voxel::FacesOcclusion>,
    neighboorhood: &[Option<&ChunkStorage<voxel::Kind>>; voxel::SIDE_COUNT],
) {
    //
}
