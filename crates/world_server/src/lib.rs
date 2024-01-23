use bevy_app::prelude::*;
use bevy_derive::{Deref, DerefMut};
use bevy_ecs::{
    prelude::*,
    query::{QueryItem, ReadOnlyWorldQuery, WorldQuery},
    system::SystemParam,
};
use bevy_log::{error, warn};
use bevy_math::prelude::*;
use bevy_time::common_conditions::on_timer;
use bevy_utils::{Duration, HashMap, HashSet};
use genesis::GeneratedChunk;
use light::NeighborLightPropagation;
use projekto_core::{
    chunk::{self, ChunkStorage},
    voxel::{self, SIDE_COUNT},
};

mod genesis;
mod light;
mod meshing;

pub struct WorldServerPlugin;

impl Plugin for WorldServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<ChunkUnload>()
            .add_event::<ChunkLoad>()
            .add_event::<ChunkGen>()
            .add_event::<LightSet>()
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
                    propagate_light
                        .run_if(on_event::<LightSet>())
                        .in_set(WorldSet::Propagation),
                    (faces_occlusion.run_if(changed::<ChunkKind>),)
                        .in_set(WorldSet::Meshing)
                        .run_if(on_timer(Duration::from_secs_f32(0.5))),
                ),
            );
    }
}

#[derive(SystemSet, Debug, Copy, Clone, Hash, PartialEq, Eq)]
enum WorldSet {
    ChunkManagement,
    FlushCommands,
    ChunkInitialization,
    Propagation,
    Meshing,
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

#[derive(Component, Default, Debug, Clone, Deref, DerefMut)]
struct ChunkFacesOcclusion(ChunkStorage<voxel::FacesOcclusion>);

#[derive(Component, Default, Debug, Clone, Deref, DerefMut)]
struct ChunkFacesSoftLight(ChunkStorage<voxel::FacesSoftLight>);

#[derive(Bundle, Default)]
struct ChunkBundle {
    kind: ChunkKind,
    light: ChunkLight,
    local: ChunkLocal,
    neighborhood: ChunkNeighborhood,
    occlusion: ChunkFacesOcclusion,
    soft_light: ChunkFacesSoftLight,
}

#[derive(Resource, Debug, Clone, Deref, DerefMut)]
struct ChunkMap(HashMap<IVec3, Entity>);

#[derive(SystemParam)]
struct ChunkQuery<'w, 's, Q: WorldQuery + 'static, F: ReadOnlyWorldQuery + 'static = ()> {
    map: Res<'w, ChunkMap>,
    query: Query<'w, 's, Q, F>,
}

impl<'w, 's, Q: WorldQuery + 'static, F: ReadOnlyWorldQuery + 'static> ChunkQuery<'w, 's, Q, F> {
    fn get_chunk_entity(&self, chunk: IVec3) -> Option<Entity> {
        self.map.0.get(&chunk).copied()
    }

    fn get_chunk(&self, chunk: IVec3) -> Option<QueryItem<'_, <Q as WorldQuery>::ReadOnly>> {
        self.map.0.get(&chunk).map(|&entity| {
            self.query
                .get(entity)
                .expect("All entities inside the map must exists")
        })
    }

    fn get_chunk_mut(&mut self, chunk: IVec3) -> Option<Q::Item<'_>> {
        self.map.0.get(&chunk).map(|&entity| {
            self.query
                .get_mut(entity)
                .expect("All entities inside the map must exists")
        })
    }

    fn get_chunk_component<T: Component>(&self, chunk: IVec3) -> Option<&T> {
        if let Some(&entity) = self.map.0.get(&chunk) {
            if let Ok(component) = self.query.get_component::<T>(entity) {
                return Some(component);
            }
        }
        None
    }

    fn get_chunk_component_mut<T: Component>(&mut self, chunk: IVec3) -> Option<Mut<'_, T>> {
        if let Some(&entity) = self.map.0.get(&chunk) {
            if let Ok(component) = self.query.get_component_mut::<T>(entity) {
                return Some(component);
            }
        }
        None
    }
}

impl<'w, 's, Q: WorldQuery + 'static, F: ReadOnlyWorldQuery + 'static> std::ops::Deref
    for ChunkQuery<'w, 's, Q, F>
{
    type Target = Query<'w, 's, Q, F>;

    fn deref(&self) -> &Self::Target {
        &self.query
    }
}

impl<'w, 's, Q: WorldQuery + 'static, F: ReadOnlyWorldQuery + 'static> std::ops::DerefMut
    for ChunkQuery<'w, 's, Q, F>
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.query
    }
}

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

fn changed<T: Component>(q_changed_chunks: Query<(), Changed<T>>) -> bool {
    !q_changed_chunks.is_empty()
}

fn update_chunk_neighborhood(
    q_added_chunks: Query<(Entity, &ChunkLocal), Added<ChunkNeighborhood>>,
    mut q_neighborhood: ChunkQuery<&mut ChunkNeighborhood>,
) {
    q_added_chunks
        .iter()
        .for_each(|(new_chunk, &ChunkLocal(local))| {
            voxel::SIDES.iter().for_each(|side| {
                let neighbor = local + side.dir();

                let neighbor_entity = q_neighborhood.get_chunk_entity(neighbor);
                if let Ok(mut neighborhood) = q_neighborhood.get_mut(new_chunk) {
                    neighborhood[side.index()] = neighbor_entity;
                } else {
                    error!("Unable to find newly added chunk {local}");
                };

                if let Some(mut neighborhood) = q_neighborhood.get_chunk_mut(neighbor) {
                    let opposide_idx = side.opposite().index();
                    neighborhood[opposide_idx] = Some(new_chunk);
                } else {
                    let neighbor_local = local + side.dir();
                    error!("Unable to find newly added chunk neighbor at {neighbor_local}");
                };
            });
        });
}

