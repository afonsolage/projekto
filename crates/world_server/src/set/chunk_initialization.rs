use bevy::{prelude::*, utils::HashMap};
use projekto_core::{
    chunk::{self},
    voxel::{self},
};

use crate::{
    any_chunk,
    light::{self, NeighborLightPropagation},
    ChunkKind, ChunkLight, ChunkLocal, LightUpdate, WorldSet,
};

pub struct ChunkInitializationPlugin;

impl Plugin for ChunkInitializationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (init_light
                .run_if(any_chunk::<Added<ChunkLight>>)
                .in_set(WorldSet::ChunkInitialization),),
        );
    }
}

fn init_light(
    mut q: Query<(&ChunkLocal, &ChunkKind, &mut ChunkLight), Added<ChunkLight>>,
    mut writer: EventWriter<LightUpdate>,
) {
    let mut map = HashMap::new();
    let mut count = 0;

    q.for_each_mut(|(local, kind, mut light)| {
        chunk::top_voxels().for_each(|voxel| {
            light.set_type(
                voxel,
                voxel::LightTy::Natural,
                voxel::Light::MAX_NATURAL_INTENSITY,
            );
        });

        let neighbor_propagation = light::propagate(
            kind,
            &mut light,
            voxel::LightTy::Natural,
            chunk::top_voxels(),
        );

        neighbor_propagation.into_iter().for_each(
            |NeighborLightPropagation {
                 side,
                 voxel,
                 ty,
                 intensity,
             }| {
                let chunk = local.neighbor(side.dir());
                map.entry((chunk, ty))
                    .or_insert(vec![])
                    .push((voxel, intensity));
            },
        );

        count += 1;
    });

    let events = map.len();

    map.into_iter().for_each(|((chunk, ty), values)| {
        writer.send(LightUpdate { chunk, ty, values });
    });

    trace!("[init_light] {count} chunks light initialized. {events} propagation events sent.");
}
