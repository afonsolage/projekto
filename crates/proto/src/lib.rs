use std::any::Any;

use bevy::{ecs::world::World, log::error};

mod channel;
pub use channel::{WorldChannel, WorldChannelError, WorldChannelPair};

mod net;
pub use net::{connect_to_server, start_server, Client, Server};

mod ecs;
pub use ecs::{RegisterMessageHandler, RunMessageHandlers};

#[derive(thiserror::Error, Debug)]
pub enum MessageError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Failed to serialize. Error: {0}")]
    Bincode(#[from] Box<bincode::ErrorKind>),
    #[error("Failed to downcast message: {0:?}")]
    Downcasting(MessageSource),
    #[error("{0}")]
    Channel(#[from] channel::WorldChannelError),
    #[error("Failed to parse {0}. Invalid message code {1}.")]
    InvalidMessage(&'static str, u16),
}

pub trait MessageType: std::fmt::Debug + Send + Sync + 'static {
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
    fn run_handlers(&self, boxed: BoxedMessage<Self>, client_id: u32, world: &mut World);
    fn name() -> &'static str;
}

#[derive(Debug, Hash, Eq, PartialEq)]
pub enum MessageSource {
    Client,
    Server,
}

pub type BoxedMessage<T> = Box<dyn Message<T>>;

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
pub trait Message<T: MessageType>: Downcast + std::fmt::Debug + Send + 'static {
    fn msg_type(&self) -> T;

    fn msg_source(&self) -> MessageSource {
        T::source()
    }
}

impl<T: MessageType> dyn Message<T> {
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

impl<M, T: MessageType> Message<T> for (u32, M)
where
    M: Message<T>,
{
    fn msg_type(&self) -> T {
        self.1.msg_type()
    }
}
