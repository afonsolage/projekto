use bevy::{app::ScheduleRunnerPlugin, prelude::*};
use projekto_core::{
    chunk::{Chunk, ChunkStorage},
    voxel,
};

use crate::bundle::{ChunkBundle, ChunkKind, ChunkLocal, ChunkMap};

mod genesis;

#[derive(Debug, Default)]
pub(crate) struct GeneratedChunks {
    pub chunk: Chunk,
    pub kind: ChunkStorage<voxel::Kind>,
    pub light: ChunkStorage<voxel::Light>,
    pub occlusion: ChunkStorage<voxel::FacesOcclusion>,
    pub soft_light: ChunkStorage<voxel::FacesSoftLight>,
    pub vertex: Vec<voxel::Vertex>,
}

#[derive(Resource)]
struct Chunks(Vec<Chunk>);

pub(crate) fn setup_gen_app(chunks: Vec<Chunk>) -> App {
    let mut app = App::new();

    app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_once()));
    app.init_resource::<ChunkMap>();
    app.insert_resource(Chunks(chunks));

    app.add_systems(Update, chunks_gen);

    app
}

fn chunks_gen(mut commands: Commands, chunks: Res<Chunks>, mut chunk_map: ResMut<ChunkMap>) {
    let mut count = 0;
    for &chunk in &chunks.0 {
        let kind = genesis::generate_chunk(chunk);
        let entity = commands
            .spawn(ChunkBundle {
                kind: ChunkKind(kind),
                local: ChunkLocal(chunk),
                ..Default::default()
            })
            .id();

        let existing = chunk_map.insert(chunk, entity);
        debug_assert_eq!(existing, None, "Can't replace existing chunk {chunk}");
        count += 1;
    }
    trace!("[chunks_gen] {count} chunks generated and spawned.");
}

pub(crate) trait ExtractChunks {
    fn extract_chunks(&mut self) -> Vec<GeneratedChunks>;
}

impl ExtractChunks for App {
    fn extract_chunks(&mut self) -> Vec<GeneratedChunks> {
        let world = &mut self.world;

        let entities = world
            .query_filtered::<Entity, With<ChunkLocal>>()
            .iter(world)
            .collect::<Vec<_>>();

        entities
            .into_iter()
            .map(|entity| {
                let ChunkBundle {
                    kind,
                    light,
                    local,
                    occlusion,
                    soft_light,
                    vertex,
                } = world
                    .entity_mut(entity)
                    .take::<ChunkBundle>()
                    .expect("No components from bundle is removed");

                GeneratedChunks {
                    chunk: local.0,
                    kind: kind.0,
                    light: light.0,
                    occlusion: occlusion.0,
                    soft_light: soft_light.0,
                    vertex: vertex.0,
                }
            })
            .collect()
    }
}
