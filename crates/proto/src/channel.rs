use std::fmt::Debug;

use async_channel::{Receiver, Sender};

use super::{BoxedMessage, Message, MessageType};

pub struct WorldChannelPair<S: MessageType, R: MessageType> {
    pub client: WorldChannel<S, R>,
    pub server: WorldChannel<R, S>,
}

#[derive(Debug)]
pub struct WorldChannel<S, R> {
    sender: Sender<BoxedMessage<S>>,
    receiver: Receiver<BoxedMessage<R>>,
}

impl<S, R> WorldChannel<S, R> {
    pub fn is_closed(&self) -> bool {
        self.sender.is_closed() || self.receiver.is_closed()
    }
}

impl<S, R> Clone for WorldChannel<S, R> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            receiver: self.receiver.clone(),
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum WorldChannelError {
    #[error("Failed to send message.")]
    Send(),
    #[error("Failed to receive message: {0}")]
    Recv(#[from] async_channel::RecvError),
}

impl<S: MessageType, R: MessageType> WorldChannel<S, R> {
    pub fn new_pair() -> WorldChannelPair<S, R> {
        let (server_sender, server_receiver) = async_channel::unbounded();
        let (client_sender, client_receiver) = async_channel::unbounded();
        WorldChannelPair {
            client: WorldChannel::<S, R> {
                sender: server_sender,
                receiver: client_receiver,
            },
            server: WorldChannel::<R, S> {
                sender: client_sender,
                receiver: server_receiver,
            },
        }
    }

    pub fn len(&self) -> usize {
        self.receiver.len()
    }

    pub fn is_empty(&self) -> bool {
        self.receiver.is_empty()
    }

    pub fn recv(&self) -> Option<BoxedMessage<R>> {
        self.receiver.try_recv().ok().map(|msg| {
            bevy::log::trace!(
                "[{:?}] Received message: {:?}",
                msg.msg_source(),
                msg.msg_type()
            );
            msg
        })
    }

    pub fn recv_all(&self) -> Vec<BoxedMessage<R>> {
        let mut messages = vec![];

        while let Some(msg) = self.recv() {
            messages.push(msg);
        }

        messages
    }

    pub async fn wait(&self) -> Result<BoxedMessage<R>, WorldChannelError> {
        Ok(self.receiver.recv().await?)
    }

    pub fn send(&self, msg: impl Message<S>) -> Result<(), WorldChannelError> {
        let boxed: BoxedMessage<S> = Box::new(msg);
        self.send_boxed(boxed)
    }

    pub fn send_boxed(&self, boxed: BoxedMessage<S>) -> Result<(), WorldChannelError> {
        bevy::log::trace!(
            "[{:?}] Sending message: {:?}",
            boxed.msg_source(),
            boxed.msg_type()
        );

        self.sender
            .try_send(boxed)
            .map_err(|_| WorldChannelError::Send())
    }

    pub fn close(self) {
        let _ = self.receiver.close();
        let _ = self.sender.close();
    }
}