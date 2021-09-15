use bevy::{
    prelude::*,
    render::{
        pipeline::{PipelineDescriptor, RenderPipeline},
        shader::ShaderStages,
    },
    utils::HashMap,
};

use crate::{
    debug::{PerfCounter, PerfCounterRes},
    world::storage::chunk,
};

use super::{
    ChunkBuildingBundle, ChunkBundle, ChunkEntityMap, ChunkLocal, ChunkPipeline, EvtChunkAdded,
    EvtChunkDirty, EvtChunkRemoved, EvtChunkUpdated,
};

pub(super) struct EntityManagingPlugin;

impl Plugin for EntityManagingPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<EvtChunkDirty>()
            .add_startup_system_to_stage(super::PipelineStartup::EntityManaging, setup_resources)
            .add_system_set_to_stage(
                super::Pipeline::EntityManaging,
                SystemSet::new()
                    .with_system(despawn_chunks_system.label("despawn"))
                    .with_system(spawn_chunks_system.label("spawn").after("despawn"))
                    .with_system(update_chunks_system.after("spawn")),
            );
    }
}

fn setup_resources(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut pipelines: ResMut<Assets<PipelineDescriptor>>,
) {
    let pipeline_handle = pipelines.add(PipelineDescriptor {
        // primitive: PrimitiveState {
        //     topology: PrimitiveTopology::TriangleList,
        //     strip_index_format: None,
        //     front_face: FrontFace::Ccw,
        //     cull_mode: Some(Face::Back),
        //     polygon_mode: PolygonMode::Fill,
        //     clamp_depth: false,
        //     conservative: false,
        // },
        ..PipelineDescriptor::default_config(ShaderStages {
            vertex: asset_server.load("shaders/voxel.vert"),
            fragment: Some(asset_server.load("shaders/voxel.frag")),
        })
    });

    commands.insert_resource(ChunkPipeline(pipeline_handle));
    commands.insert_resource(ChunkEntityMap(HashMap::default()));
}

fn spawn_chunks_system(
    perf_res: Res<PerfCounterRes>,
    mut commands: Commands,
    mut entity_map: ResMut<ChunkEntityMap>,
    chunk_pipeline: Res<ChunkPipeline>,
    mut reader: EventReader<EvtChunkAdded>,
    mut writer: EventWriter<EvtChunkDirty>,
) {
    let mut perf_counter = PerfCounter::new("Spawn Chunks");

    for EvtChunkAdded(local) in reader.iter() {
        let _perf = perf_counter.measure();

        trace!("Spawning chunk entity {}", *local);

        let entity = commands
            .spawn_bundle(ChunkBundle {
                local: ChunkLocal(*local),
                mesh_bundle: MeshBundle {
                    render_pipelines: RenderPipelines::from_pipelines(vec![RenderPipeline::new(
                        chunk_pipeline.0.clone(),
                    )]),
                    transform: Transform::from_translation(chunk::to_world(*local)),
                    ..Default::default()
                },
                ..Default::default()
            })
            .id();
        entity_map.0.insert(*local, entity);
        writer.send(EvtChunkDirty(*local));
    }

    perf_counter.calc_meta();
    perf_res.lock().unwrap().add(perf_counter);
}

fn despawn_chunks_system(
    perf_res: Res<PerfCounterRes>,
    mut commands: Commands,
    mut entity_map: ResMut<ChunkEntityMap>,
    mut reader: EventReader<EvtChunkRemoved>,
) {
    let mut perf_counter = PerfCounter::new("Despawn Chunks");

    for EvtChunkRemoved(local) in reader.iter() {
        let _perf = perf_counter.measure();

        if let Some(entity) = entity_map.0.remove(local) {
            trace!("Despawning chunk entity {}", *local);
            commands.entity(entity).despawn_recursive();
        }
    }

    perf_counter.calc_meta();
    perf_res.lock().unwrap().add(perf_counter);
}

