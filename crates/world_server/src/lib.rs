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
use chunk::ChunkStorage;
use genesis::GeneratedChunk;
use light::NeighborLightPropagation;
use projekto_core::voxel::{self};

pub mod chunk;
mod genesis;
mod light;
mod meshing;

pub struct WorldServerPlugin;

impl Plugin for WorldServerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChunkMap>()
            .add_event::<ChunkUnload>()
            .add_event::<ChunkLoad>()
            .add_event::<ChunkGen>()
            .add_event::<LightUpdate>()
            .add_systems(
                FixedUpdate,
                (
                    // Landscape Update
                    (update_landscape.run_if(resource_changed_or_removed::<Landscape>()),)
                        .in_set(WorldSet::LandscapeUpdate)
                        .before(WorldSet::ChunkManagement),
                    // Chunk Management
                    (
                        chunks_unload.run_if(on_event::<ChunkUnload>()),
                        chunks_load.run_if(on_event::<ChunkLoad>()),
                        chunks_gen.run_if(on_event::<ChunkGen>()),
                    )
                        .chain()
                        .in_set(WorldSet::ChunkManagement)
                        .before(WorldSet::FlushCommands),
                    apply_deferred.in_set(WorldSet::FlushCommands),
                    // Chunk Initialization
                    (init_light.run_if(any_chunk::<Added<ChunkLight>>),)
                        .in_set(WorldSet::ChunkInitialization)
                        .after(WorldSet::FlushCommands),
                    // Chunk Propagation
                    (propagate_light.run_if(on_event::<LightUpdate>()),)
                        .in_set(WorldSet::Propagation)
                        .after(WorldSet::ChunkInitialization),
                    // Meshing
                    (
                        faces_occlusion, //.run_if(any_chunk::<Changed<ChunkKind>>),
                        faces_light_softening,
                        // .run_if(any_chunk::<Or<(Changed<ChunkKind>, Changed<ChunkLight>)>>),
                        generate_vertices,
                        // .run_if(any_chunk::<Or<(Changed<ChunkKind>, Changed<ChunkLight>)>>),
                    )
                        .chain()
                        .in_set(WorldSet::Meshing)
                        .after(WorldSet::Propagation)
                        .run_if(on_timer(Duration::from_secs_f32(0.5))),
                ),
            );
    }
}

#[derive(SystemSet, Debug, Copy, Clone, Hash, PartialEq, Eq)]
enum WorldSet {
    LandscapeUpdate,
    ChunkManagement,
    FlushCommands,
    ChunkInitialization,
    Propagation,
    Meshing,
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Chunk(IVec2);

impl Chunk {
    pub fn new(x: i32, z: i32) -> Self {
        Self(IVec2::new(x, z))
    }

