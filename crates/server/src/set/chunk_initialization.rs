use bevy::prelude::*;
use projekto_core::{
    chunk::{self},
    voxel::LightTy,
};

use crate::{
    WorldSet,
    bundle::{ChunkLight, ChunkLocal, ChunkMap},
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
    new_chunks: Query<(&ChunkLocal, &ChunkLight), Added<ChunkLight>>,
    lights: Query<&ChunkLight>,
    map: Res<ChunkMap>,
    mut writer: MessageWriter<LightUpdate>,
) {
    for (local, light) in &new_chunks {
        for side in chunk::SIDES {
            let neighbor = local.neighbor(side.dir());
            let Some(&entity) = map.get(&neighbor) else {
                continue;
            };
            let Ok(neighbor_light) = lights.get(entity) else {
                continue;
            };

            let into_chunk =
                light::border_transfer(&neighbor_light.0, side.opposite(), LightTy::Natural);
            if !into_chunk.is_empty() {
                writer.write(LightUpdate {
                    chunk: local.0,
                    ty: LightTy::Natural,
                    values: into_chunk,
                });
            }

            let into_neighbor = light::border_transfer(&light.0, side, LightTy::Natural);
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
    use bevy::{app::ScheduleRunnerPlugin, platform::collections::HashMap};
    use projekto_core::{
        chunk::ChunkStorage,
        coords::{Chunk, ChunkVoxel},
        voxel::{self},
    };

    use crate::{
        WorldSet,
        bundle::{ChunkBundle, ChunkLight, ChunkLocal, ChunkMap},
        set::PropagationPlugin,
    };

    use super::*;

    #[test]
    fn seeds_light_across_the_border() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_once()))
            .add_plugins((ChunkInitializationPlugin, PropagationPlugin))
            .configure_sets(
                Update,
                (WorldSet::ChunkInitialization, WorldSet::Propagation).chain(),
            );

        let mut light = ChunkStorage::<voxel::Light>::default();
        light.set_type(ChunkVoxel::new(Chunk::X_END, 5, 3), LightTy::Natural, 15);

        let a = app
            .world_mut()
            .spawn(ChunkBundle {
                light: ChunkLight(light),
                ..Default::default()
            })
            .id();
        let b = app
            .world_mut()
            .spawn(ChunkBundle {
                local: ChunkLocal(Chunk::new(1, 0)),
                ..Default::default()
            })
            .id();

        app.world_mut().insert_resource(ChunkMap(HashMap::from([
            (Chunk::default(), a),
            (Chunk::new(1, 0), b),
        ])));

        app.update();
        app.update();

        let light = app.world().get::<ChunkLight>(b).unwrap();

        assert_eq!(
            light.get(ChunkVoxel::new(0, 5, 3)).get(LightTy::Natural),
            14,
            "light should cross the border from the lit chunk"
        );
    }
}
