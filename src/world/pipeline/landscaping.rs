use std::collections::VecDeque;

use bevy::{
    core::FixedTimestep,
    prelude::*,
    render::{
        pipeline::{PipelineDescriptor, RenderPipeline},
        shader::ShaderStages,
    },
    utils::HashMap,
};

use crate::{
    fly_by_camera::FlyByCamera,
    world::{
        pipeline::genesis::{CmdChunkUnload, EvtChunkLoaded, EvtChunkUnloaded},
        query,
        storage::{chunk, landscape},
    },
};

use super::{
    genesis::CmdChunkLoad, ChunkBundle, ChunkEntityMap, ChunkLocal, ChunkPipeline,
    EvtChunkMeshDirty, EvtChunkUpdated,
};

pub(super) struct LandscapingPlugin;

impl Plugin for LandscapingPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<EvtChunkMeshDirty>()
            .add_startup_system_to_stage(super::PipelineStartup::Landscaping, setup_resources)
            // .add_startup_system_to_stage(super::PipelineStartup::Landscaping, setup_landscape)
            .add_system_set_to_stage(
                super::Pipeline::Landscaping,
                SystemSet::new()
                    .with_system(despawn_chunks_system.label("despawn"))
                    .with_system(spawn_chunks_system.label("spawn").after("despawn"))
                    .with_system(update_chunks_system.after("spawn"))
                    .with_system(
                        update_landscape_system
                            .label("update")
                            .with_run_criteria(FixedTimestep::step(0.1)),
                    ),
            );
    }
}

#[derive(Default)]
pub struct LandscapeConfig {
    pub paused: bool,
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
    commands.insert_resource(LandscapeConfig::default())
}

#[derive(Default)]
struct UpdateLandscapeMeta {
    load_queue: VecDeque<IVec3>,
    unload_queue: VecDeque<IVec3>,
    last_pos: IVec3,
    next_sync: f32,
    pending_load: Vec<IVec3>,
    pending_unload: Vec<IVec3>,
}

fn update_landscape_system(
    time: Res<Time>,
    entity_map: ResMut<ChunkEntityMap>,
    config: Res<LandscapeConfig>,
    mut load_writer: EventWriter<CmdChunkLoad>,
    mut unload_writer: EventWriter<CmdChunkUnload>,
    mut loaded_reader: EventReader<EvtChunkLoaded>,
    mut unloaded_reader: EventReader<EvtChunkUnloaded>,
    mut meta: Local<UpdateLandscapeMeta>,
    q: Query<&Transform, With<FlyByCamera>>,
) {
    let mut _perf = perf_fn!();
    perf_scope!(_perf);

    for EvtChunkLoaded(local) in loaded_reader.iter() {
        meta.pending_load.retain(|v| v != local);
    }

    for EvtChunkUnloaded(local) in unloaded_reader.iter() {
        meta.pending_unload.retain(|v| v != local);
    }

    if config.paused {
        return;
    }

    let center = match q.get_single() {
        Ok(t) => chunk::to_local(t.translation),
        Err(_) => return,
    };

    meta.next_sync -= time.delta_seconds();

    if center != meta.last_pos || meta.next_sync < 0.0 {
        meta.next_sync = 1.0;
        meta.last_pos = center;

        debug!("Updating landscape to center {}", center);

        let begin = center + IVec3::splat(landscape::BEGIN);
        let end = center + IVec3::splat(landscape::END);

        let visible_locals = query::range(begin, end).collect::<Vec<_>>();
        let existing_locals = entity_map.0.keys().map(|k| *k).collect::<Vec<_>>();

        let (to_load, to_unload) = disjoin(&visible_locals, &existing_locals);

        for v in to_load {
            if !meta.load_queue.contains(v) && !meta.pending_load.contains(v) {
                meta.load_queue.push_back(*v);
            }

            if meta.unload_queue.contains(v) {
                meta.unload_queue.retain(|uv| uv != v);
            }
        }

        for v in to_unload {
            if !meta.unload_queue.contains(v) && !meta.pending_unload.contains(v) {
                meta.unload_queue.push_back(*v);
            }

            if meta.load_queue.contains(v) {
                meta.load_queue.retain(|uv| uv != v);
            }
        }
    }

    while let Some(next) = meta.load_queue.pop_front() {
        load_writer.send(CmdChunkLoad(next));
        meta.pending_load.push(next);
    }

    while let Some(next) = meta.unload_queue.pop_front() {
        meta.pending_unload.push(next);
        unload_writer.send(CmdChunkUnload(next));
    }
}