    pub fn neighbor(&self, dir: IVec2) -> Self {
        Chunk(self.0 + dir)
    }
}

impl From<IVec2> for Chunk {
    fn from(value: IVec2) -> Self {
        Self(value)
    }
}

impl From<(i32, i32)> for Chunk {
    fn from(value: (i32, i32)) -> Self {
        Self(value.into())
    }
}

impl From<Chunk> for Vec3 {
    fn from(value: Chunk) -> Self {
        chunk::to_world(value)
    }
}

impl std::fmt::Display for Chunk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

pub type Voxel = IVec3;

// Components
#[derive(Component, Default, Debug, Clone, Deref, DerefMut)]
struct ChunkKind(ChunkStorage<voxel::Kind>);

#[derive(Component, Default, Debug, Clone, Deref, DerefMut)]
struct ChunkLight(ChunkStorage<voxel::Light>);

#[derive(Component, Default, Debug, Clone, Copy, Deref, DerefMut)]
pub struct ChunkLocal(Chunk);

#[derive(Component, Default, Debug, Clone, Deref, DerefMut)]
struct ChunkFacesOcclusion(ChunkStorage<voxel::FacesOcclusion>);

#[derive(Component, Default, Debug, Clone, Deref, DerefMut)]
struct ChunkFacesSoftLight(ChunkStorage<voxel::FacesSoftLight>);

#[derive(Component, Default, Debug, Clone, Deref, DerefMut)]
pub struct ChunkVertex(Vec<voxel::Vertex>);

#[derive(Bundle, Default)]
struct ChunkBundle {
    kind: ChunkKind,
    light: ChunkLight,
    local: ChunkLocal,
    occlusion: ChunkFacesOcclusion,
    soft_light: ChunkFacesSoftLight,
    vertex: ChunkVertex,
}

#[derive(Resource, Debug, Clone, Copy)]
pub struct Landscape {
    pub center: IVec2,
    pub radius: u8,
}

impl Default for Landscape {
    fn default() -> Self {
        Self {
            center: Default::default(),
            radius: 1,
        }
    }
}

#[derive(Resource, Default, Debug, Clone, Deref, DerefMut)]
struct ChunkMap(HashMap<Chunk, Entity>);

#[derive(SystemParam)]
struct ChunkQuery<'w, 's, Q: WorldQuery + 'static, F: ReadOnlyWorldQuery + 'static = ()> {
    map: Res<'w, ChunkMap>,
    query: Query<'w, 's, Q, F>,
}

impl<'w, 's, Q: WorldQuery + 'static, F: ReadOnlyWorldQuery + 'static> ChunkQuery<'w, 's, Q, F> {
    // fn get_chunk_entity(&self, chunk: IVec3) -> Option<Entity> {
    //     self.map.0.get(&chunk).copied()
    // }

    fn get_chunk(&self, chunk: Chunk) -> Option<QueryItem<'_, <Q as WorldQuery>::ReadOnly>> {
        self.map.0.get(&chunk).map(|&entity| {
            self.query
                .get(entity)
                .expect("All entities inside the map must exists")
        })
    }