#[derive(Event, Debug, Clone, Copy)]
struct LightSet {
    chunk: IVec3,
    voxel: IVec3,
    ty: voxel::LightTy,
    intensity: u8,
}

fn init_light(
    mut q: Query<(&ChunkLocal, &ChunkKind, &mut ChunkLight), Added<ChunkLight>>,
    mut writer: EventWriter<LightSet>,
) {
    q.for_each_mut(|(local, kind, mut light)| {
        let top_voxels = (0..=chunk::X_END)
            .zip(0..chunk::Z_END)
            .map(|(x, z)| IVec3::new(x, chunk::Y_END, z))
            .collect::<Vec<_>>();

        let neighbor_propagation =
            light::propagate(kind, &mut light, voxel::LightTy::Natural, &top_voxels);

        neighbor_propagation.into_iter().for_each(
            |NeighborLightPropagation {
                 dir,
                 voxel,
                 ty,
                 intensity,
             }| {
                let chunk = dir + **local;

                writer.send(LightSet {
                    chunk,
                    voxel,
                    ty,
                    intensity,
                });
            },
        );
    });
}

fn propagate_light(
    mut q_light: ChunkQuery<(&ChunkKind, &mut ChunkLight)>,
    mut reader: EventReader<LightSet>,
    mut writer: EventWriter<LightSet>,
) {
    reader
        .read()
        .fold(
            HashMap::<(IVec3, voxel::LightTy), Vec<IVec3>>::new(),
            |mut map,
             &LightSet {
                 chunk,
                 voxel,
                 ty,
                 intensity,
             }| {
                let Some((_, mut light)) = q_light.get_chunk_mut(chunk) else {
                    warn!("Failed to set light on chunk {chunk}. Entity not found on query");
                    return map;
                };

                if intensity > light.get(voxel).get(ty) {
                    light.set_type(voxel, ty, intensity);
                    map.entry((chunk, ty)).or_default().push(voxel);
                }

                map
            },
        )
        .into_iter()
        .for_each(|((chunk, light_ty), voxels)| {
            let (kind, mut light) = q_light
                .get_chunk_mut(chunk)
                .expect("Missing entities was filtered already");
            let neighborhood_propagation = light::propagate(kind, &mut light, light_ty, &voxels);

            neighborhood_propagation.into_iter().for_each(
                |NeighborLightPropagation {
                     dir,
                     voxel,
                     ty,
                     intensity,
                 }| {
                    let neighbor = dir + chunk;

                    writer.send(LightSet {
                        chunk: neighbor,
                        voxel,
                        ty,
                        intensity,
                    });
                },
            );
        });
}

fn faces_occlusion(
    q_changed_chunks: Query<&ChunkLocal, Changed<ChunkKind>>,
    q_kinds: ChunkQuery<&ChunkKind>,
    mut q_occlusions: ChunkQuery<&mut ChunkFacesOcclusion>,
) {
    let mut tmp_faces_occlusion = Default::default();
    q_changed_chunks
        .iter()
        .flat_map(|local| {
            // TODO: There should be a better way to avoid update everything.
            // When a chunk kind is updated, we have to check all its surrounding.
            let neighbors = voxel::SIDES.map(|s| **local + s.dir());
            std::iter::once(**local).chain(neighbors)
        })
        .collect::<HashSet<_>>()
        .into_iter()
        .for_each(|local| {
            let mut neighborhood = [None; voxel::SIDE_COUNT];

            // Update neighborhood
            voxel::SIDES.iter().for_each(|side| {
                let neighbor = local + side.dir();
                neighborhood[side.index()] = q_kinds.get_chunk(neighbor).map(|kind| &**kind);
            });

            let kind = q_kinds.get_chunk(local).expect("Entity exists");
            meshing::faces_occlusion(kind, &mut tmp_faces_occlusion, &neighborhood);

            let mut faces_occlusion = q_occlusions.get_chunk_mut(local).expect("Entity exists");

            // Avoid triggering change detection when the value didn't changed.
            if !tmp_faces_occlusion.eq(&faces_occlusion) {
                faces_occlusion.copy_from(&tmp_faces_occlusion);
            }
        });
}

fn faces_light_softening(
    q_changed_chunks: Query<&ChunkLocal, Or<(Changed<ChunkKind>, Changed<ChunkLight>)>>,
    q_chunks: ChunkQuery<(&ChunkLocal, &ChunkKind, &ChunkLight, &ChunkFacesOcclusion)>,
    mut q_soft_light: ChunkQuery<(&mut ChunkFacesSoftLight,)>,
) {
    q_changed_chunks
        .iter()
        .flat_map(|local| {
            // TODO: There should be a better way to avoid update everything.
            // When a chunk kind is updated, we have to check all its surrounding.
            let neighbors = voxel::SIDES.map(|s| **local + s.dir());
            std::iter::once(**local).chain(neighbors)
        })
        .collect::<HashSet<_>>()
        .into_iter()
        .for_each(|chunk| {
            let mut soft_light = ChunkStorage::<voxel::FacesSoftLight>::default();

            let occlusion = &**q_chunks
                .get_chunk_component::<ChunkFacesOcclusion>(chunk)
                .expect("Chunk must exists");

            light::smooth_lighting(
                chunk,
                occlusion,
                &mut soft_light,
                |chunk| {
                    q_chunks
                        .get_chunk_component::<ChunkKind>(chunk)
                        .map(|c| &**c)
                },
                |chunk| {
                    q_chunks
                        .get_chunk_component::<ChunkLight>(chunk)
                        .map(|c| &**c)
                },
            );

            //
        });
}

// TODO: Extract and render to check if its working.
