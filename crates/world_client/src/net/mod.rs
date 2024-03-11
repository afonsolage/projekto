use std::{
    io,
    sync::{atomic::AtomicBool, Arc},
};

use async_net::TcpStream;
use bevy::{
    log::debug,
    tasks::{AsyncComputeTaskPool, TaskPool},
};
use projekto_proto::{
    channel::{WorldChannel, WorldChannelPair},
    net, MessageType,
};

mod plugin;
pub(crate) use plugin::*;

#[derive(Debug, Clone)]
pub struct Server<S, R> {
    channel: WorldChannel<S, R>,
    closed: Arc<AtomicBool>,
}

impl<S: MessageType, R: MessageType> Server<S, R> {
    fn new(server: WorldChannel<S, R>, closed: Arc<AtomicBool>) -> Self {
        Self {
            channel: server,
            closed,
        }
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn channel(&self) -> &WorldChannel<S, R> {
        &self.channel
    }
}

async fn connect_to_server<S: MessageType, R: MessageType>() -> Result<Server<S, R>, io::Error> {
    let addr = "127.0.0.1:11223";
    let stream = TcpStream::connect(addr).await?;
    stream.set_nodelay(true)?;

    let WorldChannelPair { client, server } = WorldChannel::<S, R>::new_pair();

    let closed = Arc::new(AtomicBool::new(false));

    let stream_clone = stream.clone();
    let server_clone = server.clone();
    let send_closed = closed.clone();
    AsyncComputeTaskPool::get_or_init(TaskPool::default)
        .spawn(async move {
            if let Err(err) = net::net_to_channel(stream_clone, server_clone).await {
                debug!("Failed to receive messages from server: Error: {err:?}");
                send_closed.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        })
        .detach();

    let recv_closed = closed.clone();
    AsyncComputeTaskPool::get_or_init(TaskPool::default)
        .spawn(async move {
            if let Err(err) = net::channel_to_net(stream, server).await {
                debug!("Failed to send messages to server: Error: {err:?}");
                recv_closed.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        })
        .detach();

    Ok(Server::new(client, closed))
}
