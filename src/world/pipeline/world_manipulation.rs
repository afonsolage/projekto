use bevy::prelude::*;
use bracket_noise::prelude::*;

use crate::{
    debug::PerfCounter,
    world::storage::{chunk, landscape, voxel, VoxWorld},
};

pub(super) struct WorldManipulationPlugin;

impl Plugin for WorldManipulationPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<CmdChunkAdd>()
            .add_event::<CmdChunkRemove>()
            .add_event::<CmdChunkUpdate>()
            .add_event::<EvtChunkAdded>()
            .add_event::<EvtChunkUpdated>()
            .add_event::<EvtChunkRemoved>()
            .add_startup_system_to_stage(super::PipelineStartup::WorldManipulation, setup_world)
            .add_system_set_to_stage(
                super::Pipeline::WorldManipulation,
                SystemSet::new()
                    .with_system(process_add_chunks_system.label("add"))
                    .with_system(process_remove_chunks_system.label("remove").after("add"))
                    .with_system(process_update_chunks_system.after("remove")),
            );
    }
}

#[derive(Clone)]
pub struct CmdChunkAdd(pub IVec3, pub Vec<(IVec3, voxel::Kind)>);

#[derive(Clone, Copy)]
pub struct CmdChunkRemove(pub IVec3);

#[derive(Clone)]
pub struct CmdChunkUpdate(pub IVec3, pub Vec<(IVec3, voxel::Kind)>);

#[derive(Clone, Copy)]
pub struct EvtChunkAdded(pub IVec3);

#[derive(Clone, Copy)]
pub struct EvtChunkRemoved(pub IVec3);

#[derive(Clone, Copy)]
pub struct EvtChunkUpdated(pub IVec3);

fn setup_world(mut commands: Commands, mut writer: EventWriter<CmdChunkAdd>) {
    commands.insert_resource(VoxWorld::default());

    let mut perf_counter = PerfCounter::new("Setup World");

    // TODO: Find a better place for this initialization
    for x in landscape::BEGIN..landscape::END {
        for y in landscape::BEGIN..landscape::END {
            for z in landscape::BEGIN..landscape::END {
                let local = (x, y, z).into();
                let world = chunk::to_world(local);

                // TODO: How to generate for negative height chunks?
                if world.y < 0.0 {
                    continue;
                }

                let _perf = perf_counter.measure();

                let mut noise = FastNoise::seeded(15);
                noise.set_noise_type(NoiseType::SimplexFractal);
                noise.set_frequency(0.03);
                noise.set_fractal_type(FractalType::FBM);
                noise.set_fractal_octaves(3);
                noise.set_fractal_gain(0.9);
                noise.set_fractal_lacunarity(0.5);

                let mut voxels = vec![];

                for x in 0..chunk::AXIS_SIZE {
                    for z in 0..chunk::AXIS_SIZE {
                        let h = noise.get_noise(world.x + x as f32, world.z + z as f32);
                        let world_height = ((h + 1.0) / 2.0) * (2 * chunk::AXIS_SIZE) as f32;

                        let height_local = world_height - world.y;

                        if height_local < f32::EPSILON {
                            continue;
                        }

                        let end = usize::min(height_local as usize, chunk::AXIS_SIZE);

                        for y in 0..end {
                            voxels.push(((x as i32, y as i32, z as i32).into(), 1.into()));
                        }
                    }
                }

                if !voxels.is_empty() {
                    writer.send(CmdChunkAdd(local, voxels));
                }
            }
        }
    }

    perf_counter.calc_meta();
    info!("{}", perf_counter);
}

fn process_add_chunks_system(
    mut world: ResMut<VoxWorld>,
    mut reader: EventReader<CmdChunkAdd>,
    mut writer: EventWriter<EvtChunkAdded>,
) {
    for CmdChunkAdd(local, voxels) in reader.iter() {
        trace!("Adding chunk {} to world", *local);
        world.add(*local);
        let chunk = world.get_mut(*local).unwrap();

        for &(voxel, kind) in voxels {
            chunk.set(voxel, kind);
        }

        writer.send(EvtChunkAdded(*local));
    }
}

fn process_remove_chunks_system(
    mut world: ResMut<VoxWorld>,
    mut reader: EventReader<CmdChunkRemove>,
    mut writer: EventWriter<EvtChunkRemoved>,
) {
    for CmdChunkRemove(local) in reader.iter() {
        trace!("Removing chunk {} from world", *local);
        world.remove(*local);
        writer.send(EvtChunkRemoved(*local));
    }
}

