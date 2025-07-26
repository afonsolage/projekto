use std::any::Any;

use bevy::{ecs::world::World, log::error};

mod channel;
pub use channel::{Channel, ChannelError, ChannelPair};

mod net;
pub use net::{connect_to_server, start_server, Client, ClientId, Server};

mod ecs;
pub use ecs::{NoCopy, RegisterMessageHandler, RunMessageHandlers};

mod enc_dec;
pub use enc_dec::{decode, encode};

#[derive(thiserror::Error, Debug)]
pub enum MessageError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Failed to encode. Error: {0}")]
    Encode(#[from] bincode::error::EncodeError),
    #[error("Failed to decode. Error: {0}")]
    Decode(#[from] bincode::error::DecodeError),
    #[error("Failed to downcast message: {0:?}")]
    Downcasting(MessageSource),
    #[error("{0}")]
    Channel(#[from] channel::ChannelError),
    #[error("Failed to parse {0}. Invalid message code {1}.")]
    InvalidMessage(&'static str, u16),
}

pub trait MessageType: std::fmt::Debug + Send + Sync + 'static {
    fn name() -> &'static str;
    fn source() -> MessageSource;
    fn code(&self) -> u16;
    fn try_from_code(n: u16) -> Result<Self, MessageError>
    where
        Self: Sized;
    fn deserialize_boxed(&self, buf: &[u8]) -> Result<BoxedMessage<Self>, MessageError>;
    fn serialize_boxed(
        &self,
        boxed: BoxedMessage<Self>,
        buf: &mut [u8],
    ) -> Result<u32, MessageError>;
    fn run_handlers(&self, boxed: BoxedMessage<Self>, client_id: net::ClientId, world: &mut World);
    fn is_unit_type(&self) -> bool;
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

#[cfg(test)]
mod tests {
    use crate::{
        self as projekto_proto, BoxedMessage, Message, MessageSource, MessageType, NoCopy,
    };
    use projekto_proto_macros::message_source;

    #[message_source(MessageSource::Client)]
    enum TestMsg {
        UnitMsg,
        UnnamedMsg(u32, u8, bool),
        NamedMsg {
            a: i8,
            b: f32,
            c: (u8, u8),
        },
        #[no_copy]
        NoCopyMsg(String, Vec<u8>),
    }

    fn no_copy_only(_: impl Message<TestMsg> + NoCopy) {
        //
    }

    #[test]
    fn macro_message_source_base() {
        assert_eq!(TestMsg::name(), "TestMsg");
        assert_eq!(TestMsg::source(), MessageSource::Client);
    }

    #[test]
    fn macro_message_source_unit() {
        assert!(TestMsg::UnitMsg.is_unit_type());

        let boxed: BoxedMessage<TestMsg> = Box::new(UnitMsg);
        let mut buf = vec![0u8; 0];
        let size = TestMsg::UnitMsg
            .serialize_boxed(boxed, &mut buf)
            .expect("Unit types should generate not byte when serializing");

        assert_eq!(size, 0);

        let boxed = TestMsg::UnitMsg
            .deserialize_boxed(&[])
            .expect("Unit types should not require bytes to deserialize");

        assert_eq!(boxed.downcast::<UnitMsg>().unwrap(), UnitMsg);
    }

    #[test]
    fn macro_message_source_unnamed() {
        assert!(!TestMsg::UnnamedMsg.is_unit_type());

        let boxed: BoxedMessage<TestMsg> = Box::new(UnnamedMsg(1, 2, true));
        let mut buf = vec![0u8; 64];
        let size = TestMsg::UnnamedMsg
            .serialize_boxed(boxed, &mut buf)
            .unwrap();

        assert!(size > 0);

        let boxed = TestMsg::UnnamedMsg
            .deserialize_boxed(&buf[..size as usize])
            .unwrap();

        let UnnamedMsg(n1, n2, b1) = boxed.downcast::<UnnamedMsg>().unwrap();

        assert_eq!(n1, 1);
        assert_eq!(n2, 2);
        assert!(b1);
    }

    #[test]
    fn macro_message_source_named() {
        assert!(!TestMsg::NamedMsg.is_unit_type());

        let boxed: BoxedMessage<TestMsg> = Box::new(NamedMsg {
            a: 42,
            b: 3.123,
            c: (22, 33),
        });

        let mut buf = vec![0u8; 64];
        let size = TestMsg::NamedMsg.serialize_boxed(boxed, &mut buf).unwrap();

        assert!(size > 0);

        let boxed = TestMsg::NamedMsg
            .deserialize_boxed(&buf[..size as usize])
            .unwrap();

        let NamedMsg { a, b, c } = boxed.downcast::<NamedMsg>().unwrap();

        assert_eq!(a, 42);
        assert_eq!(b, 3.123);
        assert_eq!(c, (22, 33));
    }

    #[test]
    fn macro_message_source_no_copy() {
        no_copy_only(NoCopyMsg("asd".to_string(), vec![]));

        assert!(!TestMsg::NoCopyMsg.is_unit_type());

        let boxed: BoxedMessage<TestMsg> = Box::new(NoCopyMsg("msg".to_string(), vec![1, 2, 3]));

        let mut buf = vec![0u8; 64];
        let size = TestMsg::NoCopyMsg.serialize_boxed(boxed, &mut buf).unwrap();

        assert!(size > 0);

        let boxed = TestMsg::NoCopyMsg
            .deserialize_boxed(&buf[..size as usize])
            .unwrap();

        let NoCopyMsg(s, v) = boxed.downcast::<NoCopyMsg>().unwrap();

        assert_eq!(s, "msg".to_string());
        assert_eq!(v, vec![1, 2, 3]);
    }
}
