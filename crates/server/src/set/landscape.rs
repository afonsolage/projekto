use bevy::prelude::*;
use projekto_core::coords::Chunk;

use crate::{WorldSet, bundle::ChunkMap};

use super::{ChunkLoad, ChunkUnload};

pub(crate) struct LandscapePlugin;

impl Plugin for LandscapePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (update_landscape.run_if(resource_changed_or_removed::<Landscape>),)
                .in_set(WorldSet::LandscapeUpdate),
        );
    }
}

#[derive(Resource, Default, Debug, Clone, Copy, Reflect)]
pub struct Landscape {
    pub center: IVec2,
    pub radius: u8,
}

fn update_landscape(
    maybe_landscape: Option<Res<Landscape>>,
    chunk_map: Res<ChunkMap>,
    mut load_writer: EventWriter<ChunkLoad>,
    mut unload_writer: EventWriter<ChunkUnload>,
) {
    trace!("Updating landscape!");
    let new_landscape_chunks = {
        if let Some(landscape) = maybe_landscape {
            let radius = landscape.radius as i32;
            let center = landscape.center;
            let mut chunks = (-radius..=radius)
                .flat_map(|x| {
                    (-radius..=radius).map(move |z| Chunk::new(x + center.x, z + center.y))
                })
                .collect::<Vec<_>>();

            let center: Chunk = center.into();
            chunks.sort_by(|a, b| {
                let a_dist = (IVec2::from(*a) - IVec2::from(center)).length_squared();
                let b_dist = (IVec2::from(*b) - IVec2::from(center)).length_squared();
                a_dist.cmp(&b_dist)
            });

            chunks
        } else {
            vec![]
        }
    };

    let mut unloaded = 0;
    chunk_map
        .keys()
        .filter(|&c| !new_landscape_chunks.contains(c))
        .for_each(|&c| {
            unload_writer.write(ChunkUnload(c));
            unloaded += 1;
        });

    let mut loaded = 0;
    new_landscape_chunks
        .into_iter()
        .filter(|c| !chunk_map.contains_key(c))
        .for_each(|c| {
            load_writer.write(ChunkLoad(c));
            loaded += 1;
        });

    trace!("[update_landscape] Unloaded: {unloaded}, loaded: {loaded}");
}

#[cfg(test)]
mod tests {
    use bevy::app::ScheduleRunnerPlugin;

    use super::*;

    #[test]
    fn update_no_landscape() {
        // arrange
        let mut app = App::new();

        app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_once()))
            .init_resource::<ChunkMap>()
            .add_event::<ChunkLoad>()
            .add_event::<ChunkUnload>()
            .add_plugins(super::LandscapePlugin);

        // act
        app.update();

        // assert
        let load_events = app.world().get_resource::<Events<ChunkLoad>>().unwrap();
        let unload_events = app.world().get_resource::<Events<ChunkUnload>>().unwrap();

        assert!(load_events.is_empty(), "No entity should be loaded");
        assert!(unload_events.is_empty(), "No entity should be unloaded");
    }

    #[test]
    fn update_landscape_radius_1() {
        // arrange
        let mut app = App::new();

        app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_once()))
            .init_resource::<ChunkMap>()
            .add_event::<ChunkLoad>()
            .add_event::<ChunkUnload>()
            .add_plugins(super::LandscapePlugin);

        app.world_mut().insert_resource(Landscape {
            radius: 1,
            ..Default::default()
        });

        // act
        app.update();

        // assert
        let load_events = app.world().get_resource::<Events<ChunkLoad>>().unwrap();
        let unload_events = app.world().get_resource::<Events<ChunkUnload>>().unwrap();

        assert_eq!(load_events.len(), 9, "9 Chunks events should be load");
        assert!(unload_events.is_empty(), "No unload events should be sent");
    }

    #[test]
    fn update_landscape_zero() {
        // arrange
        let mut app = App::new();

        app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_once()))
            .init_resource::<ChunkMap>()
            .add_event::<ChunkLoad>()
            .add_event::<ChunkUnload>()
            .add_plugins(super::LandscapePlugin);

        app.world_mut().insert_resource(Landscape {
            radius: 0,
            ..Default::default()
        });

        // act
        app.update();

        // assert
        let load_events = app.world().get_resource::<Events<ChunkLoad>>().unwrap();
        let unload_events = app.world().get_resource::<Events<ChunkUnload>>().unwrap();

        assert_eq!(load_events.len(), 1, "1 Chunk event should be load");
        assert!(unload_events.is_empty(), "No unload events should be sent");
    }
}
