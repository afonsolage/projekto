use bevy_app::prelude::*;
use bevy_derive::{Deref, DerefMut};
use bevy_ecs::prelude::*;
use bevy_log::{error, warn};
use bevy_math::prelude::*;
use bevy_utils::HashMap;
use genesis::GeneratedChunk;
use projekto_core::{
    chunk::{self, ChunkStorage},
    voxel::{self, SIDE_COUNT},
};

mod genesis;
mod light;

pub struct WorldServerPlugin;

impl Plugin for WorldServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<ChunkUnload>()
            .add_event::<ChunkLoad>()
            .add_event::<ChunkGen>()
            .add_systems(
                Update,
                (
                    (
                        chunks_unload.run_if(on_event::<ChunkUnload>()),
                        chunks_load.run_if(on_event::<ChunkLoad>()),
                        chunks_gen.run_if(on_event::<ChunkGen>()),
                    )
                        .in_set(WorldSet::ChunkManagement),
                    apply_deferred.in_set(WorldSet::FlushCommands),
                    (
                        update_chunk_neighborhood.run_if(added::<ChunkNeighborhood>),
                        init_light.run_if(added::<ChunkLight>),
                    )
                        .in_set(WorldSet::ChunkInitialization),
                ),
            );
    }
}

#[derive(SystemSet, Debug, Copy, Clone, Hash, PartialEq, Eq)]
enum WorldSet {
    ChunkManagement,
    FlushCommands,
    ChunkInitialization,
}

// Components
#[derive(Component, Default, Debug, Clone, Deref, DerefMut)]
struct ChunkKind(ChunkStorage<voxel::Kind>);

#[derive(Component, Default, Debug, Clone, Deref, DerefMut)]
struct ChunkLight(ChunkStorage<voxel::Light>);

#[derive(Component, Default, Debug, Clone, Copy, Deref, DerefMut)]
struct ChunkLocal(IVec3);

#[derive(Component, Default, Debug, Clone, Deref, DerefMut)]
struct ChunkNeighborhood([Option<Entity>; SIDE_COUNT]);

#[derive(Bundle, Default)]
struct ChunkBundle {
    kind: ChunkKind,
    light: ChunkLight,
    local: ChunkLocal,
    neighborhood: ChunkNeighborhood,
}

#[derive(Resource, Debug, Clone, Deref, DerefMut)]
struct ChunkMap(HashMap<IVec3, Entity>);

#[derive(Event, Debug, Clone, Copy)]
struct ChunkUnload(IVec3);

fn chunks_unload(
    mut commands: Commands,
    mut chunk_map: ResMut<ChunkMap>,
    mut reader: EventReader<ChunkUnload>,
    mut q_neighborhood: Query<&mut ChunkNeighborhood>,
) {
    let removed = reader
        .read()
        .filter_map(|evt| {
            if let Some(entity) = chunk_map.remove(&evt.0) {
                commands.entity(entity).despawn();
                Some(entity)
            } else {
                let local = evt.0;
                warn!("Chunk {local} entity not found.");
                None
            }
        })
        .collect::<Vec<_>>();

    q_neighborhood.iter_mut().for_each(|mut neighborhood| {
        for i in 0..voxel::SIDE_COUNT {
            if let Some(ref e) = neighborhood[i] {
                if removed.contains(e) {
                    neighborhood[i] = None;
                }
            }
        }
    })
}

#[derive(Event, Debug, Clone, Copy)]
struct ChunkLoad(IVec3);

fn chunks_load(mut reader: EventReader<ChunkLoad>, mut writer: EventWriter<ChunkGen>) {
    let locals = reader.read().map(|evt| evt.0).collect::<Vec<_>>();

    // TODO: Include load generated chunks from cache

    locals
        .into_iter()
        .for_each(|local| writer.send(ChunkGen(local)));
}

#[derive(Event, Debug, Clone, Copy)]
struct ChunkGen(IVec3);

fn chunks_gen(
    mut commands: Commands,
    mut reader: EventReader<ChunkGen>,
    mut chunk_map: ResMut<ChunkMap>,
) {
    for &ChunkGen(local) in reader.read() {
        let GeneratedChunk { kind, light } = genesis::generate_chunk(local);
        let entity = commands
            .spawn(ChunkBundle {
                kind: ChunkKind(kind),
                light: ChunkLight(light),
                ..Default::default()
            })
            .id();

        let existing = chunk_map.insert(local, entity).is_none();

        assert!(!existing);
    }
}

fn added<T: Component>(q_added_chunks: Query<(), Added<T>>) -> bool {
    !q_added_chunks.is_empty()
}

fn update_chunk_neighborhood(
    chunk_map: Res<ChunkMap>,
    q_added_chunks: Query<(Entity, &ChunkLocal), Added<ChunkNeighborhood>>,
    mut q_neighborhood: Query<&mut ChunkNeighborhood>,
) {
    q_added_chunks
        .iter()
        .for_each(|(new_chunk, &ChunkLocal(local))| {
            voxel::SIDES
                .iter()
                .filter_map(|side| {
                    let neighbor = side.dir() + local;
                    chunk_map
                        .get(&neighbor)
                        .copied()
                        .map(|neighbor| (side, neighbor))
                })
                .for_each(|(side, neighbor)| {
                    if let Ok(mut neighborhood) = q_neighborhood.get_mut(new_chunk) {
                        neighborhood[side.index()] = Some(neighbor);
                    } else {
                        error!("Unable to find newly added chunk {local}");
                    };

                    if let Ok(mut neighborhood) = q_neighborhood.get_mut(neighbor) {
                        let opposide_idx = side.opposite().index();
                        neighborhood[opposide_idx] = Some(new_chunk);
                    } else {
                        let neighbor_local = local + side.dir();
                        error!("Unable to find newly added chunk neighbor at {neighbor_local}");
                    };
                });
        });
}

fn init_light(mut q: Query<(&ChunkKind, &mut ChunkLight), Added<ChunkLight>>) {
    q.for_each_mut(|(kind, mut light)| {
        let top_voxels = (0..=chunk::X_END)
            .zip(0..chunk::Z_END)
            .map(|(x, z)| IVec3::new(x, chunk::Y_END, z))
            .collect::<Vec<_>>();

        light::propagate(kind, &mut light, voxel::LightTy::Natural, &top_voxels);
    });
}

// TODO: Extract and render to check if its working.
