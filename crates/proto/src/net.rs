use std::mem::size_of;

use async_net::TcpStream;
use futures_lite::{AsyncReadExt, AsyncWriteExt};

use crate::{channel::WorldChannel, MessageError, MessageType};

const CACHE_BUFFER_SIZE: usize = 1024 * 1024 * 32; // 32 MB

pub async fn net_to_channel<S: MessageType, R: MessageType>(
    mut stream: TcpStream,
    channel: WorldChannel<S, R>,
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

pub async fn channel_to_net<S: MessageType, R: MessageType>(
    mut stream: TcpStream,
    channel: WorldChannel<S, R>,
) -> Result<(), MessageError> {
    let mut cache_buffer = vec![0; CACHE_BUFFER_SIZE];

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
        stream.write_all(packet_buffer).await?;
        stream.flush().await?;
    }

    stream.close().await?;
    channel.close();

    Ok(())
}
