use std::{
    io,
    mem::size_of,
    sync::{atomic::AtomicBool, Arc},
};

use async_net::{SocketAddr, TcpListener, TcpStream};
use bevy::{
    log::{debug, info},
    tasks::{AsyncComputeTaskPool, TaskPool},
};
use futures_lite::{AsyncReadExt, AsyncWriteExt, StreamExt};

use crate::{
    channel::{Channel, ChannelPair},
    MessageError, MessageType,
};

const CACHE_BUFFER_SIZE: usize = 1024 * 1024 * 32; // 32 MB

async fn net_to_channel<S: MessageType, R: MessageType>(
    mut stream: TcpStream,
    channel: Channel<S, R>,
) -> Result<(), MessageError> {
    let mut cache_buffer = vec![0; CACHE_BUFFER_SIZE];

    let mut msg_code = [0; size_of::<u16>()];
    let mut msg_len = [0; size_of::<u32>()];

    loop {
        // First get the message type and check if it is a valid one.
        stream.read_exact(&mut msg_code).await?;
        let msg_type = S::try_from_code(u16::from_be_bytes(msg_code))?;

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
    channel: Channel<S, R>,
) -> Result<(), MessageError> {
    let mut cache_buffer = vec![0; CACHE_BUFFER_SIZE];

    while let Ok(boxed) = channel.recv().await {
        let msg_type = boxed.msg_type();
        let msg_type_bytes = msg_type.code().to_be_bytes();

        let msg_size_offset = msg_type_bytes.len();
        let msg_offset = msg_size_offset + std::mem::size_of::<u32>();

        // First serialize at right offset (6 bytes - 2 + 4)
        let msg_size = msg_type.serialize_boxed(boxed, &mut cache_buffer[msg_offset..])?;
        let msg_size_bytes = msg_size.to_be_bytes();

        // Then prepend msg type (2 bytes) and msg size (4 bytes)
        cache_buffer[0..msg_size_offset].copy_from_slice(&msg_type_bytes);
        cache_buffer[msg_size_offset..msg_offset].copy_from_slice(&msg_size_bytes);

        // The final packet to be send is type + size + the serialized message size.
        let packet_buffer = &cache_buffer[..msg_offset + msg_size as usize];
        stream.write_all(packet_buffer).await?;
        stream.flush().await?;
    }

    stream.close().await?;
    channel.close();

    Ok(())
}

#[derive(Debug, Clone, Copy, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct ClientId(u32);

impl std::fmt::Display for ClientId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(test)]
impl From<u32> for ClientId {
    fn from(value: u32) -> Self {
        ClientId(value)
    }
}

#[derive(Debug, Clone)]
pub struct Client<S, R> {
    id: ClientId,
    addr: SocketAddr,
    channel: Channel<R, S>,
    closed: Arc<AtomicBool>,
}

impl<S: MessageType, R: MessageType> Client<S, R> {
    fn new(id: ClientId, addr: SocketAddr, server: Channel<R, S>, closed: Arc<AtomicBool>) -> Self {
        Self {
            id,
            addr,
            channel: server,
            closed,
        }
    }

    pub fn channel(&self) -> &Channel<R, S> {
        &self.channel
    }

    pub fn id(&self) -> ClientId {
        self.id
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(std::sync::atomic::Ordering::Relaxed)
    }
}

pub async fn start_server<F, S: MessageType, R: MessageType>(
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
        let id = ClientId(client_idx);

        info!("[Networking] Client {id}({addr}) connected!");

        let ChannelPair { client, server } = Channel::<S, R>::new_pair();
        let closed = Arc::new(AtomicBool::new(false));

        let stream_clone = stream.clone();
        let client_clone = client.clone();
        let send_closed = closed.clone();
        AsyncComputeTaskPool::get_or_init(TaskPool::default)
            .spawn(async move {
                if let Err(err) = net_to_channel(stream_clone, client_clone).await {
                    debug!("[{id}] Failed to receive messages from {addr}: Error: {err}");
                    send_closed.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            })
            .detach();

        let send_closed = closed.clone();
        AsyncComputeTaskPool::get_or_init(TaskPool::default)
            .spawn(async move {
                if let Err(err) = channel_to_net(stream, client).await {
                    debug!("[{id}] Failed to send messages to {addr}: Error: {err}");
                    send_closed.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            })
            .detach();

        on_client_connected(Client::new(id, addr, server, closed));
    }

    Ok(())
}

#[derive(Debug, Clone)]
pub struct Server<S, R> {
    channel: Channel<S, R>,
    closed: Arc<AtomicBool>,
}

impl<S: MessageType, R: MessageType> Server<S, R> {
    fn new(server: Channel<S, R>, closed: Arc<AtomicBool>) -> Self {
        Self {
            channel: server,
            closed,
        }
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn channel(&self) -> &Channel<S, R> {
        &self.channel
    }
}

pub async fn connect_to_server<S: MessageType, R: MessageType>() -> Result<Server<S, R>, io::Error>
{
    let addr = "127.0.0.1:11223";
    let stream = TcpStream::connect(addr).await?;
    stream.set_nodelay(true)?;

    let ChannelPair { client, server } = Channel::<S, R>::new_pair();

    let closed = Arc::new(AtomicBool::new(false));

    let stream_clone = stream.clone();
    let server_clone = server.clone();
    let send_closed = closed.clone();
    AsyncComputeTaskPool::get_or_init(TaskPool::default)
        .spawn(async move {
            if let Err(err) = net_to_channel(stream_clone, server_clone).await {
                debug!("Failed to receive messages from server: Error: {err:?}");
                send_closed.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        })
        .detach();

    let recv_closed = closed.clone();
    AsyncComputeTaskPool::get_or_init(TaskPool::default)
        .spawn(async move {
            if let Err(err) = channel_to_net(stream, server).await {
                debug!("Failed to send messages to server: Error: {err:?}");
                recv_closed.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        })
        .detach();

    Ok(Server::new(client, closed))
}
