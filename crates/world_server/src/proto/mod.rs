use std::any::Any;

use bevy::{ecs::world::World, log::error};

#[derive(thiserror::Error, Debug)]
pub enum MessageError {
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
    fn source() -> MessageSource;
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
