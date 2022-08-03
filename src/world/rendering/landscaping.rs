use bevy::{
    prelude::*,
    utils::{HashMap, HashSet},
};

use crate::{
    fly_by_camera::FlyByCamera,
    world::{
        query,
        rendering::{ChunkMaterial, ChunkMaterialHandle},
        storage::{chunk, landscape, voxel},
        terraformation::prelude::KindsAtlasRes,
    },
};

use super::{
    ChunkBundle, ChunkEntityMap, ChunkLocal, EvtChunkMeshDirty, EvtChunkUpdated, WorldRes,
};

pub(super) struct LandscapingPlugin;

impl Plugin for LandscapingPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<EvtChunkMeshDirty>()
            .add_plugin(MaterialPlugin::<ChunkMaterial>::default())
            .add_startup_system(setup_resources)
            .add_system(process_chunk_updated_events)
            .add_system(update_landscape);
    }
}

#[derive(Default)]
pub struct LandscapeConfig {
    pub paused: bool,
}

fn setup_resources(
    mut commands: Commands,
    mut materials: ResMut<Assets<ChunkMaterial>>,
    kinds_res: Res<KindsAtlasRes>,
) {
    trace_system_run!();
    let material = materials.add(ChunkMaterial {
        tile_texture_size: 1.0 / voxel::KindsDescs::get_or_init().count_tiles() as f32,
        texture: kinds_res.atlas.clone(),
    });

    commands.insert_resource(ChunkMaterialHandle(material));
    commands.insert_resource(ChunkEntityMap(HashMap::default()));
    commands.insert_resource(LandscapeConfig { paused: false })
}

#[derive(Default)]
struct UpdateLandscapeMeta {
    last_pos: IVec3,
    next_sync: f32,
}

fn update_landscape(
    mut commands: Commands,
    mut entity_map: ResMut<ChunkEntityMap>,
    material: Res<ChunkMaterialHandle>,
    time: Res<Time>,              // TODO: Change this to a Run Criteria later on
    config: Res<LandscapeConfig>, // TODO: Change this to a Run Criteria later on
    world_res: Res<WorldRes>,     // TODO: Change this to a Run Criteria later on
    mut meta: Local<UpdateLandscapeMeta>,
    mut writer: EventWriter<EvtChunkMeshDirty>,
    q: Query<&Transform, With<FlyByCamera>>, // TODO: Use a proper marker
) {
    let mut _perf = perf_fn!();

    if config.paused || !world_res.is_ready() {
        return;
    }

    let center = match q.get_single() {
        Ok(t) => chunk::to_local(t.translation),
        Err(_) => return,
    };

    meta.next_sync -= time.delta_seconds();

    if center != meta.last_pos || meta.next_sync < 0.0 {
        perf_scope!(_perf);
        meta.next_sync = 1.0;
        meta.last_pos = center;

        let radius = IVec3::new(
            landscape::HORIZONTAL_RADIUS as i32,
            0,
            landscape::HORIZONTAL_RADIUS as i32,
        );
        let begin = center - radius;
        let end = center + radius;

        let visible_locals = query::range_inclusive(begin, end).collect::<HashSet<_>>();
        let existing_locals = entity_map.0.keys().copied().collect::<HashSet<_>>();

        let spawn = visible_locals
            .iter()
            .filter(|&i| !existing_locals.contains(i))
            .filter(|&&i| world_res.exists(i))
            .collect::<Vec<_>>();

        if spawn.len() > 0 {
            debug!("Spawning {} chunks", spawn.len());
        }

        for &local in spawn.into_iter() {
            spawn_chunk(
                &mut commands,
                &mut entity_map,
                &material,
                &mut writer,
                local,
            )
        }

        let despawn = existing_locals
            .iter()
            .filter(|&i| !visible_locals.contains(i))
            .collect::<Vec<_>>();

        if despawn.len() > 0 {
            debug!("Despawning {} chunks", despawn.len());
        }

        for &local in despawn.into_iter() {
            despawn_chunk(&mut commands, &mut entity_map, local);
        }
    }
}

fn spawn_chunk(
    commands: &mut Commands,
    entity_map: &mut ChunkEntityMap,
    material: &ChunkMaterialHandle,
    writer: &mut EventWriter<EvtChunkMeshDirty>,
    local: IVec3,
) {
    perf_fn_scope!();

    let entity = commands
        .spawn_bundle(ChunkBundle {
            local: ChunkLocal(local),
            mesh_bundle: MaterialMeshBundle {
                material: material.0.clone(),
                transform: Transform::from_translation(chunk::to_world(local)),
                ..Default::default()
            },
        })
        .insert(Name::new(format!("Chunk {}", local)))
        .id();
    entity_map.0.insert(local, entity);
    writer.send(EvtChunkMeshDirty(local));
}

fn despawn_chunk(commands: &mut Commands, entity_map: &mut ChunkEntityMap, local: IVec3) {
    perf_fn_scope!();

    if let Some(entity) = entity_map.0.remove(&local) {
        commands.entity(entity).despawn_recursive();
    }
}

fn process_chunk_updated_events(
    mut reader: EventReader<EvtChunkUpdated>,
    mut writer: EventWriter<EvtChunkMeshDirty>,
    entity_map: Res<ChunkEntityMap>,
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
    use bevy::{ecs::event::Events, prelude::*, utils::HashMap};

    use super::*;

    #[test]
    fn update_chunks() {
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
        stage.add_system(super::process_chunk_updated_events);

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
