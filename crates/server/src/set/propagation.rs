use bevy::{platform::collections::HashMap, prelude::*};
use projekto_core::{
    coords::{Chunk, ChunkVoxel},
    voxel::{self},
};

use crate::{
    WorldSet,
    bundle::{CachedMesh, ChunkKind, ChunkLight, ChunkMap, ChunkQuery},
    light::{self, NeighborLightPropagation},
};

pub struct PropagationPlugin;

impl Plugin for PropagationPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<LightUpdate>().add_systems(
            Update,
            (propagate_light.run_if(on_message::<LightUpdate>),).in_set(WorldSet::Propagation),
        );
    }
}

#[derive(Message, Debug, Clone)]
pub struct LightUpdate {
    pub chunk: Chunk,
    pub ty: voxel::LightTy,
    pub values: Vec<(ChunkVoxel, u8)>,
}

fn propagate_light(
    mut commands: Commands,
    mut q_light: ChunkQuery<(&ChunkKind, &mut ChunkLight)>,
    q_cached: Query<&CachedMesh>,
    chunk_map: Res<ChunkMap>,
    mut params: ParamSet<(MessageReader<LightUpdate>, MessageWriter<LightUpdate>)>,
) {
    let mut count = 0;

    let propagate_to_neighbors = params
        .p0()
        .read()
        .fold(
            HashMap::<(Chunk, voxel::LightTy), Vec<ChunkVoxel>>::new(),
            |mut map, LightUpdate { chunk, ty, values }| {
                if let Some((kind, mut light)) = q_light.get_chunk_mut(*chunk) {
                    values.iter().for_each(|&(voxel, intensity)| {
                        if kind.get(voxel).is_opaque() {
                            return;
                        }

                        if intensity > light.get(voxel).get(*ty) {
                            light.set_type(voxel, *ty, intensity);
                            map.entry((*chunk, *ty)).or_default().push(voxel);

                            // A saved mesh was built from the old light, so it can no longer be
                            // treated as a valid archive restore.
                            if let Some(&entity) = chunk_map.get(chunk)
                                && q_cached.contains(entity)
                            {
                                commands.entity(entity).remove::<CachedMesh>();
                            }
                        }
                    });

                    count += 1;
                };

                map
            },
        )
        .into_iter()
        .fold(
            HashMap::<(Chunk, voxel::LightTy), Vec<_>>::new(),
            |mut map, ((chunk, light_ty), voxels)| {
                let (kind, mut light) = q_light
                    .get_chunk_mut(chunk)
                    .expect("Missing entities was filtered already");
                let neighborhood_propagation =
                    light::propagate(kind, &mut light, light_ty, voxels.iter().copied());

                neighborhood_propagation.into_iter().for_each(
                    |NeighborLightPropagation {
                         side,
                         voxel,
                         ty,
                         intensity,
                     }| {
                        let neighbor = chunk.neighbor(side.dir());
                        map.entry((neighbor, ty))
                            .or_default()
                            .push((voxel, intensity));
                    },
                );

                map
            },
        );

    let events = propagate_to_neighbors.len();
    let mut writer = params.p1();
    propagate_to_neighbors
        .into_iter()
        .for_each(|((chunk, ty), values)| {
            writer.write(LightUpdate { chunk, ty, values });
        });

    trace!("[propagate_light] {count} chunks light propagated. {events} propagation events sent.");
}
