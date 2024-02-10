use std::time::Duration;

use bevy::{prelude::*, time::common_conditions::on_timer};
use set::{
    ChunkInitializationPlugin, ChunkManagementPlugin, LandscapePlugin, MeshingPlugin,
    PropagationPlugin,
};

pub mod app;
pub mod channel;
mod genesis;
mod light;
mod meshing;

pub mod bundle;
pub mod set;

const MESHING_TICK_MS: u64 = 500;

pub struct WorldServerPlugin;

impl Plugin for WorldServerPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            Update,
            (
                WorldSet::LandscapeUpdate.before(WorldSet::ChunkManagement),
                WorldSet::ChunkManagement.before(WorldSet::FlushCommands),
                WorldSet::ChunkInitialization.after(WorldSet::FlushCommands),
                WorldSet::Propagation.after(WorldSet::ChunkInitialization),
                WorldSet::Meshing
                    .after(WorldSet::Propagation)
                    .run_if(on_timer(Duration::from_millis(MESHING_TICK_MS))),
            ),
        )
        .add_plugins((
            LandscapePlugin,
            ChunkManagementPlugin,
            ChunkInitializationPlugin,
            PropagationPlugin,
            MeshingPlugin,
        ))
        .add_systems(Update, (apply_deferred.in_set(WorldSet::FlushCommands),));
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
}
