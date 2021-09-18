use bevy::{
    prelude::*,
    render::{
        pipeline::{PipelineDescriptor, RenderPipeline},
        shader::ShaderStages,
    },
    utils::HashMap,
};

use crate::world::storage::{chunk, landscape};

use super::{
    genesis::CmdChunkLoad, ChunkBuildingBundle, ChunkBundle, ChunkEntityMap, ChunkLocal,
    ChunkPipeline, EvtChunkAdded, EvtChunkDirty, EvtChunkRemoved, EvtChunkUpdated,
};

pub(super) struct LandscapingPlugin;

impl Plugin for LandscapingPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<EvtChunkDirty>()
            .add_startup_system_to_stage(super::PipelineStartup::Landscaping, setup_resources)
            .add_startup_system_to_stage(super::PipelineStartup::Landscaping, setup_landscape)
            .add_system_set_to_stage(
                super::Pipeline::Landscaping,
                SystemSet::new()
                    .with_system(despawn_chunks_system.label("despawn"))
                    .with_system(spawn_chunks_system.label("spawn").after("despawn"))
                    .with_system(update_chunks_system.after("spawn")),
            );
    }
}

fn setup_landscape(mut writer: EventWriter<CmdChunkLoad>) {
    trace_system_run!();

    for x in landscape::BEGIN..landscape::END {
        for y in landscape::BEGIN..landscape::END {
            for z in landscape::BEGIN..landscape::END {
                let local = (x, y, z).into();
                let world = chunk::to_world(local);

                // TODO: How to generate for negative height chunks?
                if world.y < 0.0 {
                    continue;
                }

                writer.send(CmdChunkLoad(local));
            }
        }
    }
}

fn setup_resources(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut pipelines: ResMut<Assets<PipelineDescriptor>>,
) {
    trace_system_run!();

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
    mut commands: Commands,
    mut entity_map: ResMut<ChunkEntityMap>,
    chunk_pipeline: Res<ChunkPipeline>,
    mut reader: EventReader<EvtChunkAdded>,
    mut writer: EventWriter<EvtChunkDirty>,
) {
    let mut _perf = perf_fn!();
    for EvtChunkAdded(local) in reader.iter() {
        trace_system_run!(local);
        perf_scope!(_perf);

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
}

fn despawn_chunks_system(
    mut commands: Commands,
    mut entity_map: ResMut<ChunkEntityMap>,
    mut reader: EventReader<EvtChunkRemoved>,
) {
    let mut _perf = perf_fn!();

    for EvtChunkRemoved(local) in reader.iter() {
        trace_system_run!(local);
        perf_scope!(_perf);

        if let Some(entity) = entity_map.0.remove(local) {
            commands.entity(entity).despawn_recursive();
        }
    }
}

fn update_chunks_system(
    mut commands: Commands,
    mut reader: EventReader<EvtChunkUpdated>,
    mut writer: EventWriter<EvtChunkDirty>,
    entity_map: ResMut<ChunkEntityMap>,
) {
    let mut _perf = perf_fn!();

    for EvtChunkUpdated(chunk_local) in reader.iter() {
        if let Some(&entity) = entity_map.0.get(chunk_local) {
            trace_system_run!(chunk_local);
            perf_scope!(_perf);

            commands
                .entity(entity)
                .insert_bundle(ChunkBuildingBundle::default());
            writer.send(EvtChunkDirty(*chunk_local));
        }
    }
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