fn process_update_chunks_system(
    mut world: ResMut<VoxWorld>,
    mut reader: EventReader<CmdChunkUpdate>,
    mut writer: EventWriter<EvtChunkUpdated>,
) {
    for CmdChunkUpdate(chunk_local, voxels) in reader.iter() {
        let chunk = match world.get_mut(*chunk_local) {
            None => {
                warn!(
                    "Skipping update on {} since the chunk doesn't exists",
                    *chunk_local
                );
                continue;
            }
            Some(c) => c,
        };

        trace!("Update chunk {} in world ({:?})", *chunk_local, &voxels);

        let mut neighbor_chunks = vec![];

        for (voxel, kind) in voxels {
            chunk.set(*voxel, *kind);
        }

        drop(chunk);

        for (voxel, _) in voxels {
            if chunk::is_at_bounds(*voxel) {
                let dir = chunk::get_boundary_dir(*voxel);
                let neighbor_chunk = *chunk_local + dir;

                if world.get(neighbor_chunk).is_some() {
                    neighbor_chunks.push(neighbor_chunk);
                }
            }
        }

        debug!("Updating chunk {}", *chunk_local);
        writer.send(EvtChunkUpdated(*chunk_local));

        for neighbor in neighbor_chunks {
            debug!("Notifying neighbor chunk {}", neighbor);
            writer.send(EvtChunkUpdated(neighbor));
        }
    }
}

#[cfg(test)]
mod test {
    use bevy::{
        app::Events,
        prelude::{self, *},
    };

    use crate::world::{
        pipeline::{CmdChunkRemove, EvtChunkRemoved},
        storage,
    };

    use super::*;

    #[test]
    fn process_add_chunks_system() {
        // Arrange
        let mut events = Events::<CmdChunkAdd>::default();
        events.send(CmdChunkAdd((1, 2, 3).into(), vec![]));

        let mut world = prelude::World::default();
        world.insert_resource(storage::VoxWorld::default());
        world.insert_resource(events);
        world.insert_resource(Events::<EvtChunkAdded>::default());

        let mut stage = SystemStage::parallel();
        stage.add_system(super::process_add_chunks_system);

        // Act
        stage.run(&mut world);

        // Assert
        assert!(world
            .get_resource::<storage::VoxWorld>()
            .unwrap()
            .get((1, 2, 3).into())
            .is_some());

        assert_eq!(
            world
                .get_resource_mut::<Events::<EvtChunkAdded>>()
                .unwrap()
                .iter_current_update_events()
                .next()
                .unwrap()
                .0,
            (1, 2, 3).into()
        );
    }

    #[test]
    fn process_remove_chunks_system() {
        // Arrange
        let mut events = Events::<CmdChunkRemove>::default();
        events.send(CmdChunkRemove((1, 2, 3).into()));

        let mut voxel_world = storage::VoxWorld::default();
        voxel_world.add((1, 2, 3).into());

        let mut world = prelude::World::default();
        world.insert_resource(voxel_world);
        world.insert_resource(events);
        world.insert_resource(Events::<EvtChunkRemoved>::default());

        let mut stage = SystemStage::parallel();
        stage.add_system(super::process_remove_chunks_system);

        // Act
        stage.run(&mut world);

        // Assert
        assert!(!world
            .get_resource::<storage::VoxWorld>()
            .unwrap()
            .get((1, 2, 3).into())
            .is_some());

        assert_eq!(
            world
                .get_resource_mut::<Events::<EvtChunkRemoved>>()
                .unwrap()
                .iter_current_update_events()
                .next()
                .unwrap()
                .0,
            (1, 2, 3).into()
        );
    }

    #[test]
    fn process_update_chunks_system() {
        // Arrange
        let mut events = Events::<CmdChunkUpdate>::default();
        events.send(CmdChunkUpdate(
            (1, 2, 3).into(),
            vec![(IVec3::ONE, 2.into())],
        ));

        let mut voxel_world = storage::VoxWorld::default();
        voxel_world.add((1, 2, 3).into());

        let mut world = prelude::World::default();
        world.insert_resource(voxel_world);
        world.insert_resource(events);
        world.insert_resource(Events::<EvtChunkUpdated>::default());

        let mut stage = SystemStage::parallel();
        stage.add_system(super::process_update_chunks_system);

        // Act
        stage.run(&mut world);

        // Assert
        assert_eq!(
            world
                .get_resource::<storage::VoxWorld>()
                .unwrap()
                .get((1, 2, 3).into())
                .unwrap()
                .get(IVec3::ONE),
            2.into()
        );

        let evt = world
            .get_resource_mut::<Events<EvtChunkUpdated>>()
            .unwrap()
            .iter_current_update_events()
            .next()
            .unwrap()
            .clone();

        assert_eq!(evt.0, (1, 2, 3).into());
    }
}
