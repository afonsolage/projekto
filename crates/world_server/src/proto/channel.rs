use async_channel::{Receiver, Sender};

use super::{BoxedMessasing, Message};

pub struct WorldChannelPair {
    pub client: WorldChannel,
    pub server: WorldChannel,
}

#[derive(Debug, Clone)]
pub struct WorldChannel {
    sender: Sender<BoxedMessasing>,
    receiver: Receiver<BoxedMessasing>,
}

#[derive(thiserror::Error, Debug)]
pub enum WorldChannelError {
    #[error("Failed to receive message: {0}")]
    Recv(#[from] async_channel::RecvError),
}

impl WorldChannel {
    pub fn new_pair() -> WorldChannelPair {
        let (server_sender, server_receiver) = async_channel::unbounded();
        let (client_sender, client_receiver) = async_channel::unbounded();
        WorldChannelPair {
            client: Self {
                sender: server_sender,
                receiver: client_receiver,
            },
            server: Self {
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

    pub fn recv(&self) -> Option<BoxedMessasing> {
        self.receiver.try_recv().ok()
    }

    pub async fn wait(&self) -> Result<BoxedMessasing, WorldChannelError> {
        Ok(self.receiver.recv().await?)
    }

    pub fn send(&self, msg: impl Message + Send) {
        let boxed = Box::new(msg);
        self.sender
            .try_send(boxed)
            .expect("Channel to be unbounded and to be always open");
    }
}
