use std::{
    io,
    sync::{atomic::AtomicBool, Arc},
};

use async_net::{SocketAddr, TcpListener};
use bevy::{
    prelude::*,
    tasks::{AsyncComputeTaskPool, TaskPool},
};
use futures_lite::StreamExt;
use projekto_proto::{
    channel::{WorldChannel, WorldChannelPair},
    net, MessageType,
};

mod plugin;

pub(crate) use plugin::*;

#[derive(Debug, Clone)]
pub struct Client<S, R> {
    id: u32,
    addr: SocketAddr,
    channel: WorldChannel<R, S>,
    closed: Arc<AtomicBool>,
}

impl<S: MessageType, R: MessageType> Client<S, R> {
    fn new(id: u32, addr: SocketAddr, server: WorldChannel<R, S>, closed: Arc<AtomicBool>) -> Self {
        Self {
            id,
            addr,
            channel: server,
            closed,
        }
    }

    pub fn channel(&self) -> &WorldChannel<R, S> {
        &self.channel
    }

    fn id(&self) -> u32 {
        self.id
    }

    fn addr(&self) -> SocketAddr {
        self.addr
    }

    fn is_closed(&self) -> bool {
        self.closed.load(std::sync::atomic::Ordering::Relaxed)
    }
}

async fn start_server<F, S: MessageType, R: MessageType>(
    on_client_connected: F,
) -> Result<(), io::Error>
where
    F: Fn(Client<S, R>),
{
    let bind_addr = "127.0.0.1:11223";
    let listener = TcpListener::bind(bind_addr).await?;

    let mut incoming = listener.incoming();

    info!("[Networking] Starting to listen: {bind_addr}");

    let mut client_idx = 0;
    while let Some(stream) = incoming.next().await {
        let stream = stream?;
        stream.set_nodelay(true)?;

        let addr = stream.peer_addr()?;

        client_idx += 1;
        let id = client_idx;

        info!("[Networking] Client {id}({addr}) connected!");

        let WorldChannelPair { client, server } = WorldChannel::<S, R>::new_pair();
        let closed = Arc::new(AtomicBool::new(false));

        let stream_clone = stream.clone();
        let client_clone = client.clone();
        let send_closed = closed.clone();
        AsyncComputeTaskPool::get_or_init(TaskPool::default)
            .spawn(async move {
                if let Err(err) = net::net_to_channel(stream_clone, client_clone).await {
                    debug!("[{id}] Failed to receive messages from {addr}: Error: {err}");
                    send_closed.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            })
            .detach();

        let send_closed = closed.clone();
        AsyncComputeTaskPool::get_or_init(TaskPool::default)
            .spawn(async move {
                if let Err(err) = net::channel_to_net(stream, client).await {
                    debug!("[{id}] Failed to send messages to {addr}: Error: {err}");
                    send_closed.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            })
            .detach();

        on_client_connected(Client::new(id, addr, server, closed));
    }

    Ok(())
}
