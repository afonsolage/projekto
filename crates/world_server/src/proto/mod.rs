use std::any::Any;

use bevy::math::IVec2;
use projekto_core::{chunk::Chunk, voxel};
use serde::{Deserialize, Serialize};

#[derive(thiserror::Error, Debug)]
pub enum MessageError {
    #[error("Failed to downcast message: {0:?}")]
    Downcasting(MessageSource),
}

pub mod channel;
mod plugin;

pub(crate) use plugin::*;
pub use plugin::{has_messages, MessageQueue, WorldClientChannel};

#[derive(Debug)]
pub enum ClientMessage {
    ChunkLoad,
    LandscapeSpawn,
}

#[derive(Debug)]
pub enum ServerMessage {
    ChunkVertex,
}

#[derive(Debug)]
pub enum MessageSource {
    Client(ClientMessage),
    Server(ServerMessage),
}

impl From<ClientMessage> for MessageSource {
    fn from(value: ClientMessage) -> Self {
        Self::Client(value)
    }
}

impl From<ServerMessage> for MessageSource {
    fn from(value: ServerMessage) -> Self {
        Self::Server(value)
    }
}

pub type BoxedMessage = Box<dyn Message + Send + 'static>;

trait Downcast: Any {
    fn into_any(self: Box<Self>) -> Box<dyn Any>;
    fn as_any(&self) -> &dyn Any;
}

impl<T: Any> Downcast for T {
    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[allow(private_bounds)]
pub trait Message: Downcast + std::fmt::Debug {
    fn msg_source(&self) -> MessageSource;
}

impl dyn Message + Send {
    fn is<T: Message>(&self) -> bool {
        Downcast::as_any(self).is::<T>()
    }

    pub fn downcast<T: Message>(self: Box<Self>) -> Result<T, Box<Self>> {
        if self.is::<T>() {
            Ok(*Downcast::into_any(self).downcast::<T>().unwrap())
        } else {
            Err(self)
        }
    }

    // fn downcast_ref<T: Message>(&self) -> Option<&T> {
    //     Downcast::as_any(self).downcast_ref::<T>()
    // }
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
pub struct ChunkVertexNfy {
    pub chunk: Chunk,
    pub vertex: Vec<voxel::Vertex>,
}

impl Message for ChunkVertexNfy {
    fn msg_source(&self) -> MessageSource {
        ServerMessage::ChunkVertex.into()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LandscapeSpawnReq {
    pub center: IVec2,
    pub radius: u8,
}

impl Message for LandscapeSpawnReq {
    fn msg_source(&self) -> MessageSource {
        ClientMessage::LandscapeSpawn.into()
    }
}
