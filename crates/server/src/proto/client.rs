use bevy::prelude::*;
use projekto_core::chunk::Chunk;
use projekto_proto::prelude::*;
use projekto_proto_macros::message_source;

#[message_source(MessageSource::Client)]
pub enum ClientMessage {
    ChunkLoad { pub chunk: Chunk },
    LandscapeUpdate { pub center: IVec2, pub radius: u8 },
}
