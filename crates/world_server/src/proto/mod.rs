use std::any::Any;

use bevy::math::IVec2;
use projekto_core::{chunk::Chunk, voxel};
use serde::{Deserialize, Serialize};

#[derive(thiserror::Error, Debug)]
pub enum MessageError {
    #[error("Failed to downcast message: {0:?}")]
    Downcasting(MessageType),
}

pub mod channel;
mod plugin;

pub use plugin::WorldClientChannel;
pub(crate) use plugin::*;

#[derive(Debug)]
pub enum MessageType {
    ChunkLoadReq,
    ChunkVertexNfy,
    LandscapeSpawnReq,
}

pub type BoxedMessasing = Box<dyn Message + Send + 'static>;

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
    fn msg_type(&self) -> MessageType;
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
    fn msg_type(&self) -> MessageType {
        MessageType::ChunkLoadReq
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChunkVertexNfy {
    pub chunk: Chunk,
    pub vertex: Vec<voxel::Vertex>,
}

impl Message for ChunkVertexNfy {
    fn msg_type(&self) -> MessageType {
        MessageType::ChunkVertexNfy
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LandscapeSpawnReq {
    pub center: IVec2,
    pub radius: u8,
}

impl Message for LandscapeSpawnReq {
    fn msg_type(&self) -> MessageType {
        MessageType::LandscapeSpawnReq
    }
}
