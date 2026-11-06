use bevy::prelude::*;
use net::NetPlugin;

// pub mod app;
pub mod light;
pub mod meshing;

mod cache;

pub use cache::*;
pub use projekto_archive::ChunkAsset;

pub mod net;

pub mod genesis;

pub mod bundle;
pub mod set;

/// Task pool options tuned for the server: give meshing most of the available threads.
pub fn server_task_pool_options() -> bevy::app::TaskPoolOptions {
    let mut options = bevy::app::TaskPoolOptions::default();
    let parallelism = std::thread::available_parallelism()
        .map(|threads| threads.get())
        .unwrap_or(1);

    options.async_compute.max_threads = parallelism.saturating_sub(2).max(1);
    options.async_compute.percent = 1.0;
    options
}

pub struct WorldServerPlugin;

impl Plugin for WorldServerPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(PreUpdate, WorldSet::ReceiveRequests)
            .configure_sets(
                Update,
                (
                    WorldSet::LandscapeUpdate,
                    WorldSet::ChunkManagement,
                    WorldSet::ChunkInitialization,
                    WorldSet::Propagation,
                    WorldSet::Meshing,
                )
                    .chain(),
            )
            .configure_sets(PostUpdate, WorldSet::SendResponses)
            .add_plugins((
                NetPlugin,
                set::ReceiveRequestsPlugin,
                set::LandscapePlugin,
                set::ChunkManagementPlugin,
                set::ChunkInitializationPlugin,
                set::PropagationPlugin,
                set::MeshingPlugin,
                set::SendResponsesPlugin,
            ));
    }
}

#[derive(SystemSet, Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub enum WorldSet {
    ReceiveRequests,
    LandscapeUpdate,
    ChunkManagement,
    ChunkInitialization,
    Propagation,
    Meshing,
    SendResponses,
}

#[cfg(test)]
mod test {
    use bevy::app::ScheduleRunnerPlugin;

    use super::*;

    #[test]
    fn plugin() {
        App::new()
            .add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_once()))
            .add_plugins(AssetPlugin::default())
            .add_plugins(super::WorldServerPlugin)
            .run();
    }
}
