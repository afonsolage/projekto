use bevy::prelude::*;
use projekto_core::{chunk::Chunk, voxel};
use projekto_proto::MessageSource;
use projekto_proto_macros::message_source;

#[message_source(MessageSource::Client)]
pub enum ClientMessage {
    ChunkLoad { pub chunk: Chunk },
    LandscapeUpdate { pub center: IVec2, pub radius: u8 },
}

#[message_source(MessageSource::Server)]
pub enum ServerMessage {
    #[no_copy]
    ChunkVertex {
        pub chunk: Chunk,
        pub vertex: Vec<voxel::Vertex>,
    },
}
