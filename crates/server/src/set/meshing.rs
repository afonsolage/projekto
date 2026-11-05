use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use crate::{
    DerivedCaches, DirtyChunks, WorldSet, adjacent_chunks, affected_chunks,
    bundle::{
        CachedMesh, ChunkKind, ChunkLight, ChunkLocal, ChunkLod, ChunkMap, ChunkQuery, ChunkVertex,
    },
    cache::sort_nearest_first,
    light, meshing,
};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on};
use projekto_core::{
    chunk::{self, ChunkStorage},
    coords::Chunk,
    voxel,
};

use super::{Landscape, TerrainStats};

pub struct MeshingPlugin;

impl Plugin for MeshingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DerivedCaches>()
            .init_resource::<DirtyChunks>()
            .init_resource::<MeshTasks>()
            .init_resource::<TerrainStats>()
            .add_systems(
                Update,
                (update_chunk_lods, mark_dirty_chunks, generate_vertices)
                    .chain()
                    .in_set(WorldSet::Meshing),
            );
    }
}

/// Maximum number of meshes generated concurrently on worker threads.
pub const MESH_MAX_IN_FLIGHT: usize = 32;

/// Maximum number of worker tasks admitted per update.
pub const MESH_MAX_SPAWNS: usize = 16;

/// In-flight mesh generation tasks, keyed by the chunk each one meshes.
///
/// Meshing runs on [`AsyncComputeTaskPool`], so this resource is the only link between a worker
/// and the ECS: [`generate_vertices`] polls it, applies finished results and drops stale ones.
/// Tasks are bounded so a large batch of dirty chunks cannot saturate the pool.
#[derive(Resource, Default)]
pub struct MeshTasks {
    in_flight: HashMap<Chunk, Task<FullMeshOutput>>,
}

impl MeshTasks {
    /// Returns `true` while a mesh for `chunk` is being generated.
    pub fn is_pending(&self, chunk: Chunk) -> bool {
        self.in_flight.contains_key(&chunk)
    }

    /// Iterates over the chunks with a mesh in flight.
    pub fn pending(&self) -> impl Iterator<Item = Chunk> + '_ {
        self.in_flight.keys().copied()
    }

    /// Cancels the task for `chunk`, returning `true` if there was one.
    pub fn cancel(&mut self, chunk: Chunk) -> bool {
        self.in_flight.remove(&chunk).is_some()
    }
}

/// Mesh generated on a worker thread, waiting to be applied by [`generate_vertices`].
struct FullMeshOutput {
    lod: u8,
    vertex: Vec<voxel::Vertex>,
    occlusion: ChunkStorage<voxel::FacesOcclusion>,
    soft_light: ChunkStorage<voxel::FacesSoftLight>,
    elapsed: Duration,
}

/// Chunks inside this square keep full-resolution meshes; chunks outside use a coarser LOD.
pub const LOD_FULL_RADIUS: i32 = 16;

/// Extra distance, in chunks, that a full-detail chunk keeps full detail before it is demoted.
///
/// This prevents a chunk from being remeshed to coarse right before it leaves the landscape.
pub const LOD_HYSTERESIS: i32 = 2;

/// Maximum number of LOD transitions applied per update, to avoid a visibility dip.
pub const LOD_UPDATES_PER_FRAME: usize = 8;

/// Returns the LOD a chunk should use when it has no [`ChunkLod`] yet, or when nothing constrains
/// it: full detail (LOD 0) inside the [`LOD_FULL_RADIUS`] square around `center`, coarse (LOD 1)
/// outside it.
///
/// The coarse mesher supports any LOD level ([`meshing::lod::LodScale`] grows the horizontal cell
/// with the level), but streaming only distinguishes near from far, so this returns either 0 or
/// 1. More rings would need a per-ring radius table here and in [`next_lod`].
pub(crate) fn desired_lod(chunk: Chunk, center: bevy::math::IVec2) -> u8 {
    let dx = (chunk.x - center.x).abs();
    let dz = (chunk.z - center.y).abs();

    if dx <= LOD_FULL_RADIUS && dz <= LOD_FULL_RADIUS {
        0
    } else {
        1
    }
}

