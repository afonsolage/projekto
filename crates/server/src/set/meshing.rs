use bevy::prelude::*;
use projekto_core::{
    chunk::{self, ChunkStorage},
    voxel,
};

use crate::{
    DerivedCaches, DirtyChunks, WorldSet, affected_chunks,
    bundle::{ChunkKind, ChunkLight, ChunkLocal, ChunkMap, ChunkQuery, ChunkVertex},
    debug, light, meshing,
};

pub struct MeshingPlugin;

impl Plugin for MeshingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DerivedCaches>()
            .init_resource::<DirtyChunks>()
            .add_systems(
                Update,
                (mark_dirty_chunks, generate_vertices)
                    .chain()
                    .in_set(WorldSet::Meshing),
            );
    }
}

fn mark_dirty_chunks(
    q_kind_changed: Query<&ChunkLocal, Changed<ChunkKind>>,
    q_light_changed: Query<&ChunkLocal, Changed<ChunkLight>>,
    mut dirty: ResMut<DirtyChunks>,
    mut caches: ResMut<DerivedCaches>,
) {
    let mut count = 0;

    q_kind_changed.iter().for_each(|local| {
        for chunk in affected_chunks(local.0) {
            dirty.0.insert(chunk);
            caches.invalidate(chunk);
            count += 1;
        }
    });

    q_light_changed.iter().for_each(|local| {
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

fn generate_vertices(
    mut dirty: ResMut<DirtyChunks>,
    mut caches: ResMut<DerivedCaches>,
    chunk_map: Res<ChunkMap>,
    q_chunks: ChunkQuery<(&ChunkKind, &ChunkLight)>,
    mut q_vertex: Query<&mut ChunkVertex>,
    mut metrics: ResMut<debug::Metrics>,
) {
    let chunks = std::mem::take(&mut dirty.0);
    let mut count = 0;
    let mut map = [0; voxel::SIDE_COUNT];

    let mut metric_data = debug::MetricData::new("generate_vertices");

    for chunk in chunks {
        let Some((kind, _light)) = q_chunks.get_chunk(chunk) else {
            continue;
        };
        let Some(&entity) = chunk_map.0.get(&chunk) else {
            continue;
        };

        let kind = &kind.0;
        let neighborhood = chunk::SIDES.map(|side| {
            q_chunks
                .get_chunk(chunk.neighbor(side.dir()))
                .map(|(kind, _)| &kind.0)
        });

        let DerivedCaches {
            occlusion: occlusion_cache,
            soft_light: soft_light_cache,
        } = &mut *caches;

        let occlusion = occlusion_cache.get_or_insert_with(chunk, || {
            let mut occlusion = ChunkStorage::default();
            meshing::faces_occlusion(kind, &mut occlusion, &neighborhood);
            occlusion
        });

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

        metric_data.begin();

        let mut vertex = if occlusion.all(|occ| occ.is_fully_occluded()) {
            Vec::new()
        } else {
            let faces = meshing::generate_faces(kind, occlusion, soft_light);
            faces.iter().for_each(|face| map[face.side.index()] += 1);
            meshing::generate_vertices(&faces)
        };

        let mut chunk_vertex = q_vertex.get_mut(entity).expect("Entity must exists");
        std::mem::swap(&mut vertex, &mut chunk_vertex);

        metric_data.end();
        count += 1;
    }

    metrics.add(metric_data);

    if count > 0 {
        trace!("[generate_vertices] {count} chunks vertices generated. {map:?}");
    }
}

#[cfg(test)]
mod tests {
    use bevy::app::ScheduleRunnerPlugin;
    use projekto_core::coords::{Chunk, ChunkVoxel};

    use crate::{
        bundle::{ChunkBundle, ChunkKind, ChunkLight, ChunkLocal, ChunkMap, ChunkVertex},
        debug::Metrics,
    };

    use super::*;

    fn app_with_chunk() -> (App, Entity, Chunk) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_once()))
            .init_resource::<ChunkMap>()
            .insert_resource(Metrics::new())
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
                kind: ChunkKind(ChunkStorage::default()),
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
                kind: ChunkKind(ChunkStorage::default()),
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
