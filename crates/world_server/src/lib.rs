use std::time::Duration;

use bevy::{
    ecs::{
        query::{QueryItem, ReadOnlyWorldQuery, WorldQuery},
        system::SystemParam,
    },
    prelude::*,
    time::common_conditions::on_timer,
    utils::{HashMap, HashSet},
};
use genesis::GeneratedChunk;
use light::NeighborLightPropagation;
use projekto_core::{
    chunk::{self, ChunkStorage},
    voxel::{self},
};

mod genesis;
mod light;
mod meshing;

pub struct WorldServerPlugin;

impl Plugin for WorldServerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChunkMap>()
            .init_resource::<LandscapeCenter>()
            .add_event::<ChunkUnload>()
            .add_event::<ChunkLoad>()
            .add_event::<ChunkGen>()
            .add_event::<LightUpdate>()
            .add_systems(
                Update,
                (
                    update_landscape.run_if(resource_changed::<LandscapeCenter>()),
                    // Chunk Management
                    (
                        chunks_unload.run_if(on_event::<ChunkUnload>()),
                        chunks_load.run_if(on_event::<ChunkLoad>()),
                        chunks_gen.run_if(on_event::<ChunkGen>()),
                    )
                        .chain()
                        .in_set(WorldSet::ChunkManagement),
                    apply_deferred.in_set(WorldSet::FlushCommands),
                    // Chunk Initialization
                    (init_light.run_if(any_chunk::<Added<ChunkLight>>),)
                        .in_set(WorldSet::ChunkInitialization),
                    // Chunk Propagation
                    (propagate_light.run_if(on_event::<LightUpdate>()),)
                        .in_set(WorldSet::Propagation),
                    // Meshing
                    (
                        faces_occlusion.run_if(any_chunk::<Changed<ChunkKind>>),
                        faces_light_softening
                            .run_if(any_chunk::<Or<(Changed<ChunkKind>, Changed<ChunkLight>)>>),
                        generate_vertices
                            .run_if(any_chunk::<Or<(Changed<ChunkKind>, Changed<ChunkLight>)>>),
                    )
                        .chain()
                        .in_set(WorldSet::Meshing)
                        .run_if(on_timer(Duration::from_secs_f32(0.5))),
                )
                    .chain(),
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
struct ChunkFacesOcclusion(ChunkStorage<voxel::FacesOcclusion>);

#[derive(Component, Default, Debug, Clone, Deref, DerefMut)]
struct ChunkFacesSoftLight(ChunkStorage<voxel::FacesSoftLight>);

#[derive(Component, Default, Debug, Clone, Deref, DerefMut)]
struct ChunkVertex(Vec<voxel::Vertex>);

#[derive(Bundle, Default)]
struct ChunkBundle {
    kind: ChunkKind,
    light: ChunkLight,
    local: ChunkLocal,
    occlusion: ChunkFacesOcclusion,
    soft_light: ChunkFacesSoftLight,
    vertex: ChunkVertex,
}

#[derive(Resource, Default, Debug, Clone, Copy)]
struct LandscapeCenter(IVec3);

#[derive(Resource, Default, Debug, Clone, Deref, DerefMut)]
struct ChunkMap(HashMap<IVec3, Entity>);

#[derive(SystemParam)]
struct ChunkQuery<'w, 's, Q: WorldQuery + 'static, F: ReadOnlyWorldQuery + 'static = ()> {
    map: Res<'w, ChunkMap>,
    query: Query<'w, 's, Q, F>,
}

impl<'w, 's, Q: WorldQuery + 'static, F: ReadOnlyWorldQuery + 'static> ChunkQuery<'w, 's, Q, F> {
    // fn get_chunk_entity(&self, chunk: IVec3) -> Option<Entity> {
    //     self.map.0.get(&chunk).copied()
    // }

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

    // fn get_chunk_component_mut<T: Component>(&mut self, chunk: IVec3) -> Option<Mut<'_, T>> {
    //     if let Some(&entity) = self.map.0.get(&chunk) {
    //         if let Ok(component) = self.query.get_component_mut::<T>(entity) {
    //             return Some(component);
    //         }
    //     }
    //     None
    // }
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

#[derive(Event, Debug, Clone, Copy)]
struct ChunkLoad(IVec3);

#[derive(Event, Debug, Clone, Copy)]
struct ChunkGen(IVec3);

#[derive(Event, Debug, Clone, Copy)]
struct LightUpdate {
    chunk: IVec3,
    voxel: IVec3,
    ty: voxel::LightTy,
    intensity: u8,
}

fn update_landscape(center: Res<LandscapeCenter>) {
    // TODO: Load and unload chunks based on landscape position.
}

fn chunks_unload(
    mut commands: Commands,
    mut chunk_map: ResMut<ChunkMap>,
    mut reader: EventReader<ChunkUnload>,
) {
    reader.read().for_each(|evt| {
        if let Some(entity) = chunk_map.remove(&evt.0) {
            commands.entity(entity).despawn();
        } else {
            let local = evt.0;
            warn!("Chunk {local} entity not found.");
        }
    });
}

fn chunks_load(mut reader: EventReader<ChunkLoad>, mut writer: EventWriter<ChunkGen>) {
    let locals = reader.read().map(|evt| evt.0).collect::<Vec<_>>();

    // TODO: Include load generated chunks from cache

    locals
        .into_iter()
        .for_each(|local| writer.send(ChunkGen(local)));
}

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

        let existing = chunk_map.insert(local, entity);

        assert_eq!(existing, None);
    }
}