/// Returns the LOD `chunk` should transition to, given its `current` LOD.
///
/// Same two-level split as [`desired_lod`], plus hysteresis: a chunk already at LOD 0 keeps full
/// detail until it is [`LOD_HYSTERESIS`] chunks past the full radius. Without that band a chunk
/// oscillating around the boundary would be remeshed on every move, and a chunk about to leave
/// the landscape would be remeshed to coarse right before being unloaded. Demotion to LOD 1 is
/// deliberately not sticky: a coarse chunk inside the full radius is promoted immediately.
pub fn next_lod(current: u8, chunk: Chunk, center: bevy::math::IVec2) -> u8 {
    let margin = if current == 0 { LOD_HYSTERESIS } else { 0 };
    let dx = (chunk.x - center.x).abs();
    let dz = (chunk.z - center.y).abs();

    if dx <= LOD_FULL_RADIUS + margin && dz <= LOD_FULL_RADIUS + margin {
        0
    } else {
        1
    }
}

/// Applies LOD transitions for chunks whose [`ChunkLod`] no longer matches [`next_lod`].
///
/// Every transition changes the mesh at the seam with the neighbors, so the chunk and its
/// horizontal neighbors are marked dirty and their derived caches dropped. At most
/// [`LOD_UPDATES_PER_FRAME`] transitions are applied per update: demoting many chunks at once
/// leaves them briefly empty, which shows up as a visibility dip while walking.
fn update_chunk_lods(
    mut commands: Commands,
    landscape: Option<Res<Landscape>>,
    mut dirty: ResMut<DirtyChunks>,
    mut caches: ResMut<DerivedCaches>,
    q: Query<(Entity, &ChunkLocal, Option<&ChunkLod>, Option<&CachedMesh>)>,
) {
    let Some(landscape) = landscape else {
        return;
    };
    let mut changed = 0;

    for (entity, ChunkLocal(chunk), lod, cached) in &q {
        let desired = lod.map_or_else(
            || desired_lod(*chunk, landscape.center),
            |lod| next_lod(lod.0, *chunk, landscape.center),
        );

        if lod.map(|lod| lod.0) == Some(desired) {
            continue;
        }
        if changed >= LOD_UPDATES_PER_FRAME {
            break;
        }
        changed += 1;

        commands.entity(entity).insert(ChunkLod(desired));

        // A chunk without a LOD component is adopting a coarse mesh it never had, which only
        // affects its neighbors when it lands on LOD 1. Every other transition changes the seam.
        let seams_changed = lod.is_some() || desired > 0;

        if lod.is_some() || cached.is_some() {
            commands.entity(entity).remove::<CachedMesh>();
        }

        if seams_changed {
            // Mark neighbor chunks as dirty, since they may need to be updated.
            for affected in adjacent_chunks(*chunk) {
                dirty.0.insert(affected);
                caches.invalidate(affected);
            }
        }
    }
}

