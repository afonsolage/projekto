use std::collections::VecDeque;

use bevy_math::IVec3;
use projekto_core::{
    chunk::{self, ChunkStorage},
    voxel::{self, LightTy},
};

fn calc_propagated_intensity(ty: LightTy, side: voxel::Side, intensity: u8) -> u8 {
    if side == voxel::Side::Down
        && ty == LightTy::Natural
        && intensity == voxel::Light::MAX_NATURAL_INTENSITY
    {
        intensity
    } else {
        assert!(intensity > 0);
        intensity - 1
    }
}

pub fn propagate(
    kind: &ChunkStorage<voxel::Kind>,
    light: &mut ChunkStorage<voxel::Light>,
    light_ty: LightTy,
    voxels: &[IVec3],
) {
    let mut queue = voxels.iter().copied().collect::<VecDeque<_>>();

    while let Some(voxel) = queue.pop_front() {
        let current_intensity = light.get(voxel).get(light_ty);

        for side in voxel::SIDES {
            let side_voxel = voxel + side.dir();
            let propagated_intensity = calc_propagated_intensity(light_ty, side, current_intensity);

            if !chunk::is_within_bounds(side_voxel) {
                // TODO: propagate to neighbor chunks
                continue;
            }

            let side_kind = kind.get(side_voxel);
            if side_kind.is_opaque() {
                continue;
            }

            let side_intensity = light.get(side_voxel).get(light_ty);
            if side_intensity >= propagated_intensity {
                continue;
            }

            light.set_type(side_voxel, light_ty, propagated_intensity);

            if propagated_intensity > 1 {
                queue.push_back(side_voxel);
            }
        }
    }
}
