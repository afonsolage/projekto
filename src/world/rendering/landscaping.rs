use bevy::{
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    utils::{HashMap, HashSet},
};
use projekto_core::{chunk, landscape, query, voxel};

use crate::world::{
    rendering::{ChunkMaterial, ChunkMaterialHandle},
    terraformation::prelude::KindsAtlasRes,
};

use super::{
    ChunkBundle, ChunkEntityMap, ChunkLocal, EvtChunkMeshDirty, EvtChunkUpdated, LandscapeCenter,
    WorldRes,
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

struct LandscapeMeta {
    root: Entity,
    last_pos: IVec3,
    next_sync: f32,
}

fn setup_resources(
    mut commands: Commands,
    mut materials: ResMut<Assets<ChunkMaterial>>,
    mut images: ResMut<Assets<Image>>,
    kinds_res: Res<KindsAtlasRes>,
) {
    trace_system_run!();

    const WIDTH: usize = landscape::HORIZONTAL_SIZE * chunk::X_AXIS_SIZE;
    const HEIGHT: usize = landscape::HORIZONTAL_SIZE * chunk::Z_AXIS_SIZE;
    let clip_map = images.add(Image::new(
        Extent3d {
            width: (WIDTH * HEIGHT) as u32,
            height: 1u32,
            ..Default::default()
        },
        TextureDimension::D1,
        vec![0; WIDTH * HEIGHT],
        TextureFormat::R8Uint,
    ));

    let material = materials.add(ChunkMaterial {
        texture: kinds_res.atlas.clone(),
        tile_texture_size: 1.0 / voxel::KindsDescs::get().count_tiles() as f32,
        clip_map_origin: Vec2::ZERO,
        clip_height: f32::MAX,
        clip_map: clip_map,
    });

    commands.insert_resource(ChunkMaterialHandle(material));
    commands.insert_resource(ChunkEntityMap(HashMap::default()));
    commands.insert_resource(LandscapeConfig { paused: false });

    let root = commands
        .spawn_bundle(SpatialBundle::default())
        .insert(Name::new("Landscape"))
        .id();
    commands.insert_resource(LandscapeMeta {
        root,
        last_pos: default(),
        next_sync: default(),
    });
}

#[derive(Default)]
struct UpdateLandscapeMeta {}

fn update_landscape(
    mut commands: Commands,
    mut entity_map: ResMut<ChunkEntityMap>,
    material: Res<ChunkMaterialHandle>,
    time: Res<Time>,              // TODO: Change this to a Run Criteria later on
    config: Res<LandscapeConfig>, // TODO: Change this to a Run Criteria later on
    world_res: Res<WorldRes>,     // TODO: Change this to a Run Criteria later on
    mut meta: ResMut<LandscapeMeta>,
    mut writer: EventWriter<EvtChunkMeshDirty>,
    center_query: Query<&Transform, With<LandscapeCenter>>,
) {
    let mut _perf = perf_fn!();

    if config.paused || !world_res.is_ready() {
        return;
    }

    let center = match center_query.get_single() {
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
            // Spawn chunks

            let entity = commands
                .spawn_bundle(ChunkBundle {
                    local: ChunkLocal(local),
                    mesh_bundle: MaterialMeshBundle {
                        material: material.clone(),
                        transform: Transform::from_translation(chunk::to_world(local)),
                        ..Default::default()
                    },
                })
                .insert(Name::new(format!("Chunk {}", local)))
                .id();
            entity_map.0.insert(local, entity);
            writer.send(EvtChunkMeshDirty(local));

            commands.entity(meta.root).add_child(entity);
        }

        let despawn = existing_locals
            .iter()
            .filter(|&i| !visible_locals.contains(i))
            .collect::<Vec<_>>();

        if despawn.len() > 0 {
            debug!("Despawning {} chunks", despawn.len());
        }

        for &local in despawn.into_iter() {
            if let Some(entity) = entity_map.0.remove(&local) {
                commands.entity(entity).despawn_recursive();
            }
        }
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
