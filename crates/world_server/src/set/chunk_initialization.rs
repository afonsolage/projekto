use bevy::{prelude::*, utils::HashMap};
use projekto_core::{
    chunk::{self},
    voxel::{self},
};

use crate::{
    any_chunk,
    light::{self, NeighborLightPropagation},
    ChunkKind, ChunkLight, ChunkLocal, LightUpdate, WorldSet,
};

pub struct ChunkInitializationPlugin;

impl Plugin for ChunkInitializationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (init_light
                .run_if(any_chunk::<Added<ChunkLight>>)
                .in_set(WorldSet::ChunkInitialization),),
        );
    }
}

fn init_light(
    mut q: Query<(&ChunkLocal, &ChunkKind, &mut ChunkLight), Added<ChunkLight>>,
    mut writer: EventWriter<LightUpdate>,
) {
    let mut map = HashMap::new();
    let mut count = 0;

    q.for_each_mut(|(local, kind, mut light)| {
        chunk::top_voxels().for_each(|voxel| {
            light.set_type(
                voxel,
                voxel::LightTy::Natural,
                voxel::Light::MAX_NATURAL_INTENSITY,
            );
        });

        let neighbor_propagation = light::propagate(
            kind,
            &mut light,
            voxel::LightTy::Natural,
            chunk::top_voxels(),
        );

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

#[cfg(test)]
mod tests {
    use bevy::app::ScheduleRunnerPlugin;

    use crate::ChunkBundle;

    use super::*;

    #[test]
    fn init_light_empty_chunk() {
        // Arrange
        let mut app = App::new();
        app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_once()))
            .add_event::<LightUpdate>()
            .add_plugins(ChunkInitializationPlugin);

        app.world.spawn(ChunkBundle::default());

        // Act
        app.update();

        // Assert
        let light = app.world.query::<&ChunkLight>().single(&app.world);
        assert!(
            light
                .iter()
                .all(|light| light.get(voxel::LightTy::Natural)
                    == voxel::Light::MAX_NATURAL_INTENSITY),
            "All voxels should have max natural light on empty chunk"
        );

        let light_update_events = app.world.resource::<Events<LightUpdate>>();
        assert_eq!(
            light_update_events.len(),
            4,
            "Init light should propagate to all 4 neighbors"
        );
    }
}
