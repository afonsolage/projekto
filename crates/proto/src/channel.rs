use std::fmt::Debug;

use async_channel::{Receiver, Sender};

use super::{BoxedMessage, Message, MessageType};

pub struct ChannelPair<S, R> {
    pub client: Channel<S, R>,
    pub server: Channel<R, S>,
}

#[derive(Debug)]
pub struct Channel<S, R> {
    sender: Sender<BoxedMessage<S>>,
    receiver: Receiver<BoxedMessage<R>>,
}

impl<S, R> Channel<S, R> {
    pub fn is_closed(&self) -> bool {
        self.sender.is_closed() || self.receiver.is_closed()
    }

    pub fn close(&self) {
        let _ = self.receiver.close();
        let _ = self.sender.close();
    }

    pub fn len(&self) -> usize {
        self.receiver.len()
    }

    pub fn is_empty(&self) -> bool {
        self.receiver.is_empty()
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ChannelError {
    #[error("Failed to send message. Channel is closed.")]
    Send(),
    #[error("Failed to receive message. Channel is empty and closed.")]
    Recv(#[from] async_channel::RecvError),
}

impl<S: MessageType, R: MessageType> Channel<S, R> {
    pub fn new_pair() -> ChannelPair<S, R> {
        let (server_sender, server_receiver) = async_channel::unbounded();
        let (client_sender, client_receiver) = async_channel::unbounded();
        ChannelPair {
            client: Channel::<S, R> {
                sender: server_sender,
                receiver: client_receiver,
            },
            server: Channel::<R, S> {
                sender: client_sender,
                receiver: server_receiver,
            },
        }
    }

    pub fn try_recv(&self) -> Option<BoxedMessage<R>> {
        self.receiver.try_recv().ok()
    }

    pub fn try_recv_all(&self) -> Vec<BoxedMessage<R>> {
        let mut messages = Vec::with_capacity(self.len());

        while let Some(msg) = self.try_recv() {
            messages.push(msg);
        }

        messages
    }

    pub async fn recv(&self) -> Result<BoxedMessage<R>, ChannelError> {
        Ok(self.receiver.recv().await?)
    }

    pub fn send(&self, msg: impl Message<S>) -> Result<(), ChannelError> {
        let boxed: BoxedMessage<S> = Box::new(msg);
        self.send_boxed(boxed)
    }

    pub fn send_boxed(&self, boxed: BoxedMessage<S>) -> Result<(), ChannelError> {
        self.sender
            .try_send(boxed)
            .map_err(|_| ChannelError::Send())
    }
}

impl<S, R> Clone for Channel<S, R> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            receiver: self.receiver.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use futures_lite::future::block_on;
    use projekto_proto_macros::message_source;

    use crate::{
        self as projekto_proto, Channel, ChannelPair, Message, MessageSource, MessageType,
    };

    #[message_source(MessageSource::Client)]
    enum ClientTestMsg {
        A,
        B,
    }

    #[test]
    fn send_n_recv() {
        let ChannelPair { client, server } = Channel::<ClientTestMsg, ClientTestMsg>::new_pair();
        client.send(A).expect("Should be able to send a message");

        let msg = block_on(async move {
            server
                .recv()
                .await
                .expect("Should be able to receive a message")
        });

        assert_eq!(msg.msg_type().code(), A.msg_type().code());
    }

    #[test]
    fn recv_closed_channel() {
        let ChannelPair { client, server } = Channel::<ClientTestMsg, ClientTestMsg>::new_pair();
        client.send(A).expect("Should be able to send a message");

        drop(client);

        let msg = block_on(async move {
            server
                .recv()
                .await
                .expect("Should be able to receive a message even after channel is closed")
        });

        assert_eq!(msg.msg_type().code(), A.msg_type().code());
    }

    #[test]
    fn send_closed_channel() {
        let ChannelPair { client, server } = Channel::<ClientTestMsg, ClientTestMsg>::new_pair();

        drop(server);

        client
            .send(A)
            .expect_err("Should not be able to send a message on closed channel");
    }

    #[test]
    fn close() {
        let ChannelPair { client, server } = Channel::<ClientTestMsg, ClientTestMsg>::new_pair();
        assert!(!client.is_closed());
        assert!(!server.is_closed());

        client.close();

        assert!(server.is_closed());
    }
}
