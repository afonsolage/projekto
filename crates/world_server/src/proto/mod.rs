use std::any::Any;

use bevy::{ecs::world::World, log::error, math::IVec2};
use projekto_core::{chunk::Chunk, voxel};
use serde::{Deserialize, Serialize};

#[derive(thiserror::Error, Debug)]
pub enum MessageError {
    #[error("Failed to downcast message: {0:?}")]
    Downcasting(MessageSource),
}

pub mod channel;
mod client;
mod plugin;

pub use client::*;
pub use plugin::WorldClientChannel;
pub(crate) use plugin::*;

#[derive(Debug, Hash, Eq, PartialEq)]
pub enum ServerMessage {
    ChunkVertex,
}

#[derive(Debug, Hash, Eq, PartialEq)]
pub enum MessageSource {
    Client(ClientMessage),
    Server(ServerMessage),
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
    fn get_handler<T: Message + Send + Sync + 'static>(
        world: &mut World,
    ) -> Option<&MessageHandlers<T>>
    where
        Self: Sized,
    {
        world.get_resource()
    }
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
