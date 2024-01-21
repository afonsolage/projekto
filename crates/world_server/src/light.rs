use std::collections::VecDeque;

use bevy_math::IVec3;
use projekto_core::{
    chunk::{self, ChunkStorage},
    math,
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

pub struct NeighborLightPropagation {
    pub dir: IVec3,
    pub voxel: IVec3,
    pub ty: LightTy,
    pub intensity: u8,
}

pub fn propagate(
    kind: &ChunkStorage<voxel::Kind>,
    light: &mut ChunkStorage<voxel::Light>,
    light_ty: LightTy,
    voxels: &[IVec3],
) -> Vec<NeighborLightPropagation> {
    let mut queue = voxels.iter().copied().collect::<VecDeque<_>>();
    let mut neighbor_light_propagation = vec![];

    while let Some(voxel) = queue.pop_front() {
        let current_intensity = light.get(voxel).get(light_ty);

        for side in voxel::SIDES {
            let side_voxel = voxel + side.dir();
            let propagated_intensity = calc_propagated_intensity(light_ty, side, current_intensity);

            if !chunk::is_within_bounds(side_voxel) {
                if propagated_intensity > 1 {
                    let neighbor_voxel = math::euclid_rem(
                        side_voxel,
                        IVec3::new(
                            chunk::X_AXIS_SIZE as i32,
                            chunk::Y_AXIS_SIZE as i32,
                            chunk::Z_AXIS_SIZE as i32,
                        ),
                    );

                    neighbor_light_propagation.push(NeighborLightPropagation {
                        dir: side.dir(),
                        voxel: neighbor_voxel,
                        ty: light_ty,
                        intensity: propagated_intensity,
                    });
                }
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

    neighbor_light_propagation
}

#[cfg(test)]
mod test {
    use bevy_utils::HashMap;

    use super::*;

    #[test]
    fn propagate_empty_chunk() {
        let kind = ChunkStorage::<voxel::Kind>::default();
        let mut light = ChunkStorage::<voxel::Light>::default();

        let top_voxels = (0..=chunk::X_END)
            .flat_map(|x| (0..=chunk::Z_END).map(move |z| IVec3::new(x, chunk::Y_END, z)))
            .collect::<Vec<_>>();

        top_voxels.iter().for_each(|&voxel| {
            light.set_type(voxel, LightTy::Natural, voxel::Light::MAX_NATURAL_INTENSITY);
        });

        let _ = propagate(&kind, &mut light, LightTy::Natural, &top_voxels);

        assert!(
            light
                .iter()
                .all(|l| l.get(LightTy::Natural) == voxel::Light::MAX_NATURAL_INTENSITY),
            "All voxels should have full natural light propagated"
        );
    }

    #[test]
    fn propagate_enclosed_light() {
        let mut kind = ChunkStorage::<voxel::Kind>::default();
        let mut light = ChunkStorage::<voxel::Light>::default();

        let top_voxels = (0..=chunk::X_END)
            .flat_map(|x| (0..=chunk::Z_END).map(move |z| IVec3::new(x, chunk::Y_END, z)))
            .collect::<Vec<_>>();

        top_voxels.iter().for_each(|&voxel| {
            light.set_type(voxel, LightTy::Natural, voxel::Light::MAX_NATURAL_INTENSITY);
        });

        kind.set(IVec3::new(1, 0, 0), 1.into());
        kind.set(IVec3::new(0, 1, 0), 1.into());
        kind.set(IVec3::new(0, 0, 1), 1.into());

        let _ = propagate(&kind, &mut light, LightTy::Natural, &top_voxels);

        assert_eq!(
            light.get(IVec3::ZERO).get(LightTy::Natural),
            0,
            "Should not propagate to enclosed voxels"
        );

        assert_eq!(
            light.get(IVec3::new(1, 0, 0)).get(LightTy::Natural),
            0,
            "Should not propagate to opaque voxels"
        );

        assert_eq!(
            light.get(IVec3::new(2, 0, 0)).get(LightTy::Natural),
            voxel::Light::MAX_NATURAL_INTENSITY,
            "Should propagate at max intensity to empty voxels"
        );
    }

    #[test]
    fn propagate_partial_enclosed_light() {
        let mut kind = ChunkStorage::<voxel::Kind>::default();
        let mut light = ChunkStorage::<voxel::Light>::default();

        let top_voxels = (0..=chunk::X_END)
            .flat_map(|x| (0..=chunk::Z_END).map(move |z| IVec3::new(x, chunk::Y_END, z)))
            .collect::<Vec<_>>();

        top_voxels.iter().for_each(|&voxel| {
            light.set_type(voxel, LightTy::Natural, voxel::Light::MAX_NATURAL_INTENSITY);
        });

        // block all light except for a voxel at 0, 1, 0
        (0..=chunk::X_END)
            .flat_map(|x| (0..=chunk::Z_END).map(move |z| IVec3::new(x, 1, z)))
            .skip(1)
            .for_each(|v| {
                kind.set(v, 1.into());
            });

        let _ = propagate(&kind, &mut light, LightTy::Natural, &top_voxels);

        (0..=chunk::Z_END).enumerate().for_each(|(i, z)| {
            let voxel = IVec3::new(0, 0, z);
            assert_eq!(
                light.get(voxel).get(LightTy::Natural),
                voxel::Light::MAX_NATURAL_INTENSITY - i as u8,
                "Should propagate decreasing intensity at {voxel}"
            );
        });
    }

    #[test]
    fn propagate_to_neighborhood_empty_chunk() {
        let kind = ChunkStorage::<voxel::Kind>::default();
        let mut light = ChunkStorage::<voxel::Light>::default();

        let top_voxels = (0..=chunk::X_END)
            .flat_map(|x| (0..=chunk::Z_END).map(move |z| IVec3::new(x, chunk::Y_END, z)))
            .collect::<Vec<_>>();

        top_voxels.iter().for_each(|&voxel| {
            light.set_type(voxel, LightTy::Natural, voxel::Light::MAX_NATURAL_INTENSITY);
        });

        let neighbor_propagation = propagate(&kind, &mut light, LightTy::Natural, &top_voxels);
        neighbor_propagation
            .iter()
            .fold(
                HashMap::<IVec3, Vec<_>>::new(),
                |mut map, &NeighborLightPropagation { dir, voxel, .. }| {
                    map.entry(dir).or_default().push(voxel);
                    map
                },
            )
            .into_iter()
            .for_each(|(dir, voxels)| {
                if dir == voxel::Side::Up.dir() || dir == voxel::Side::Down.dir() {
                    assert_eq!(voxels.len(), chunk::X_AXIS_SIZE * chunk::Z_AXIS_SIZE);
                } else {
                    assert_eq!(voxels.len(), chunk::X_AXIS_SIZE * chunk::Y_AXIS_SIZE);
                }
            });

        neighbor_propagation.into_iter().for_each(
            |NeighborLightPropagation {
                 dir,
                 voxel,
                 ty,
                 intensity,
             }| {
                assert_eq!(
                    ty,
                    LightTy::Natural,
                    "Only natural light should be propagated"
                );
                if dir == voxel::Side::Down.dir() {
                    assert_eq!(
                        intensity,
                        voxel::Light::MAX_NATURAL_INTENSITY,
                        "Downwards propagation should keep max natural intensity"
                    );
                } else {
                    assert_eq!(
                        intensity,
                        voxel::Light::MAX_NATURAL_INTENSITY - 1,
                        "Non-downwards propagation should reduce natural intensity"
                    );
                }
                assert!(
                    chunk::is_at_bounds(voxel),
                    "All voxels propagated should be at boundry"
                );
            },
        );
    }

    #[test]
    fn propagate_to_neighborhood() {
        let mut kind = ChunkStorage::<voxel::Kind>::default();
        let mut light = ChunkStorage::<voxel::Light>::default();

        let top_voxels = (0..=chunk::X_END)
            .flat_map(|x| (0..=chunk::Z_END).map(move |z| IVec3::new(x, chunk::Y_END, z)))
            .filter(|v| v.x != 0)
            .collect::<Vec<_>>();

        top_voxels.iter().for_each(|&voxel| {
            light.set_type(voxel, LightTy::Natural, voxel::Light::MAX_NATURAL_INTENSITY);
        });

        let left_wall = (0..=chunk::Z_END)
            .flat_map(|z| (0..=chunk::Y_END).map(move |y| IVec3::new(0, y, z)))
            .collect::<Vec<_>>();
        left_wall.iter().for_each(|&v| kind.set(v, 1.into()));

        let neighbor_propagation = propagate(&kind, &mut light, LightTy::Natural, &top_voxels);

        neighbor_propagation
            .into_iter()
            .for_each(|NeighborLightPropagation { dir, .. }| {
                assert_ne!(
                    dir,
                    voxel::Side::Left.dir(),
                    "No light should be propagated to left"
                );
            });
    }
}
