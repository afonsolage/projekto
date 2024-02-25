use projekto_core::{chunk::Chunk, voxel};
use projekto_world_server_macros::message_source;

use super::MessageSource;

#[message_source(MessageSource::Server)]
pub enum ServerMessage {
    ChunkVertex {
        pub chunk: Chunk,
        pub vertex: Vec<voxel::Vertex>,
    },
}
