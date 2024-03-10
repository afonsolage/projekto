use std::{
    io,
    sync::{atomic::AtomicBool, Arc},
};

use async_net::{SocketAddr, TcpListener, TcpStream};
use bevy::{
    prelude::*,
    tasks::{AsyncComputeTaskPool, TaskPool},
};
use futures_lite::{AsyncReadExt, AsyncWriteExt, StreamExt};

use crate::proto::{
    channel::{WorldChannel, WorldChannelPair},
    MessageError, MessageType,
};

mod plugin;

pub(crate) use plugin::*;

#[derive(Debug, Clone)]
struct Client<S, R> {
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

    fn channel(&self) -> &WorldChannel<R, S> {
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

async fn net_to_channel<S: MessageType, R: MessageType>(
    mut stream: TcpStream,
    channel: WorldChannel<R, S>,
) -> Result<(), MessageError> {
    let mut cache_buffer = vec![0; R::MAX_MESSAGE_SIZE];
    let mut msg_code = [0; std::mem::size_of::<u16>()];
    let mut msg_len = [0; std::mem::size_of::<u32>()];

    loop {
        // First get the message type and check if it is a valid one.
        stream.read_exact(&mut msg_code).await?;
        let msg_type = R::try_from_code(u16::from_be_bytes(msg_code))?;

        // Then check if the message len is also valid.
        stream.read_exact(&mut msg_len).await?;
        let msg_len = u32::from_be_bytes(msg_len) as usize;

        if msg_len == 0 {
            return Err(MessageError::Io(std::io::ErrorKind::BrokenPipe.into()));
        }

        if msg_len >= cache_buffer.len() {
            return Err(MessageError::Io(std::io::ErrorKind::InvalidData.into()));
        }

        // Get a mutable slice which fits the incomming message.
        let buffer = &mut cache_buffer[..msg_len];
        stream.read_exact(buffer).await?;

        let boxed = msg_type.deserialize_boxed(buffer)?;
        channel.send_boxed(boxed)?;
    }
}

async fn channel_to_net<S: MessageType, R: MessageType>(
    mut stream: TcpStream,
    channel: WorldChannel<S, R>,
) -> Result<(), MessageError> {
    let mut buffer = vec![0; S::MAX_MESSAGE_SIZE];

    while let Ok(boxed) = channel.wait().await {
        let msg_type = boxed.msg_type();
        let msg_type_bytes = msg_type.code().to_be_bytes();

        let msg_size_offset = msg_type_bytes.len();
        let msg_offset = msg_size_offset + std::mem::size_of::<u32>();

        // First serialize at right offset (6 bytes - 2 + 4)
        let msg_size = msg_type.serialize_boxed(boxed, &mut buffer[msg_offset..])?;
        let msg_size_bytes = msg_size.to_be_bytes();

        // Then prepend msg type (2 bytes) and msg size (4 bytes)
        buffer[0..].copy_from_slice(&msg_type_bytes);
        buffer[msg_size_offset..].copy_from_slice(&msg_size_bytes);

        // The final packet to be send is type + size + the serialized message size.
        let packet_buffer = &buffer[..msg_offset + msg_size as usize];
        stream.write_all(packet_buffer).await?;
        stream.flush().await?;
    }

    stream.close().await?;
    channel.close();

    Ok(())
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
                if let Err(err) = net_to_channel(stream_clone, client_clone).await {
                    debug!("[{id}] Failed to receive messages from {addr}: Error: {err:?}");
                    send_closed.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            })
            .detach();

        let send_closed = closed.clone();
        AsyncComputeTaskPool::get_or_init(TaskPool::default)
            .spawn(async move {
                if let Err(err) = channel_to_net(stream, client).await {
                    debug!("[{id}] Failed to send messages to {addr}: Error: {err:?}");
                    send_closed.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            })
            .detach();

        on_client_connected(Client::new(id, addr, server, closed));
    }

    Ok(())
}
