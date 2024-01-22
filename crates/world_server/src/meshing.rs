use bevy_math::IVec3;
use projekto_core::{
    chunk::{self, ChunkStorage},
    math,
    voxel::{self, FacesOcclusion},
};

pub(super) fn faces_occlusion(
    kind: &ChunkStorage<voxel::Kind>,
    faces_occlusion: &mut ChunkStorage<voxel::FacesOcclusion>,
    neighboorhood: &[Option<&ChunkStorage<voxel::Kind>>; voxel::SIDE_COUNT],
) {
    chunk::voxels().for_each(|voxel| {
        if kind.get(voxel).is_none() {
            faces_occlusion.set(voxel, voxel::FacesOcclusion::fully_occluded())
        } else {
            let mut faces = FacesOcclusion::default();
            voxel::SIDES.iter().for_each(|&side| {
                let neighbor = voxel + side.dir();

                let neighbor_kind = if chunk::is_within_bounds(neighbor) {
                    kind.get(neighbor)
                } else {
                    let Some(neighbor_kind) = neighboorhood[side as usize] else {
                        return;
                    };
                    let neighbor_chunk_voxel = math::euclid_rem(
                        neighbor,
                        IVec3::new(
                            chunk::X_AXIS_SIZE as i32,
                            chunk::Y_AXIS_SIZE as i32,
                            chunk::Z_AXIS_SIZE as i32,
                        ),
                    );
                    neighbor_kind.get(neighbor_chunk_voxel)
                };

                faces.set(side, !neighbor_kind.is_none());
            });
            faces_occlusion.set(voxel, faces);
        }
    })
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn faces_occlusion_empty_chunk() {
        let kind = Default::default();
        let mut faces_occlusion = Default::default();
        let neighborhood = [None; voxel::SIDE_COUNT];

        super::faces_occlusion(&kind, &mut faces_occlusion, &neighborhood);

        assert!(
            faces_occlusion.is_fully_occluded(),
            "Should be fully occluded in an empty chunk"
        );
    }

    #[test]
    fn faces_occlusion_opaque_voxel() {
        let mut kind = ChunkStorage::<voxel::Kind>::default();
        let mut faces_occlusion = Default::default();
        let neighborhood = [None; voxel::SIDE_COUNT];

        kind.set([0, 0, 0].into(), 1.into());

        super::faces_occlusion(&kind, &mut faces_occlusion, &neighborhood);

        let occ = faces_occlusion.get([0, 0, 0].into());

        voxel::SIDES.iter().for_each(|&side| {
            assert!(!occ.is_occluded(side), "No side should be occluded");
        })
    }

    #[test]
    fn faces_occlusion_neighbor() {
        let mut kind = ChunkStorage::<voxel::Kind>::default();
        let mut neighbor_kind = ChunkStorage::<voxel::Kind>::default();
        let mut faces_occlusion = Default::default();
        let mut neighborhood = [None; voxel::SIDE_COUNT];

        kind.set([0, 0, 0].into(), 1.into());
        neighbor_kind.set([chunk::X_END, 0, 0].into(), 1.into());
        neighborhood[voxel::Side::Left as usize] = Some(&neighbor_kind);

        super::faces_occlusion(&kind, &mut faces_occlusion, &neighborhood);

        let occ = faces_occlusion.get([0, 0, 0].into());

        voxel::SIDES.iter().for_each(|&side| {
            if side == voxel::Side::Left {
                assert!(occ.is_occluded(side), "Left side should be occluded");
            } else {
                assert!(
                    !occ.is_occluded(side),
                    "All other sides should not be occluded"
                );
            }
        })
    }
}
