use std::any::Any;

use bevy::{ecs::world::World, log::error};

#[derive(thiserror::Error, Debug)]
pub enum MessageError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Failed to serialize. Error: {0}")]
    Bincode(#[from] Box<bincode::ErrorKind>),
    #[error("Failed to downcast message: {0:?}")]
    Downcasting(MessageSource),
}

pub mod channel;
mod plugin;

pub mod client;
pub mod server;

pub(crate) use plugin::*;
pub use plugin::{handle_server_messages, RegisterMessageHandler, WorldClientChannel};

pub trait MessageType {
    const MAX_MESSAGE_SIZE: usize;

    fn source() -> MessageSource;
    fn deserialize_boxed(&self, buf: &[u8]) -> Result<BoxedMessage<Self>, MessageError>;
    fn serialize_boxed(
        &self,
        boxed: BoxedMessage<Self>,
        buf: &mut [u8],
    ) -> Result<u32, MessageError>;
    fn try_from_code(n: u16) -> Result<Self, MessageError>
    where
        Self: Sized;
    fn code(&self) -> u16;
}

#[derive(Debug, Hash, Eq, PartialEq)]
pub enum MessageSource {
    Client,
    Server,
}

pub type BoxedMessage<T> = Box<dyn Message<T> + Send + 'static>;

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
pub trait Message<T: MessageType>: Downcast + std::fmt::Debug {
    fn msg_type(&self) -> T;

    fn msg_source(&self) -> MessageSource {
        T::source()
    }

    fn get_handler<M: Message<T> + Send + Sync + 'static>(
        world: &mut World,
    ) -> Option<&MessageHandlers<M>>
    where
        Self: Sized,
    {
        world.get_resource()
    }
}

impl<T: MessageType + 'static> dyn Message<T> + Send {
    fn is<M: Message<T>>(&self) -> bool {
        Downcast::as_any(self).is::<M>()
    }

    pub fn downcast<M: Message<T>>(self: Box<Self>) -> Result<M, Box<Self>> {
        if self.is::<M>() {
            Ok(*Downcast::into_any(self).downcast::<M>().unwrap())
        } else {
            Err(self)
        }
    }
}