/// Marks chunks dirty when their [`ChunkKind`] or [`ChunkLight`] changed.
///
/// Kind changes dirty the nine surrounding chunks and drop both derived caches; a chunk's
/// geometry depends on its neighbors' voxels. Light changes dirty the same nine chunks but keep
/// the occlusion cache, which only depends on kinds. Chunks that arrive with a [`CachedMesh`] are
/// skipped: their neighbors were archived with the same world, so the saved seams are already
/// correct.
fn mark_dirty_chunks(
    q_kind_changed: Query<(&ChunkLocal, Ref<ChunkKind>, Option<&CachedMesh>), Changed<ChunkKind>>,
    q_light_changed: Query<
        (&ChunkLocal, Ref<ChunkLight>, Option<&CachedMesh>),
        Changed<ChunkLight>,
    >,
    mut dirty: ResMut<DirtyChunks>,
    mut caches: ResMut<DerivedCaches>,
) {
    let mut count = 0;

    q_kind_changed.iter().for_each(|(local, kind, cached)| {
        // Chunks loaded from the archive already carry a valid mesh. Their neighbors were
        // saved with the same world, so there is nothing to rebuild until something changes.
        if kind.is_added() && cached.is_some() {
            return;
        }

        for chunk in affected_chunks(local.0) {
            dirty.0.insert(chunk);
            caches.invalidate(chunk);
            count += 1;
        }
    });

    q_light_changed.iter().for_each(|(local, light, cached)| {
        if light.is_added() && cached.is_some() {
            return;
        }

        for chunk in affected_chunks(local.0) {
            dirty.0.insert(chunk);
            caches.invalidate_light(chunk);
            count += 1;
        }
    });

    if count > 0 {
        trace!("[mark_dirty_chunks] {count} chunk invalidations.");
    }
}

fn neighbors_ready(chunk: Chunk, chunk_map: &ChunkMap, landscape: Option<&Landscape>) -> bool {
    affected_chunks(chunk).all(|neighbor| {
        neighbor == chunk
            || landscape.is_none_or(|landscape| !landscape.contains(neighbor))
            || chunk_map.0.contains_key(&neighbor)
    })
}

/// Read-only view of the chunk data meshing needs.
///
/// Grouping these into one [`SystemParam`] keeps [`generate_vertices`] within a readable argument
/// count and gives the lookups a place to live: a chunk's [`ChunkLod`] falls back to
/// [`desired_lod`] when the component is not there yet, and a missing [`Landscape`] treats every
/// chunk as full detail.
#[derive(SystemParam)]
struct ChunkView<'w, 's> {
    chunk_map: Res<'w, ChunkMap>,
    landscape: Option<Res<'w, Landscape>>,
    q_chunks: ChunkQuery<'w, 's, (&'static ChunkKind, &'static ChunkLight)>,
    q_vertex: Query<'w, 's, (&'static mut ChunkVertex, Option<&'static mut ChunkLod>)>,
}

impl ChunkView<'_, '_> {
    /// Chunk closest-first ordering key: the landscape center, or the origin without one.
    fn center(&self) -> bevy::math::IVec2 {
        self.landscape
            .as_ref()
            .map(|landscape| landscape.center)
            .unwrap_or_default()
    }

    /// Returns the LOD attached to `entity`, or the level it should adopt if it has none.
    fn lod_of(&self, chunk: Chunk, entity: Entity) -> u8 {
        self.q_vertex
            .get(entity)
            .ok()
            .and_then(|(_, lod)| lod.map(|lod| lod.0))
            .unwrap_or_else(|| {
                self.landscape
                    .as_ref()
                    .map(|landscape| desired_lod(chunk, landscape.center))
                    .unwrap_or(0)
            })
    }
}