    fn get_chunk_mut(&mut self, chunk: Chunk) -> Option<Q::Item<'_>> {
        self.map.0.get(&chunk).map(|&entity| {
            self.query
                .get_mut(entity)
                .expect("All entities inside the map must exists")
        })
    }

    fn get_chunk_component<T: Component>(&self, chunk: Chunk) -> Option<&T> {
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
struct ChunkUnload(Chunk);

#[derive(Event, Debug, Clone, Copy)]
struct ChunkLoad(Chunk);

#[derive(Event, Debug, Clone, Copy)]
struct ChunkGen(Chunk);

#[derive(Event, Debug, Clone)]
struct LightUpdate {
    chunk: Chunk,
    ty: voxel::LightTy,
    values: Vec<(Voxel, u8)>,
}

fn update_landscape(
    maybe_landscape: Option<Res<Landscape>>,
    chunk_map: Res<ChunkMap>,
    mut load_writer: EventWriter<ChunkLoad>,
    mut unload_writer: EventWriter<ChunkUnload>,
) {
    let new_landscape_chunks = {
        if let Some(landscape) = maybe_landscape {
            let radius = landscape.radius as i32;
            let center = landscape.center;
            (-radius..=radius)
                .flat_map(|x| {
                    (-radius..=radius).map(move |z| Chunk::new(x + center.x, z + center.y))
                })
                .collect::<HashSet<_>>()
        } else {
            HashSet::new()
        }
    };

    let mut unloaded = 0;
    chunk_map
        .keys()
        .filter(|&c| !new_landscape_chunks.contains(c))
        .for_each(|&c| {
            unload_writer.send(ChunkUnload(c));
            unloaded += 1;
        });

    let mut loaded = 0;
    new_landscape_chunks
        .into_iter()
        .filter(|c| !chunk_map.contains_key(c))
        .for_each(|c| {
            load_writer.send(ChunkLoad(c));
            loaded += 1
        });

    trace!("[update_landscape] Unloaded: {unloaded}, loaded: {loaded}");
}

fn chunks_unload(
    mut commands: Commands,
    mut chunk_map: ResMut<ChunkMap>,
    mut reader: EventReader<ChunkUnload>,
) {
    let mut count = 0;
    reader.read().for_each(|evt| {
        if let Some(entity) = chunk_map.remove(&evt.0) {
            commands.entity(entity).despawn();
            count += 1;
        } else {
            let local = evt.0;
            warn!("Chunk {local} entity not found.");
        }
    });
    trace!("[chunks_unload] {count} chunks despawned");
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
    let mut count = 0;
    for &ChunkGen(chunk) in reader.read() {
        let GeneratedChunk { kind, light } = genesis::generate_chunk(chunk);
        let entity = commands
            .spawn(ChunkBundle {
                kind: ChunkKind(kind),
                light: ChunkLight(light),
                ..Default::default()
            })
            .insert(Name::new(format!("Server Chunk {chunk}")))
            .id();

        let existing = chunk_map.insert(chunk, entity);
        debug_assert_eq!(existing, None, "Can't replace existing chunk {chunk}");
        count += 1;
    }
    trace!("[chunks_gen] {count} chunks generated and spawned.");
}

fn any_chunk<T: ReadOnlyWorldQuery>(q_changed_chunks: Query<(), (T, With<ChunkLocal>)>) -> bool {
    !q_changed_chunks.is_empty()
}

fn init_light(
    mut q: Query<(&ChunkLocal, &ChunkKind, &mut ChunkLight), Added<ChunkLight>>,
    mut writer: EventWriter<LightUpdate>,
) {
    let mut map = HashMap::new();
    let mut count = 0;

    q.for_each_mut(|(local, kind, mut light)| {
        let top_voxels = (0..=chunk::X_END)
            .zip(0..chunk::Z_END)
            .map(|(x, z)| Voxel::new(x, chunk::Y_END, z))
            .collect::<Vec<_>>();

        let neighbor_propagation =
            light::propagate(kind, &mut light, voxel::LightTy::Natural, &top_voxels);

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
                let Some((_, mut light)) = q_light.get_chunk_mut(*chunk) else {
                    warn!("Failed to set light on chunk {chunk}. Entity not found on query");
                    return map;
                };

                values.iter().for_each(|&(voxel, intensity)| {
                    if intensity > light.get(voxel).get(*ty) {
                        light.set_type(voxel, *ty, intensity);
                        map.entry((*chunk, *ty)).or_default().push(voxel);
                    }
                });

                count += 1;

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
                    light::propagate(kind, &mut light, light_ty, &voxels);

                neighborhood_propagation.into_iter().for_each(
                    |NeighborLightPropagation {
                         side,
                         voxel,
                         ty,
                         intensity,
                     }| {
                        let neighbor = chunk.neighbor(side.dir());
                        map.entry((neighbor, ty))
                            .or_insert(vec![])
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
        .for_each(|((chunk, ty), values)| writer.send(LightUpdate { chunk, ty, values }));

    trace!("[propagate_light] {count} chunks light propagated. {events} propagation events sent.");
}

fn faces_occlusion(
    q_changed_chunks: Query<&ChunkLocal, Changed<ChunkKind>>,
    q_kinds: ChunkQuery<&ChunkKind>,
    mut q_occlusions: ChunkQuery<&mut ChunkFacesOcclusion>,
) {
    let mut count = 0;
    let mut fully_occluded = 0;

    q_changed_chunks
        .iter()
        .flat_map(|local| {
            // TODO: There should be a better way to avoid update everything.
            // When a chunk kind is updated, we have to check all its surrounding.
            let neighbors = chunk::SIDES.map(|s| local.neighbor(s.dir()));
            std::iter::once(**local).chain(neighbors)
        })
        .collect::<HashSet<_>>()
        .into_iter()
        .for_each(|chunk| {
            let mut neighborhood = [None; chunk::SIDE_COUNT];

            // Update neighborhood
            chunk::SIDES.iter().for_each(|side| {
                let neighbor = chunk.neighbor(side.dir());
                neighborhood[side.index()] = q_kinds.get_chunk(neighbor).map(|kind| &**kind);
            });

            let mut faces_occlusion = q_occlusions.get_chunk_mut(chunk).expect("Entity exists");
            let kind = q_kinds.get_chunk(chunk).expect("Entity exists");
            meshing::faces_occlusion(kind, &mut faces_occlusion, &neighborhood);

            if faces_occlusion.iter().all(|occ| occ.is_fully_occluded()) {
                fully_occluded += 1;
            }
            count += 1;
        });

    if count > 0 {
        trace!("[faces_occlusion] {count} chunks faces occlusion computed. {fully_occluded} chunks fully occluded.");
    }
}

#[allow(clippy::type_complexity)]
fn faces_light_softening(
    q_changed_chunks: Query<&ChunkLocal, Or<(Changed<ChunkKind>, Changed<ChunkLight>)>>,
    q_chunks: ChunkQuery<(&ChunkLocal, &ChunkKind, &ChunkLight, &ChunkFacesOcclusion)>,
    mut q_soft_light: ChunkQuery<&mut ChunkFacesSoftLight>,
) {
    let mut count = 0;

    q_changed_chunks
        .iter()
        .flat_map(|local| {
            // TODO: There should be a better way to avoid update everything.
            // When a chunk kind or light is updated, we have to check all its surrounding.
            let neighbors = chunk::SIDES.map(|s| local.neighbor(s.dir()));
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

            count += 1;
        });

    if count > 0 {
        trace!("[faces_light_softening] {count} chunks faces light softened.");
    }
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
    let mut count = 0;
    q_changed_chunks
        .iter()
        .for_each(|(entity, kind, faces_occlusion, faces_soft_light)| {
            if faces_occlusion.iter().all(|occ| occ.is_fully_occluded()) {
                return;
            }

            let faces = meshing::faces_merge(kind, faces_occlusion, faces_soft_light);
            let mut vertex = meshing::generate_vertices(faces);

            let mut chunk_vertex = q_vertex.get_mut(entity).expect("Entity must exists");
            std::mem::swap(&mut vertex, &mut chunk_vertex);

            count += 1;
        });

    if count > 0 {
        trace!("[generate_vertices] {count} chunks vertices generated.");
    }
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
    fn update_no_landscape() {
        // arrange
        let mut app = App::new();

        app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_once()))
            .add_plugins(super::WorldServerPlugin);

        // act
        app.update();

        // assert
        assert!(
            app.world.entities().is_empty(),
            "No entity should be spawned"
        );
    }

    #[test]
    fn update_landscape_radius_remove() {
        // arrange
        let mut app = App::new();

        app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_once()))
            .add_plugins(super::WorldServerPlugin);

        app.world.insert_resource(Landscape {
            radius: 1,
            ..Default::default()
        });

        app.update();
        app.world.remove_resource::<Landscape>();

        // act
        app.update();

        // assert
        assert!(
            app.world.entities().is_empty(),
            "All chunks should be removed and landscape is removed"
        );
    }

    #[test]
    fn update_landscape_radius_1() {
        // arrange
        let mut app = App::new();

        app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_once()))
            .add_plugins(super::WorldServerPlugin);

        app.world.insert_resource(Landscape {
            radius: 1,
            ..Default::default()
        });

        // act
        app.update();

        // assert
        assert_eq!(
            app.world.entities().len(),
            9,
            "There should be 9 entities in a landscape with radius 1"
        );
    }

    #[test]
    fn update_landscape_zero() {
        // arrange
        let mut app = App::new();

        app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_once()))
            .add_plugins(super::WorldServerPlugin);

        app.world.insert_resource(Landscape {
            radius: 0,
            ..Default::default()
        });

        // act
        app.update();

        // assert
        assert_eq!(
            app.world.entities().len(),
            1,
            "Only a single chunk should be spawned in a landscape with 0 radius"
        );
    }

    #[test]
    fn chunk_load() {
        // arrange
        let mut app = App::new();

        app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_once()))
            .add_plugins(super::WorldServerPlugin);

        app.world.send_event(ChunkLoad((0, 0).into()));

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

        app.world.send_event(ChunkLoad((0, 0).into()));

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