fn any_chunk<T: ReadOnlyWorldQuery>(q_changed_chunks: Query<(), (T, With<ChunkLocal>)>) -> bool {
    !q_changed_chunks.is_empty()
}

fn init_light(
    mut q: Query<(&ChunkLocal, &ChunkKind, &mut ChunkLight), Added<ChunkLight>>,
    mut writer: EventWriter<LightUpdate>,
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

                writer.send(LightUpdate {
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
    mut params: ParamSet<(EventReader<LightUpdate>, EventWriter<LightUpdate>)>,
) {
    let events = params.p0().read().copied().collect::<Vec<_>>();
    let mut writer = params.p1();

    events
        .into_iter()
        .fold(
            HashMap::<(IVec3, voxel::LightTy), Vec<IVec3>>::new(),
            |mut map,
             LightUpdate {
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

                    writer.send(LightUpdate {
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

            let mut faces_occlusion = q_occlusions.get_chunk_mut(local).expect("Entity exists");
            let kind = q_kinds.get_chunk(local).expect("Entity exists");
            meshing::faces_occlusion(kind, &mut faces_occlusion, &neighborhood);
        });
}

#[allow(clippy::type_complexity)]
fn faces_light_softening(
    q_changed_chunks: Query<&ChunkLocal, Or<(Changed<ChunkKind>, Changed<ChunkLight>)>>,
    q_chunks: ChunkQuery<(&ChunkLocal, &ChunkKind, &ChunkLight, &ChunkFacesOcclusion)>,
    mut q_soft_light: ChunkQuery<&mut ChunkFacesSoftLight>,
) {
    q_changed_chunks
        .iter()
        .flat_map(|local| {
            // TODO: There should be a better way to avoid update everything.
            // When a chunk kind or light is updated, we have to check all its surrounding.
            let neighbors = voxel::SIDES.map(|s| **local + s.dir());
            std::iter::once(**local).chain(neighbors)
        })
        .collect::<HashSet<_>>()
        .into_iter()
        .for_each(|chunk| {
            let occlusion = &**q_chunks
                .get_chunk_component::<ChunkFacesOcclusion>(chunk)
                .expect("Chunk must exists");

            let mut soft_light = q_soft_light
                .get_chunk_mut(chunk)
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
        });
}

#[allow(clippy::type_complexity)]
fn generate_vertices(
    q_changed_chunks: Query<
        (
            Entity,
            &ChunkKind,
            &ChunkFacesOcclusion,
            &ChunkFacesSoftLight,
        ),
        Or<(Changed<ChunkKind>, Changed<ChunkLight>)>,
    >,
    mut q_vertex: Query<&mut ChunkVertex>,
) {
    q_changed_chunks
        .iter()
        .for_each(|(entity, kind, faces_occlusion, faces_soft_light)| {
            if faces_occlusion.is_fully_occluded() {
                return;
            }

            let faces = meshing::faces_merge(kind, faces_occlusion, faces_soft_light);
            let mut vertex = meshing::generate_vertices(faces);

            let mut chunk_vertex = q_vertex.get_mut(entity).expect("Entity must exists");
            std::mem::swap(&mut vertex, &mut chunk_vertex);
        });
}

#[cfg(test)]
mod test {
    use bevy::app::ScheduleRunnerPlugin;

    use super::*;

    #[test]
    fn plugin() {
        App::new()
            .add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_once()))
            .add_plugins(super::WorldServerPlugin)
            .run()
    }

    #[test]
    fn chunk_load() {
        // arrange
        let mut app = App::new();

        app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_once()))
            .add_plugins(super::WorldServerPlugin);

        app.world.send_event(ChunkLoad((0, 0, 0).into()));

        // act
        app.update();

        // assert
        assert_eq!(
            app.world.entities().len(),
            1,
            "One entity should be spawned"
        );
        assert_eq!(
            app.world.get_resource::<ChunkMap>().unwrap().len(),
            1,
            "One entity should be inserted on map"
        );
    }

    #[test]
    fn chunk_gen() {
        // arrange
        let mut app = App::new();

        app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_once()))
            .add_plugins(super::WorldServerPlugin);

        app.world.send_event(ChunkLoad((0, 0, 0).into()));

        // act
        app.update();

        // assert
        let (kind, light) = app
            .world
            .query::<(&ChunkKind, &ChunkLight)>()
            .get_single(&app.world)
            .unwrap();

        for x in 0..=chunk::X_END {
            for z in 0..=chunk::Z_END {
                assert_eq!(
                    light
                        .get((x, chunk::Y_END, z).into())
                        .get(voxel::LightTy::Natural),
                    voxel::Light::MAX_NATURAL_INTENSITY,
                    "All y-most voxels should have max natural light"
                );
            }
        }

        chunk::voxels().for_each(|voxel| {
            if kind.get(voxel).is_opaque() {
                assert_eq!(
                    light.get(voxel).get_greater_intensity(),
                    0,
                    "Opaque voxels should have no light value"
                );
            }
        });
    }
}

// TODO: Extract and render to check if its working.