/// Applies finished meshes and queues new ones on the async compute pool.
///
/// The system does not mesh anything itself. Each update it imports completed
/// [`MeshTasks`] results, discarding those whose chunk was re-dirtied or whose LOD no longer
/// matches, then admits up to [`MESH_MAX_SPAWNS`] new tasks while fewer than
/// [`MESH_MAX_IN_FLIGHT`] are running. Candidates are sorted nearest-first and alternated between
/// full and coarse so the player's immediate surroundings are not starved by either kind.
/// Chunks whose halo is not loaded stay dirty until their neighbors arrive.
fn generate_vertices(
    mut dirty: ResMut<DirtyChunks>,
    mut caches: ResMut<DerivedCaches>,
    mut view: ChunkView,
    mut tasks: ResMut<MeshTasks>,
    mut stats: ResMut<TerrainStats>,
) {
    let center = view.center();

    let completed = tasks
        .in_flight
        .iter()
        .filter(|(_, task)| task.is_finished())
        .map(|(chunk, _)| *chunk)
        .collect::<Vec<_>>();

    for chunk in completed {
        let task = tasks
            .in_flight
            .remove(&chunk)
            .expect("completed task must be in flight");
        let output = block_on(task);

        let Some(&entity) = view.chunk_map.0.get(&chunk) else {
            continue;
        };

        if dirty.0.contains(&chunk) {
            continue;
        }

        let current_lod = view
            .q_vertex
            .get(entity)
            .ok()
            .and_then(|(_, lod)| lod.map(|lod| lod.0));

        if current_lod.is_some_and(|current| current != output.lod) {
            dirty.0.insert(chunk);
            continue;
        }

        let (mut vertex, chunk_lod) = view.q_vertex.get_mut(entity).expect("Entity must exists");
        vertex.0 = output.vertex;
        if let Some(mut chunk_lod) = chunk_lod {
            chunk_lod.0 = output.lod;
        }

        if output.lod == 0 {
            stats.meshed_full += 1;
            caches.occlusion.insert(chunk, output.occlusion);
            caches.soft_light.insert(chunk, output.soft_light);
        } else {
            stats.meshed_coarse += 1;
        }
        stats.mesh_nanos += output.elapsed.as_nanos() as u64;
    }

    let mut chunks = std::mem::take(&mut dirty.0).into_iter().collect::<Vec<_>>();
    // Work closest to the player first; the result is consumed nearest-first when streamed.
    sort_nearest_first(&mut chunks, center);

    let mut full_candidates = Vec::new();
    let mut coarse_candidates = Vec::new();
    let mut deferred = 0;

    for chunk in chunks {
        let Some(&entity) = view.chunk_map.0.get(&chunk) else {
            continue;
        };

        if tasks.in_flight.contains_key(&chunk) {
            dirty.0.insert(chunk);
            continue;
        }

        // Meshing before the chunk's halo is loaded produces geometry that has to be rebuilt
        // when the neighbors arrive. Waiting for the halo removes those redundant passes.
        if !neighbors_ready(chunk, &view.chunk_map, view.landscape.as_deref()) {
            dirty.0.insert(chunk);
            deferred += 1;
            continue;
        }

        if view.lod_of(chunk, entity) == 0 {
            full_candidates.push(chunk);
        } else {
            coarse_candidates.push(chunk);
        }
    }

    let mut spawned = 0;
    let mut full_iter = full_candidates.into_iter();
    let mut coarse_iter = coarse_candidates.into_iter();
    let mut prefer_full = true;

    while spawned < MESH_MAX_SPAWNS && tasks.in_flight.len() < MESH_MAX_IN_FLIGHT {
        let chunk = if prefer_full {
            full_iter.next().or_else(|| coarse_iter.next())
        } else {
            coarse_iter.next().or_else(|| full_iter.next())
        };

        let Some(chunk) = chunk else {
            break;
        };

        let Some((kind, light)) = view.q_chunks.get_chunk(chunk) else {
            continue;
        };
        let Some(&entity) = view.chunk_map.0.get(&chunk) else {
            continue;
        };

        let lod = view.lod_of(chunk, entity);

        let neighbors = affected_chunks(chunk)
            .filter(|neighbor| *neighbor != chunk)
            .filter_map(|neighbor| {
                view.q_chunks
                    .get_chunk(neighbor)
                    .map(|(kind, light)| (neighbor, kind.0.clone(), light.0.clone()))
            })
            .collect::<Vec<_>>();

        let task = if lod > 0 {
            let kind = kind.0.clone();
            let light = light.0.clone();
            AsyncComputeTaskPool::get()
                .spawn(async move { compute_coarse_mesh(chunk, kind, light, neighbors, lod) })
        } else {
            let cached_occlusion = caches.occlusion.get(chunk).cloned();
            let cached_soft_light = caches.soft_light.get(chunk).cloned();
            let neighbor_lods = chunk::SIDES.map(|side| {
                let neighbor = chunk.neighbor(side.dir());
                let entity = view.chunk_map.0.get(&neighbor)?;
                view.q_vertex
                    .get(*entity)
                    .ok()
                    .and_then(|(_, lod)| lod.map(|lod| lod.0))
            });
            let kind = kind.0.clone();
            let light = light.0.clone();

            AsyncComputeTaskPool::get().spawn(async move {
                compute_full_mesh(
                    chunk,
                    kind,
                    light,
                    neighbors,
                    neighbor_lods,
                    cached_occlusion,
                    cached_soft_light,
                )
            })
        };

        tasks.in_flight.insert(chunk, task);
        spawned += 1;
        prefer_full = !prefer_full;
    }

    dirty.0.extend(full_iter);
    dirty.0.extend(coarse_iter);

    if spawned > 0 || deferred > 0 {
        trace!("[generate_vertices] {spawned} meshes queued, {deferred} waiting for halo.");
    }
}

