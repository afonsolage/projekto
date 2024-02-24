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

#[derive(Debug, Hash, Eq, PartialEq)]
pub enum MessageSource {
    Client(client::ClientMessage),
    Server(server::ServerMessage),
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
