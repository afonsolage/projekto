use bevy::{app::ScheduleRunnerPlugin, prelude::*};
use projekto_core::chunk::Chunk;

use crate::{
    bundle::{ChunkBundle, ChunkKind, ChunkLocal, ChunkMap},
    cache::ChunkCache,
};

mod genesis;

#[derive(Resource)]
struct Chunks(Vec<Chunk>);

pub fn setup_gen_app(chunks: Vec<Chunk>) -> App {
    let mut app = App::new();

    app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_once()));
    app.init_resource::<ChunkMap>();
    app.insert_resource(Chunks(chunks));

    app.add_systems(Update, chunks_gen);
    app.add_systems(Last, save_chunks);

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
            .insert(Name::new(format!("Server Chunk {chunk}")))
            .id();

        let existing = chunk_map.insert(chunk, entity);
        debug_assert_eq!(existing, None, "Can't replace existing chunk {chunk}");
        count += 1;
    }
    trace!("[chunks_gen] {count} chunks generated and spawned.");
}

#[allow(clippy::type_complexity)]
fn save_chunks(world: &mut World) {
    let entities = world.query::<Entity>().iter(world).collect::<Vec<_>>();

    for entity in entities {
        let ChunkBundle {
            kind,
            light,
            local,
            occlusion,
            soft_light,
            vertex,
        } = world
            .entity_mut(entity)
            .remove::<ChunkBundle>()
            .take::<ChunkBundle>()
            .expect("No components from bundle is removed");

        let cache = ChunkCache {
            chunk: local.0,
            kind: kind.0,
            light: light.0,
            occlusion: occlusion.0,
            soft_light: soft_light.0,
            vertex: vertex.0,
        };

        cache.save();
    }

    //
}