fn compute_coarse_mesh(
    chunk: Chunk,
    kind: ChunkStorage<voxel::Kind>,
    light: ChunkStorage<voxel::Light>,
    neighbors: Vec<(Chunk, ChunkStorage<voxel::Kind>, ChunkStorage<voxel::Light>)>,
    lod: u8,
) -> FullMeshOutput {
    let started = Instant::now();
    let neighborhood = chunk::SIDES.map(|side| {
        let neighbor = chunk.neighbor(side.dir());
        neighbors
            .iter()
            .find_map(|(chunk, kind, _)| (*chunk == neighbor).then_some(kind))
    });
    let neighborhood_light = chunk::SIDES.map(|side| {
        let neighbor = chunk.neighbor(side.dir());
        neighbors
            .iter()
            .find_map(|(chunk, _, light)| (*chunk == neighbor).then_some(light))
    });

    let vertex = meshing::lod::generate_coarse_vertices(
        chunk,
        &kind,
        &light,
        &neighborhood,
        &neighborhood_light,
        |target| {
            if target == chunk {
                Some(&kind)
            } else {
                neighbors
                    .iter()
                    .find_map(|(neighbor, kind, _)| (*neighbor == target).then_some(kind))
            }
        },
        |target| {
            if target == chunk {
                Some(&light)
            } else {
                neighbors
                    .iter()
                    .find_map(|(neighbor, _, light)| (*neighbor == target).then_some(light))
            }
        },
        lod,
    );

    FullMeshOutput {
        lod,
        vertex,
        occlusion: ChunkStorage::default(),
        soft_light: ChunkStorage::default(),
        elapsed: started.elapsed(),
    }
}

