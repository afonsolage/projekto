use std::time::Duration;

use bevy::{prelude::*, time::common_conditions::on_timer};
use set::{
    ChunkInitializationPlugin, ChunkManagementPlugin, CollectDispatchPlugin, LandscapePlugin,
    MeshingPlugin, PropagationPlugin,
};

pub mod app;
mod light;
mod meshing;

pub mod cache;
pub mod gen;

pub mod bundle;
pub mod set;

const MESHING_TICK_MS: u64 = 500;

pub struct WorldServerPlugin;

impl Plugin for WorldServerPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(PreUpdate, WorldSet::CollectAsync)
            .configure_sets(
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
            .configure_sets(PostUpdate, WorldSet::DispatchAsync)
            .add_plugins((
                CollectDispatchPlugin,
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
pub enum WorldSet {
    CollectAsync,
    LandscapeUpdate,
    ChunkManagement,
    FlushCommands,
    ChunkInitialization,
    Propagation,
    Meshing,
    DispatchAsync,
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
