use std::{
    io,
    mem::size_of,
    sync::{atomic::AtomicBool, Arc},
};

use async_net::TcpStream;
use bevy::{
    log::{debug, trace},
    tasks::{AsyncComputeTaskPool, TaskPool},
};
use futures_lite::{AsyncReadExt, AsyncWriteExt};
use projekto_server::proto::{
    channel::{WorldChannel, WorldChannelPair},
    MessageError, MessageType,
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

async fn net_to_channel<S: MessageType, R: MessageType>(
    mut stream: TcpStream,
    channel: WorldChannel<R, S>,
) -> Result<(), MessageError> {
    let mut msg_code = [0; std::mem::size_of::<u16>()];
    let mut msg_len = [0; std::mem::size_of::<u32>()];

    let cache_buffer_size = R::MAX_MESSAGE_SIZE + msg_code.len() + msg_len.len();
    let mut cache_buffer = vec![0; cache_buffer_size];

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

        if msg_len >= cache_buffer_size {
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
    let cache_buffer_size = S::MAX_MESSAGE_SIZE + size_of::<u16>() + size_of::<u32>();
    let mut cache_buffer = vec![0; cache_buffer_size];

    while let Ok(boxed) = channel.wait().await {
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
        trace!("Sending {packet_buffer:?}");
        stream.write_all(packet_buffer).await?;
        stream.flush().await?;
    }

    stream.close().await?;
    channel.close();

    Ok(())
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