fn compute_full_mesh(
    chunk: Chunk,
    kind: ChunkStorage<voxel::Kind>,
    light: ChunkStorage<voxel::Light>,
    neighbors: Vec<(Chunk, ChunkStorage<voxel::Kind>, ChunkStorage<voxel::Light>)>,
    neighbor_lods: [Option<u8>; chunk::SIDE_COUNT],
    cached_occlusion: Option<ChunkStorage<voxel::FacesOcclusion>>,
    cached_soft_light: Option<ChunkStorage<voxel::FacesSoftLight>>,
) -> FullMeshOutput {
    let started = Instant::now();
    let neighborhood = chunk::SIDES.map(|side| {
        let neighbor = chunk.neighbor(side.dir());
        neighbors
            .iter()
            .find_map(|(chunk, kind, _)| (*chunk == neighbor).then_some(kind))
    });

    let coverage = meshing::layer_coverage(&kind, &neighborhood);
    let occlusion = cached_occlusion.unwrap_or_else(|| {
        let mut occlusion = ChunkStorage::default();
        meshing::faces_occlusion_with_lod_skipping(
            &kind,
            &mut occlusion,
            &neighborhood,
            &neighbor_lods,
            &coverage,
        );
        occlusion
    });

    let fully_occluded = occlusion.all(|occ| occ.is_fully_occluded());
    let coverage = (!fully_occluded).then_some(coverage);

    let soft_light = if fully_occluded {
        cached_soft_light.unwrap_or_default()
    } else {
        cached_soft_light.unwrap_or_else(|| {
            let mut soft_light = ChunkStorage::default();
            light::smooth_lighting_skipping(
                chunk,
                &occlusion,
                &mut soft_light,
                |target| {
                    if target == chunk {
                        Some(&kind)
                    } else {
                        neighbors
                            .iter()
                            .find_map(|(neighbor, kind, _)| (*neighbor == target).then_some(kind))
                    }
                },
                |target| {
                    if target == chunk {
                        Some(&light)
                    } else {
                        neighbors
                            .iter()
                            .find_map(|(neighbor, _, light)| (*neighbor == target).then_some(light))
                    }
                },
                coverage.as_ref().expect("non-empty mesh needs coverage"),
            );
            soft_light
        })
    };

    let vertex = if fully_occluded {
        Vec::new()
    } else {
        let coverage = coverage.as_ref().expect("non-empty mesh needs coverage");
        let faces = meshing::generate_faces_skipping(&kind, &occlusion, &soft_light, coverage);
        meshing::generate_vertices(&faces)
    };

    FullMeshOutput {
        lod: 0,
        vertex,
        occlusion,
        soft_light,
        elapsed: started.elapsed(),
    }
}

#[cfg(test)]
mod tests {
    use bevy::app::ScheduleRunnerPlugin;
    use projekto_core::coords::{Chunk, ChunkVoxel};

    use crate::bundle::{ChunkBundle, ChunkKind, ChunkLight, ChunkLocal, ChunkMap, ChunkVertex};

    use super::*;

