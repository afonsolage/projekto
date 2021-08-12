use bevy::prelude::*;

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_startup_system(setup).add_system(generate_chunk);
    }
}

fn setup(mut commands: Commands) {
    commands.spawn().insert(Chunk);
}

const CHUNK_AXIS_SIZE: usize = 16;
const CHUNK_BUFFER_SIZE: usize = CHUNK_AXIS_SIZE * CHUNK_AXIS_SIZE * CHUNK_AXIS_SIZE;

struct Chunk;

struct ChunkType([u8; CHUNK_BUFFER_SIZE]);

fn generate_chunk(mut commands: Commands, q: Query<Entity, (Added<Chunk>, Without<ChunkType>)>) {
    for e in q.iter() {
        //TODO: Generate the chunk based on noise. For now, just fill it all with 1
        commands.entity(e).insert(ChunkType([1; CHUNK_BUFFER_SIZE]));
    }
}


//struct ChunkVertex([])