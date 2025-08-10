use bevy::{platform::collections::HashMap, prelude::*};
use projekto_core::{
    chunk::Chunk,
    voxel::{self, Voxel},
};

use crate::{
    WorldSet,
    bundle::{ChunkKind, ChunkLight, ChunkQuery},
    light::{self, NeighborLightPropagation},
};

pub struct PropagationPlugin;

impl Plugin for PropagationPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<LightUpdate>().add_systems(
            Update,
            (propagate_light.run_if(on_event::<LightUpdate>),).in_set(WorldSet::Propagation),
        );
    }
}

#[derive(Event, Debug, Clone)]
pub struct LightUpdate {
    pub chunk: Chunk,
    pub ty: voxel::LightTy,
    pub values: Vec<(Voxel, u8)>,
}

fn propagate_light(
    mut q_light: ChunkQuery<(&ChunkKind, &mut ChunkLight)>,
    mut params: ParamSet<(EventReader<LightUpdate>, EventWriter<LightUpdate>)>,
) {
    let mut count = 0;

    let propagate_to_neighbors = params
        .p0()
        .read()
        .fold(
            HashMap::<(Chunk, voxel::LightTy), Vec<Voxel>>::new(),
            |mut map, LightUpdate { chunk, ty, values }| {
                if let Some((_, mut light)) = q_light.get_chunk_mut(*chunk) {
                    values.iter().for_each(|&(voxel, intensity)| {
                        if intensity > light.get(voxel).get(*ty) {
                            light.set_type(voxel, *ty, intensity);
                            map.entry((*chunk, *ty)).or_default().push(voxel);
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
