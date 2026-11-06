use bevy::prelude::*;
use projekto_core::{
    chunk::{self},
    voxel::LightTy,
};

use crate::{
    WorldSet,
    bundle::{ChunkKind, ChunkLight, ChunkLocal, ChunkMap},
    light,
};

use super::LightUpdate;

pub struct ChunkInitializationPlugin;

impl Plugin for ChunkInitializationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            seed_chunk_borders.in_set(WorldSet::ChunkInitialization),
        );
    }
}

/// Seeds natural light across the borders of chunks that just loaded.
///
/// A chunk can load before or after its neighbour, and light events sent to a missing neighbour
/// are dropped. Seeding both directions whenever either side appears makes the result independent
/// of load order and keeps cave openings lit across chunk borders.
fn seed_chunk_borders(
    new_chunks: Query<(&ChunkLocal, &ChunkKind, &ChunkLight), Added<ChunkLight>>,
    chunks: Query<(&ChunkKind, &ChunkLight)>,
    map: Res<ChunkMap>,
    mut writer: MessageWriter<LightUpdate>,
) {
    for (local, kind, light) in &new_chunks {
        for side in chunk::SIDES {
            let neighbor = local.neighbor(side.dir());
            let Some(&entity) = map.get(&neighbor) else {
                continue;
            };
            let Ok((neighbor_kind, neighbor_light)) = chunks.get(entity) else {
                continue;
            };

            let into_chunk = light::border_transfer(
                neighbor_kind,
                &neighbor_light.0,
                kind,
                side.opposite(),
                LightTy::Natural,
            );
            if !into_chunk.is_empty() {
                writer.write(LightUpdate {
                    chunk: local.0,
                    ty: LightTy::Natural,
                    values: into_chunk,
                });
            }

            let into_neighbor =
                light::border_transfer(kind, &light.0, neighbor_kind, side, LightTy::Natural);
            if !into_neighbor.is_empty() {
                writer.write(LightUpdate {
                    chunk: neighbor,
                    ty: LightTy::Natural,
                    values: into_neighbor,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::app::ScheduleRunnerPlugin;
    use projekto_core::{
        chunk::ChunkStorage,
        coords::{Chunk, ChunkVoxel},
        voxel::{self, LightTy},
    };

    use crate::{
        WorldSet,
        bundle::{CachedMesh, ChunkBundle, ChunkKind, ChunkLight, ChunkLocal, ChunkMap},
        set::PropagationPlugin,
    };

    use super::*;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_once()))
            .add_plugins((ChunkInitializationPlugin, PropagationPlugin))
            .configure_sets(
                Update,
                (WorldSet::ChunkInitialization, WorldSet::Propagation).chain(),
            );
        app
    }

    fn spawn_chunk(
        app: &mut App,
        chunk: Chunk,
        light: Option<(ChunkVoxel, u8)>,
        cached: bool,
    ) -> Entity {
        let mut storage = ChunkStorage::<voxel::Light>::default();
        if let Some((voxel, intensity)) = light {
            storage.set_type(voxel, LightTy::Natural, intensity);
        }

        let mut entity = app.world_mut().spawn(ChunkBundle {
            local: ChunkLocal(chunk),
            light: ChunkLight(storage),
            ..Default::default()
        });

        if cached {
            entity.insert(CachedMesh);
        }

        entity.id()
    }

    fn add_to_map(app: &mut App, chunk: Chunk, entity: Entity) {
        app.world_mut()
            .get_resource_or_insert_with(ChunkMap::default)
            .0
            .insert(chunk, entity);
    }

    #[test]
    fn seeds_light_across_the_border_in_both_directions() {
        let mut app = test_app();

        let a = spawn_chunk(
            &mut app,
            Chunk::default(),
            Some((ChunkVoxel::new(Chunk::X_END, 5, 3), 15)),
            false,
        );
        add_to_map(&mut app, Chunk::default(), a);
        app.update();

        // B loads later, so the load-order case is covered: A is not `Added` anymore.
        let b = spawn_chunk(
            &mut app,
            Chunk::new(1, 0),
            Some((ChunkVoxel::new(0, 6, 4), 15)),
            false,
        );
        add_to_map(&mut app, Chunk::new(1, 0), b);
        app.update();
        app.update();

        let a_light = app.world().get::<ChunkLight>(a).unwrap();
        let b_light = app.world().get::<ChunkLight>(b).unwrap();

        assert_eq!(
            b_light.get(ChunkVoxel::new(0, 5, 3)).get(LightTy::Natural),
            14,
            "the new chunk should receive light from the old one"
        );
        assert_eq!(
            a_light
                .get(ChunkVoxel::new(Chunk::X_END, 6, 4))
                .get(LightTy::Natural),
            14,
            "the old chunk should receive light from the new one"
        );
    }

    #[test]
    fn cached_archive_chunk_is_remeshed_after_light_seeding() {
        let mut app = test_app();

        let a = spawn_chunk(
            &mut app,
            Chunk::default(),
            Some((ChunkVoxel::new(Chunk::X_END, 5, 3), 15)),
            false,
        );
        add_to_map(&mut app, Chunk::default(), a);
        app.update();

        let b = spawn_chunk(&mut app, Chunk::new(1, 0), None, true);
        add_to_map(&mut app, Chunk::new(1, 0), b);
        app.update();
        app.update();

        let b_light = app.world().get::<ChunkLight>(b).unwrap();
        assert_eq!(
            b_light.get(ChunkVoxel::new(0, 5, 3)).get(LightTy::Natural),
            14
        );
        assert!(
            app.world().get::<CachedMesh>(b).is_none(),
            "a saved mesh must not survive a light change"
        );
    }

    #[test]
    fn seeds_light_on_z_and_negative_borders() {
        for (a_chunk, a_voxel, b_chunk, b_voxel) in [
            (
                Chunk::default(),
                ChunkVoxel::new(7, 7, 0),
                Chunk::new(0, -1),
                ChunkVoxel::new(7, 7, Chunk::Z_END),
            ),
            (
                Chunk::new(-2, 0),
                ChunkVoxel::new(Chunk::X_END, 8, 8),
                Chunk::new(-1, 0),
                ChunkVoxel::new(0, 8, 8),
            ),
        ] {
            let mut app = test_app();
            let a = spawn_chunk(&mut app, a_chunk, Some((a_voxel, 15)), false);
            add_to_map(&mut app, a_chunk, a);
            app.update();

            let b = spawn_chunk(&mut app, b_chunk, None, false);
            add_to_map(&mut app, b_chunk, b);
            app.update();
            app.update();

            let b_light = app.world().get::<ChunkLight>(b).unwrap();
            assert_eq!(
                b_light.get(b_voxel).get(LightTy::Natural),
                14,
                "light should cross the {a_chunk:?} -> {b_chunk:?} border"
            );
        }
    }

    #[test]
    fn seeds_do_not_touch_opaque_targets() {
        let mut app = test_app();

        let a = spawn_chunk(
            &mut app,
            Chunk::default(),
            Some((ChunkVoxel::new(Chunk::X_END, 5, 3), 15)),
            false,
        );
        add_to_map(&mut app, Chunk::default(), a);
        app.update();

        let mut kind = ChunkStorage::<voxel::Kind>::default();
        kind.set(ChunkVoxel::new(0, 5, 3), voxel::Kind::id(1));

        let b = app
            .world_mut()
            .spawn(ChunkBundle {
                local: ChunkLocal(Chunk::new(1, 0)),
                kind: ChunkKind(kind),
                ..Default::default()
            })
            .id();
        add_to_map(&mut app, Chunk::new(1, 0), b);
        app.update();
        app.update();

        let b_light = app.world().get::<ChunkLight>(b).unwrap();
        assert_eq!(
            b_light.get(ChunkVoxel::new(0, 5, 3)).get(LightTy::Natural),
            0,
            "opaque voxels must not receive light"
        );
    }

    #[test]
    fn neighbour_kinds_are_used_for_the_transfer() {
        // The helper signature is also exercised by the tests above; this one makes sure the
        // chunk kind is read from the entity rather than assumed to be air.
        let mut app = test_app();
        let mut kind = ChunkStorage::<voxel::Kind>::default();
        kind.set(ChunkVoxel::new(Chunk::X_END, 5, 3), voxel::Kind::id(1));

        let a = app
            .world_mut()
            .spawn(ChunkBundle {
                kind: ChunkKind(kind),
                light: ChunkLight({
                    let mut light = ChunkStorage::<voxel::Light>::default();
                    light.set_type(ChunkVoxel::new(Chunk::X_END, 5, 3), LightTy::Natural, 15);
                    light
                }),
                ..Default::default()
            })
            .id();
        add_to_map(&mut app, Chunk::default(), a);
        app.update();

        let b = spawn_chunk(&mut app, Chunk::new(1, 0), None, false);
        add_to_map(&mut app, Chunk::new(1, 0), b);
        app.update();
        app.update();

        let b_light = app.world().get::<ChunkLight>(b).unwrap();
        assert_eq!(
            b_light.get(ChunkVoxel::new(0, 5, 3)).get(LightTy::Natural),
            0,
            "light stored on an opaque source must not cross the border"
        );
    }
}
