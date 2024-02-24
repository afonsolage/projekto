use bevy::prelude::*;
use projekto_core::chunk::Chunk;
use projekto_world_server_macros::message_source;

use super::MessageSource;

#[message_source]
pub enum ClientMessage {
    ChunkLoad { pub chunk: Chunk },
    LandscapeUpdate { pub center: IVec2, pub radius: u8 },
}

impl From<ClientMessage> for MessageSource {
    fn from(value: ClientMessage) -> Self {
        Self::Client(value)
    }
}