    #[test]
    fn distant_chunk_is_coarsened_and_cached_mesh_is_dropped() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_once()))
            .init_resource::<ChunkMap>()
            .insert_resource(super::Landscape {
                center: bevy::math::IVec2::ZERO,
                radius: 16,
            })
            .add_plugins(MeshingPlugin);

        let chunk = Chunk::new(19, 0);
        let entity = app
            .world_mut()
            .spawn((
                ChunkBundle {
                    kind: ChunkKind(solid_kind()),
                    light: ChunkLight(ChunkStorage::default()),
                    local: ChunkLocal(chunk),
                    vertex: ChunkVertex::default(),
                },
                ChunkLod(0),
                CachedMesh,
            ))
            .id();

        app.world_mut()
            .resource_mut::<ChunkMap>()
            .0
            .insert(chunk, entity);

        app.update();

        assert_eq!(app.world().get::<ChunkLod>(entity).unwrap().0, 1);
        assert!(app.world().get::<CachedMesh>(entity).is_none());

        app.world_mut().resource_mut::<super::Landscape>().center = bevy::math::IVec2::new(19, 0);
        app.update();

        assert_eq!(app.world().get::<ChunkLod>(entity).unwrap().0, 0);
    }

    #[test]
    fn lod_is_full_near_the_center_and_coarse_outside() {
        let center = bevy::math::IVec2::new(8, 8);

        assert_eq!(desired_lod(Chunk::new(8, 8), center), 0);
        assert_eq!(desired_lod(Chunk::new(0, 0), center), 0);
        assert_eq!(desired_lod(Chunk::new(24, 8), center), 0);
        assert_eq!(desired_lod(Chunk::new(25, 8), center), 1);
        assert_eq!(desired_lod(Chunk::new(8, 25), center), 1);
    }

    fn app_with_chunk() -> (App, Entity, Chunk) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_once()))
            .init_resource::<ChunkMap>()
            .add_plugins(MeshingPlugin);

        let chunk = Chunk::new(0, 0);
        let mut kind = ChunkStorage::<voxel::Kind>::default();
        kind.set(ChunkVoxel::new(0, 0, 0), voxel::Kind::id(1));
        kind.pack();

        let entity = app
            .world_mut()
            .spawn(ChunkBundle {
                kind: ChunkKind(kind),
                light: ChunkLight(ChunkStorage::default()),
                local: ChunkLocal(chunk),
                vertex: ChunkVertex::default(),
            })
            .id();

        app.world_mut()
            .resource_mut::<ChunkMap>()
            .0
            .insert(chunk, entity);

        (app, entity, chunk)
    }

    fn solid_kind() -> ChunkStorage<voxel::Kind> {
        let mut kind = ChunkStorage::default();
        kind.set(ChunkVoxel::new(0, 0, 0), voxel::Kind::id(1));
        kind.pack();
        kind
    }

    fn settle_meshing(app: &mut App) {
        for _ in 0..2000 {
            app.update();
            if app.world().resource::<MeshTasks>().in_flight.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(
            app.world().resource::<MeshTasks>().in_flight.is_empty(),
            "meshing tasks did not finish"
        );
    }

    #[test]
    fn cached_mesh_is_not_rebuilt_on_spawn() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_once()))
            .init_resource::<ChunkMap>()
            .add_plugins(MeshingPlugin);

        let chunk = Chunk::new(0, 0);
        let cached = vec![voxel::Vertex::default()];
        let entity = app
            .world_mut()
            .spawn((
                ChunkBundle {
                    kind: ChunkKind(solid_kind()),
                    light: ChunkLight(ChunkStorage::default()),
                    local: ChunkLocal(chunk),
                    vertex: ChunkVertex(cached.clone()),
                },
                CachedMesh,
            ))
            .id();
        app.world_mut()
            .resource_mut::<ChunkMap>()
            .0
            .insert(chunk, entity);

        app.update();

        let vertex = app.world().get::<ChunkVertex>(entity).unwrap();
        assert_eq!(vertex.0, cached, "cached mesh must be reused");
        assert!(
            !app.world()
                .resource::<DerivedCaches>()
                .occlusion
                .contains(chunk),
            "cached chunk must not be meshed"
        );
    }

    #[test]
    fn meshing_populates_derived_cache_and_vertices() {
        let (mut app, entity, chunk) = app_with_chunk();

        settle_meshing(&mut app);

        let vertex = app.world().get::<ChunkVertex>(entity).unwrap();
        assert!(!vertex.0.is_empty(), "chunk should generate vertices");

        let caches = app.world().resource::<DerivedCaches>();
        assert!(caches.occlusion.contains(chunk));
        assert!(caches.soft_light.contains(chunk));
        assert_eq!(caches.occlusion.stats().misses, 1);
        assert_eq!(caches.soft_light.stats().misses, 1);
    }

    #[test]
    fn kind_change_invalidates_and_recomputes_derived_cache() {
        let (mut app, entity, chunk) = app_with_chunk();
        settle_meshing(&mut app);

        let initial_misses = app
            .world()
            .resource::<DerivedCaches>()
            .soft_light
            .stats()
            .misses;

        app.world_mut()
            .get_mut::<ChunkKind>(entity)
            .expect("chunk kind must exist")
            .0
            .set(ChunkVoxel::new(1, 0, 0), voxel::Kind::id(1));

        settle_meshing(&mut app);

        let caches = app.world().resource::<DerivedCaches>();
        assert!(caches.occlusion.contains(chunk));
        assert!(caches.soft_light.contains(chunk));
        assert_eq!(
            caches.soft_light.stats().misses,
            initial_misses + 1,
            "invalidated soft light must be recomputed"
        );
    }

    #[test]
    fn kind_change_invalidates_diagonal_neighbor_cache() {
        let (mut app, entity_a, _chunk_a) = app_with_chunk();
        let chunk_b = Chunk::new(1, 1);

        let entity_b = app
            .world_mut()
            .spawn(ChunkBundle {
                kind: ChunkKind(solid_kind()),
                light: ChunkLight(ChunkStorage::default()),
                local: ChunkLocal(chunk_b),
                vertex: ChunkVertex::default(),
            })
            .id();

        app.world_mut()
            .resource_mut::<ChunkMap>()
            .0
            .insert(chunk_b, entity_b);

        settle_meshing(&mut app);

        let initial_misses = app
            .world()
            .resource::<DerivedCaches>()
            .soft_light
            .stats()
            .misses;
        assert_eq!(initial_misses, 2, "both chunks should be cached");

        app.world_mut()
            .get_mut::<ChunkKind>(entity_a)
            .expect("chunk kind must exist")
            .0
            .set(ChunkVoxel::new(1, 0, 0), voxel::Kind::id(1));

        settle_meshing(&mut app);

        let caches = app.world().resource::<DerivedCaches>();
        assert!(
            caches.soft_light.contains(chunk_b),
            "diagonal neighbor should be recomputed and cached"
        );
        assert_eq!(
            caches.soft_light.stats().misses,
            initial_misses + 2,
            "the changed chunk and its diagonal neighbor must both be recomputed"
        );
    }

    #[test]
    fn light_change_reuses_cached_occlusion() {
        let (mut app, entity, _chunk) = app_with_chunk();
        settle_meshing(&mut app);

        let initial_occlusion = app.world().resource::<DerivedCaches>().occlusion.stats();
        let initial_soft_light = app.world().resource::<DerivedCaches>().soft_light.stats();

        app.world_mut()
            .get_mut::<ChunkLight>(entity)
            .expect("chunk light must exist")
            .0
            .set_type(
                ChunkVoxel::new(1, 0, 0),
                voxel::LightTy::Natural,
                voxel::Light::MAX_NATURAL_INTENSITY,
            );

        settle_meshing(&mut app);

        let caches = app.world().resource::<DerivedCaches>();
        assert_eq!(
            caches.occlusion.stats().misses,
            initial_occlusion.misses,
            "occlusion does not depend on light"
        );
        assert_eq!(
            caches.occlusion.stats().hits,
            initial_occlusion.hits + 1,
            "occlusion should be served from the cache"
        );
        assert_eq!(
            caches.soft_light.stats().misses,
            initial_soft_light.misses + 1,
            "soft light must be recomputed after a light change"
        );
    }

    #[test]
    fn kind_change_invalidates_neighbor_cache() {
        let (mut app, entity_a, _chunk_a) = app_with_chunk();
        let chunk_b = Chunk::new(1, 0);

        let entity_b = app
            .world_mut()
            .spawn(ChunkBundle {
                kind: ChunkKind(solid_kind()),
                light: ChunkLight(ChunkStorage::default()),
                local: ChunkLocal(chunk_b),
                vertex: ChunkVertex::default(),
            })
            .id();

        app.world_mut()
            .resource_mut::<ChunkMap>()
            .0
            .insert(chunk_b, entity_b);

        settle_meshing(&mut app);

        let initial_misses = app
            .world()
            .resource::<DerivedCaches>()
            .soft_light
            .stats()
            .misses;
        assert_eq!(initial_misses, 2, "both chunks should be cached");

        app.world_mut()
            .get_mut::<ChunkKind>(entity_a)
            .expect("chunk kind must exist")
            .0
            .set(ChunkVoxel::new(1, 0, 0), voxel::Kind::id(1));

        settle_meshing(&mut app);

        let caches = app.world().resource::<DerivedCaches>();
        assert!(
            caches.soft_light.contains(chunk_b),
            "neighbor should be recomputed and cached"
        );
        assert_eq!(
            caches.soft_light.stats().misses,
            initial_misses + 2,
            "the changed chunk and its neighbor must both be recomputed"
        );
    }
}
