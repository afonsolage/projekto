mod plugin;

use std::io;

use async_net::TcpStream;
use bevy::{
    log::debug,
    tasks::{AsyncComputeTaskPool, TaskPool},
};
use futures_lite::{AsyncReadExt, AsyncWriteExt};
pub(crate) use plugin::*;
use projekto_server::proto::{
    channel::{WorldChannel, WorldChannelPair},
    MessageError, MessageType,
};

#[derive(Debug, Clone)]
struct Server<S, R> {
    channel: WorldChannel<S, R>,
}

impl<S: MessageType, R: MessageType> Server<S, R> {
    fn new(server: WorldChannel<S, R>) -> Self {
        Self { channel: server }
    }

    fn is_closed(&self) -> bool {
        self.channel.is_closed()
    }

    fn channel(&self) -> &WorldChannel<S, R> {
        &self.channel
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

async fn connect_to_server<S: MessageType, R: MessageType>() -> Result<Server<S, R>, io::Error> {
    let addr = "127.0.0.1:11223";
    let stream = TcpStream::connect(addr).await?;
    stream.set_nodelay(true)?;

    let WorldChannelPair { client, server } = WorldChannel::<S, R>::new_pair();

    let stream_clone = stream.clone();
    let server_clone = server.clone();
    AsyncComputeTaskPool::get_or_init(TaskPool::default)
        .spawn(async move {
            if let Err(err) = net_to_channel(stream_clone, server_clone).await {
                debug!("Failed to receive messages from server: Error: {err:?}");
            }
        })
        .detach();

    AsyncComputeTaskPool::get_or_init(TaskPool::default)
        .spawn(async move {
            if let Err(err) = channel_to_net(stream, server).await {
                debug!("Failed to send messages to server: Error: {err:?}");
            }
        })
        .detach();

    Ok(Server::new(client))
}
