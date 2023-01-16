use bevy::{
    ecs::system::SystemParam,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    utils::{HashMap, HashSet},
};
use projekto_core::{chunk, landscape, query, voxel};
use projekto_genesis::{events::ChunkUpdated, ChunkKindRes};

use crate::world::{
    rendering::{ChunkMaterial, ChunkMaterialHandle},
    KindsAtlasRes,
};

use super::{ChunkBundle, ChunkEntityMap, ChunkLocal, EvtChunkMeshDirty, LandscapeCenter};

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

#[derive(Default, Resource)]
pub struct LandscapeConfig {
    pub paused: bool,
}

#[derive(Resource)]
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
        clip_map,
        show_back_faces: false,
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

#[derive(SystemParam)]
struct UpdateLandscapeParams<'w, 's> {
    kinds: Res<'w, ChunkKindRes>,
    meta: ResMut<'w, LandscapeMeta>,
    writer: EventWriter<'w, 's, EvtChunkMeshDirty>,
    material: Res<'w, ChunkMaterialHandle>,
    entity_map: ResMut<'w, ChunkEntityMap>,
    center_query: Query<'w, 's, &'static Transform, With<LandscapeCenter>>,
}

fn update_landscape(
    mut commands: Commands,
    time: Res<Time>,              // TODO: Change this to a Run Criteria later on
    config: Res<LandscapeConfig>, // TODO: Change this to a Run Criteria later on
    mut params: UpdateLandscapeParams,
) {
    if config.paused {
        return;
    }

    let center = match params.center_query.get_single() {
        Ok(t) => chunk::to_local(t.translation),
        Err(_) => return,
    };

    params.meta.next_sync -= time.delta_seconds();

    if center != params.meta.last_pos || params.meta.next_sync < 0.0 {
        params.meta.next_sync = 1.0;
        params.meta.last_pos = center;

        let radius = IVec3::new(
            landscape::HORIZONTAL_RADIUS as i32,
            0,
            landscape::HORIZONTAL_RADIUS as i32,
        );
        let begin = center - radius;
        let end = center + radius;

        let visible_locals = query::range_inclusive(begin, end).collect::<HashSet<_>>();
        let existing_locals = params.entity_map.0.keys().copied().collect::<HashSet<_>>();

        let spawn = visible_locals
            .iter()
            .filter(|&i| !existing_locals.contains(i))
            .filter(|&&i| params.kinds.exists(i))
            .collect::<Vec<_>>();

        if !spawn.is_empty() {
            debug!("Spawning {} chunks", spawn.len());
        }

        for &local in spawn.into_iter() {
            // Spawn chunks

            let entity = commands
                .spawn_bundle(ChunkBundle {
                    local: ChunkLocal(local),
                    mesh_bundle: MaterialMeshBundle {
                        material: params.material.clone(),
                        transform: Transform::from_translation(chunk::to_world(local)),
                        ..Default::default()
                    },
                })
                .insert(Name::new(format!("Chunk {}", local)))
                .id();
            params.entity_map.0.insert(local, entity);
            params.writer.send(EvtChunkMeshDirty(local));

            commands.entity(params.meta.root).add_child(entity);
        }

        let despawn = existing_locals
            .iter()
            .filter(|&i| !visible_locals.contains(i))
            .collect::<Vec<_>>();

        if !despawn.is_empty() {
            debug!("Despawning {} chunks", despawn.len());
        }

        for &local in despawn.into_iter() {
            if let Some(entity) = params.entity_map.0.remove(&local) {
                commands.entity(entity).despawn_recursive();
            }
        }
    }
}

fn process_chunk_updated_events(
    mut reader: EventReader<ChunkUpdated>,
    mut writer: EventWriter<EvtChunkMeshDirty>,
    entity_map: Res<ChunkEntityMap>,
) {
    for ChunkUpdated(chunk_local) in reader.iter() {
        if entity_map.0.get(chunk_local).is_some() {
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
        let mut added_events = Events::<ChunkUpdated>::default();
        added_events.send(ChunkUpdated((1, 2, 3).into()));

        let mut world = World::default();
        world.insert_resource(added_events);
        world.insert_resource(Events::<super::EvtChunkMeshDirty>::default());

        let mut entity_map = ChunkEntityMap(HashMap::default());
        entity_map
            .0
            .insert((1, 2, 3).into(), world.spawn(ChunkBundle::default()).id());
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
