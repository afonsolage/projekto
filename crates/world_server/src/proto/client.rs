use bevy::prelude::*;
use projekto_core::chunk::Chunk;
use serde::{Deserialize, Serialize};

use super::{Message, MessageSource};

#[derive(Debug, Hash, Eq, PartialEq)]
pub enum ClientMessage {
    ChunkLoad,
    LandscapeUpdate,
}

impl From<ClientMessage> for MessageSource {
    fn from(value: ClientMessage) -> Self {
        Self::Client(value)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChunkLoadReq {
    pub chunk: Chunk,
}

impl Message for ChunkLoadReq {
    fn msg_source(&self) -> MessageSource {
        ClientMessage::ChunkLoad.into()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LandscapeUpdate {
    pub center: IVec2,
    pub radius: u8,
}

impl Message for LandscapeUpdate {
    fn msg_source(&self) -> MessageSource {
        ClientMessage::LandscapeUpdate.into()
    }
}
