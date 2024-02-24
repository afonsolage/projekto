use projekto_core::{chunk::Chunk, voxel};
use projekto_world_server_macros::message_source;

use super::MessageSource;

impl From<ServerMessage> for MessageSource {
    fn from(value: ServerMessage) -> Self {
        Self::Server(value)
    }
}

#[message_source]
pub enum ServerMessage {
    ChunkVertex {
        pub chunk: Chunk,
        pub vertex: Vec<voxel::Vertex>,
    },
}