fn update_chunks_system(
    perf_res: Res<PerfCounterRes>,
    mut commands: Commands,
    mut reader: EventReader<EvtChunkUpdated>,
    mut writer: EventWriter<EvtChunkDirty>,
    entity_map: ResMut<ChunkEntityMap>,
) {
    let mut perf_counter = PerfCounter::new("Update Chunks");

    for EvtChunkUpdated(chunk_local) in reader.iter() {
        if let Some(&entity) = entity_map.0.get(chunk_local) {
            let _perf = perf_counter.measure();

            trace!("Updating chunk entity {}", *chunk_local);
            commands
                .entity(entity)
                .insert_bundle(ChunkBuildingBundle::default());
            writer.send(EvtChunkDirty(*chunk_local));
        }
    }
    perf_counter.calc_meta();
    perf_res.lock().unwrap().add(perf_counter);
}

#[cfg(test)]
mod test {
    use bevy::{app::Events, prelude::*, utils::HashMap};

    use crate::world::pipeline::{
        ChunkBundle, ChunkLocal, ChunkPipeline, EvtChunkDirty, EvtChunkRemoved,
    };

    use super::{ChunkEntityMap, EvtChunkAdded, EvtChunkUpdated};

    #[test]
    fn spawn_chunks_system() {
        // Arrange
        let mut added_events = Events::<EvtChunkAdded>::default();
        added_events.send(EvtChunkAdded(IVec3::ONE));

        let mut world = World::default();
        world.insert_resource(ChunkEntityMap(HashMap::default()));
        world.insert_resource(added_events);
        world.insert_resource(Events::<EvtChunkDirty>::default());
        world.insert_resource(ChunkPipeline(Handle::default()));

        let mut stage = SystemStage::parallel();
        stage.add_system(super::spawn_chunks_system);

        // Act
        stage.run(&mut world);

        // Assert
        assert_eq!(
            world
                .get_resource::<Events<EvtChunkDirty>>()
                .unwrap()
                .iter_current_update_events()
                .next()
                .unwrap()
                .0,
            IVec3::ONE
        );

        assert_eq!(world.query::<&ChunkLocal>().iter(&world).len(), 1);
    }

    #[test]
    fn despawn_chunks_system() {
        // Arrange
        let mut added_events = Events::<EvtChunkRemoved>::default();
        added_events.send(EvtChunkRemoved(IVec3::ONE));

        let mut world = World::default();
        world.insert_resource(added_events);
        world.insert_resource(Events::<super::EvtChunkDirty>::default());

        let entity = world
            .spawn()
            .insert_bundle(ChunkBundle {
                local: ChunkLocal(IVec3::ONE),
                ..Default::default()
            })
            .id();

        let mut entity_map = ChunkEntityMap(HashMap::default());
        entity_map.0.insert(IVec3::ONE, entity);
        world.insert_resource(entity_map);

        let mut stage = SystemStage::parallel();
        stage.add_system(super::despawn_chunks_system);

        // Act
        stage.run(&mut world);

        // Assert
        assert_eq!(world.query::<&ChunkLocal>().iter(&world).len(), 0);
        assert!(world.get_resource::<ChunkEntityMap>().unwrap().0.is_empty());
    }

    #[test]
    fn update_chunks_system() {
        // Arrange
        let mut added_events = Events::<EvtChunkUpdated>::default();
        added_events.send(EvtChunkUpdated((1, 2, 3).into()));

        let mut world = World::default();
        world.insert_resource(added_events);
        world.insert_resource(Events::<super::EvtChunkDirty>::default());

        let mut entity_map = ChunkEntityMap(HashMap::default());
        entity_map.0.insert(
            (1, 2, 3).into(),
            world.spawn().insert_bundle(ChunkBundle::default()).id(),
        );
        world.insert_resource(entity_map);

        let mut stage = SystemStage::parallel();
        stage.add_system(super::update_chunks_system);

        // Act
        stage.run(&mut world);

        // Assert
        assert_eq!(
            world
                .get_resource::<Events<EvtChunkDirty>>()
                .unwrap()
                .iter_current_update_events()
                .next()
                .unwrap()
                .0,
            (1, 2, 3).into()
        );
    }
}
