use projekto_core::{chunk::Chunk, voxel};
use projekto_proto::prelude::*;
use projekto_proto_macros::message_source;

#[message_source(MessageSource::Server)]
pub enum ServerMessage {
    ChunkVertex {
        pub chunk: Chunk,
        pub vertex: Vec<voxel::Vertex>,
    },
}
