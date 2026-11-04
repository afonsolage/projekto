use crate::{
    DerivedCaches, DirtyChunks, WorldSet, affected_chunks,
    bundle::{
        CachedMesh, ChunkKind, ChunkLight, ChunkLocal, ChunkLod, ChunkMap, ChunkQuery, ChunkVertex,
    },
    cache::sort_nearest_first,
    light, meshing,
};
use bevy::prelude::*;
use projekto_core::{
    chunk::{self, ChunkStorage},
    coords::Chunk,
    voxel,
};

use super::Landscape;

pub struct MeshingPlugin;

impl Plugin for MeshingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DerivedCaches>()
            .init_resource::<DirtyChunks>()
            .add_systems(
                Update,
                (update_chunk_lods, mark_dirty_chunks, generate_vertices)
                    .chain()
                    .in_set(WorldSet::Meshing),
            );
    }
}

/// Chunks inside this square keep full-resolution meshes; chunks outside use a coarser LOD.
pub const LOD_FULL_RADIUS: i32 = 16;

pub(crate) fn desired_lod(chunk: Chunk, center: bevy::math::IVec2) -> u8 {
    let dx = (chunk.x - center.x).abs();
    let dz = (chunk.z - center.y).abs();

    if dx <= LOD_FULL_RADIUS && dz <= LOD_FULL_RADIUS {
        0
    } else {
        1
    }
}

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

    for (entity, ChunkLocal(chunk), lod, cached) in &q {
        let desired = desired_lod(*chunk, landscape.center);

        match lod {
            Some(lod) if lod.0 == desired => {}
            Some(_) => {
                commands.entity(entity).insert(ChunkLod(desired));
                commands.entity(entity).remove::<CachedMesh>();

                for affected in affected_chunks(*chunk) {
                    dirty.0.insert(affected);
                    caches.invalidate(affected);
                }
            }
            None => {
                commands.entity(entity).insert(ChunkLod(desired));

                if desired != 0 {
                    if cached.is_some() {
                        commands.entity(entity).remove::<CachedMesh>();
                    }

                    for affected in affected_chunks(*chunk) {
                        dirty.0.insert(affected);
                        caches.invalidate(affected);
                    }
                }
            }
        }
    }
}

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

fn generate_vertices(
    mut dirty: ResMut<DirtyChunks>,
    mut caches: ResMut<DerivedCaches>,
    chunk_map: Res<ChunkMap>,
    landscape: Option<Res<Landscape>>,
    q_chunks: ChunkQuery<(&ChunkKind, &ChunkLight)>,
    mut q_vertex: Query<(&mut ChunkVertex, Option<&mut ChunkLod>)>,
) {
    let center = landscape
        .as_ref()
        .map(|landscape| landscape.center)
        .unwrap_or_default();
    let mut chunks = std::mem::take(&mut dirty.0).into_iter().collect::<Vec<_>>();
    // Work closest to the player first; the result is consumed nearest-first when streamed.
    sort_nearest_first(&mut chunks, center);

    let mut count = 0;
    let mut deferred = 0;
    let mut map = [0; voxel::SIDE_COUNT];

    for chunk in chunks {
        // Meshing before the chunk's halo is loaded produces geometry that has to be rebuilt
        // when the neighbors arrive. Waiting for the halo removes those redundant passes.
        if !neighbors_ready(chunk, &chunk_map, landscape.as_deref()) {
            dirty.0.insert(chunk);
            deferred += 1;
            continue;
        }

        let Some((kind, light)) = q_chunks.get_chunk(chunk) else {
            continue;
        };
        let Some(&entity) = chunk_map.0.get(&chunk) else {
            continue;
        };

        let kind = &kind.0;

        let (mut chunk_vertex, chunk_lod) = q_vertex.get_mut(entity).expect("Entity must exists");
        let lod = landscape
            .as_ref()
            .map(|landscape| desired_lod(chunk, landscape.center))
            .unwrap_or(0);

        if let Some(mut chunk_lod) = chunk_lod {
            chunk_lod.0 = lod;
        }

        let neighborhood = chunk::SIDES.map(|side| {
            q_chunks
                .get_chunk(chunk.neighbor(side.dir()))
                .map(|(kind, _)| &kind.0)
        });
        let neighborhood_light = chunk::SIDES.map(|side| {
            q_chunks
                .get_chunk(chunk.neighbor(side.dir()))
                .map(|(_, light)| &light.0)
        });
        let neighborhood_lod =
            chunk::SIDES.map(|side| Some(desired_lod(chunk.neighbor(side.dir()), center)));

        let mut vertex = if lod > 0 {
            meshing::lod::generate_coarse_vertices(
                chunk,
                kind,
                &light.0,
                &neighborhood,
                &neighborhood_light,
                |chunk| q_chunks.get_chunk(chunk).map(|(kind, _)| &kind.0),
                |chunk| q_chunks.get_chunk(chunk).map(|(_, light)| &light.0),
                lod,
            )
        } else {
            let DerivedCaches {
                occlusion: occlusion_cache,
                soft_light: soft_light_cache,
            } = &mut *caches;

            let occlusion = occlusion_cache.get_or_insert_with(chunk, || {
                let mut occlusion = ChunkStorage::default();
                meshing::faces_occlusion_with_lod(
                    kind,
                    &mut occlusion,
                    &neighborhood,
                    &neighborhood_lod,
                );
                occlusion
            });

            if occlusion.all(|occ| occ.is_fully_occluded()) {
                Vec::new()
            } else {
                let soft_light = soft_light_cache.get_or_insert_with(chunk, || {
                    let mut soft_light = ChunkStorage::default();
                    light::smooth_lighting(
                        chunk,
                        occlusion,
                        &mut soft_light,
                        |chunk| q_chunks.get_chunk(chunk).map(|(kind, _)| &kind.0),
                        |chunk| q_chunks.get_chunk(chunk).map(|(_, light)| &light.0),
                    );
                    soft_light
                });

                let faces = meshing::generate_faces(kind, occlusion, soft_light);
                faces.iter().for_each(|face| map[face.side.index()] += 1);
                meshing::generate_vertices(&faces)
            }
        };

        std::mem::swap(&mut vertex, &mut chunk_vertex);

        count += 1;
    }

    if count > 0 || deferred > 0 {
        trace!(
            "[generate_vertices] {count} chunks vertices generated, {deferred} waiting for halo. {map:?}"
        );
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

        let chunk = Chunk::new(17, 0);
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

        app.world_mut().resource_mut::<super::Landscape>().center = bevy::math::IVec2::new(17, 0);
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

        app.update();

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
        app.update();

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

        app.update();

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

        app.update();

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

        app.update();

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
        app.update();

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

        app.update();

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

        app.update();

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

        app.update();

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