fn disjoin<'a>(
    set_a: &'a [IVec3],
    set_b: &'a [IVec3],
) -> (
    impl Iterator<Item = &'a IVec3>,
    impl Iterator<Item = &'a IVec3>,
) {
    (
        set_a.iter().filter(move |v| !set_b.contains(v)),
        set_b.iter().filter(move |v| !set_a.contains(v)),
    )
}

fn spawn_chunks_system(
    mut commands: Commands,
    mut entity_map: ResMut<ChunkEntityMap>,
    chunk_pipeline: Res<ChunkPipeline>,
    mut reader: EventReader<EvtChunkLoaded>,
    mut writer: EventWriter<EvtChunkMeshDirty>,
) {
    let mut _perf = perf_fn!();
    for EvtChunkLoaded(local) in reader.iter() {
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
        writer.send(EvtChunkMeshDirty(*local));
    }
}

fn despawn_chunks_system(
    mut commands: Commands,
    mut entity_map: ResMut<ChunkEntityMap>,
    mut reader: EventReader<EvtChunkUnloaded>,
) {
    let mut _perf = perf_fn!();

    for EvtChunkUnloaded(local) in reader.iter() {
        trace_system_run!(local);
        perf_scope!(_perf);

        if let Some(entity) = entity_map.0.remove(local) {
            commands.entity(entity).despawn_recursive();
        }
    }
}

fn update_chunks_system(
    mut reader: EventReader<EvtChunkUpdated>,
    mut writer: EventWriter<EvtChunkMeshDirty>,
    entity_map: ResMut<ChunkEntityMap>,
) {
    let mut _perf = perf_fn!();

    for EvtChunkUpdated(chunk_local) in reader.iter() {
        if entity_map.0.get(chunk_local).is_some() {
            trace_system_run!(chunk_local);
            perf_scope!(_perf);
            writer.send(EvtChunkMeshDirty(*chunk_local));
        }
    }
}

#[cfg(test)]
mod test {
    use std::vec;

    use bevy::{app::Events, prelude::*, utils::HashMap};

    use crate::world::pipeline::{
        genesis::EvtChunkUnloaded, ChunkBundle, ChunkLocal, ChunkPipeline, EvtChunkMeshDirty,
    };

    use super::{ChunkEntityMap, EvtChunkLoaded, EvtChunkUpdated};

    #[test]
    fn disjoint() {
        let a = vec![
            (0, 0, 0).into(),
            (1, 1, 1).into(),
            (2, 2, 2).into(),
            (3, 3, 3).into(),
        ];
        let b = vec![
            (0, 0, 0).into(),
            (1, 1, 1).into(),
            (2, 2, 3).into(),
            (3, 3, 4).into(),
        ];

        let (d_a, d_b) = super::disjoin(&a, &b);

        let disjoint_a = d_a.map(|v| *v).collect::<Vec<_>>();
        let disjoint_b = d_b.map(|v| *v).collect::<Vec<_>>();

        assert_eq!(disjoint_a, vec![(2, 2, 2).into(), (3, 3, 3).into()]);
        assert_eq!(disjoint_b, vec![(2, 2, 3).into(), (3, 3, 4).into()]);
    }

    #[test]
    fn spawn_chunks_system() {
        // Arrange
        let mut added_events = Events::<EvtChunkLoaded>::default();
        added_events.send(EvtChunkLoaded(IVec3::ONE));

        let mut world = World::default();
        world.insert_resource(ChunkEntityMap(HashMap::default()));
        world.insert_resource(added_events);
        world.insert_resource(Events::<EvtChunkMeshDirty>::default());
        world.insert_resource(ChunkPipeline(Handle::default()));

        let mut stage = SystemStage::parallel();
        stage.add_system(super::spawn_chunks_system);

        // Act
        stage.run(&mut world);

        // Assert
        assert_eq!(
            world
                .get_resource::<Events<EvtChunkMeshDirty>>()
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
        let mut added_events = Events::<EvtChunkUnloaded>::default();
        added_events.send(EvtChunkUnloaded(IVec3::ONE));

        let mut world = World::default();
        world.insert_resource(added_events);
        world.insert_resource(Events::<super::EvtChunkMeshDirty>::default());

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
        world.insert_resource(Events::<super::EvtChunkMeshDirty>::default());

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
                .get_resource::<Events<EvtChunkMeshDirty>>()
                .unwrap()
                .iter_current_update_events()
                .next()
                .unwrap()
                .0,
            (1, 2, 3).into()
        );
    }
}
