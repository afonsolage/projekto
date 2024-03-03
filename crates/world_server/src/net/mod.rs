use std::{
    io::{self},
    sync::Arc,
};

use async_lock::RwLock;
use async_net::{SocketAddr, TcpListener, TcpStream};
use bevy::{prelude::*, utils::HashMap};
use futures_lite::{AsyncReadExt, AsyncWriteExt, StreamExt};

use crate::proto::{
    channel::WorldChannel, client::ClientMessage, server::ServerMessage, BoxedMessage,
    MessageError, MessageType,
};

pub(super) struct NetPlugin;

impl Plugin for NetPlugin {
    fn build(&self, _app: &mut App) {

        //
    }
}

#[derive(Debug)]
struct Client<S: MessageType, R: MessageType> {
    _marker: std::marker::PhantomData<(S, R)>,
    id: u32,
    addr: SocketAddr,
    stream: TcpStream,
    channel: WorldChannel<R, S>,
    recv_buffer: Vec<u8>,
    send_buffer: Vec<u8>,
    send_queue: Vec<Vec<u8>>,
}

async fn net_to_channel<
    S: MessageType + std::fmt::Debug + 'static,
    R: MessageType + std::fmt::Debug + 'static,
>(
    mut stream: TcpStream,
    channel: WorldChannel<R, S>,
) -> Result<(), MessageError> {
    let mut buffer = vec![0; R::MAX_MESSAGE_SIZE];

    loop {
        let mut msg_code = [0; std::mem::size_of::<u16>()];
        stream.read_exact(&mut msg_code).await?;
        let msg_type = R::try_from_code(u16::from_be_bytes(msg_code))?;

        let mut msg_len = [0; std::mem::size_of::<u32>()];
        stream.read_exact(&mut msg_len).await?;
        let msg_len = u32::from_be_bytes(msg_len) as usize;

        if msg_len == 0 {
            return Err(MessageError::Io(std::io::ErrorKind::BrokenPipe.into()));
        }

        let buffer = &mut buffer[0..msg_len];

        stream.read_exact(buffer).await?;

        let boxed = msg_type.deserialize_boxed(buffer)?;
        channel.send_boxed(boxed);
    }
}

async fn channel_to_net<
    S: MessageType + std::fmt::Debug + 'static,
    R: MessageType + std::fmt::Debug + 'static,
>(
    mut stream: TcpStream,
    channel: WorldChannel<S, R>,
) -> Result<(), MessageError> {
    let mut buffer = vec![0; S::MAX_MESSAGE_SIZE];

    while let Ok(boxed) = channel.wait().await {
        let msg_type = boxed.msg_type();
        let msg_type_bytes = msg_type.code().to_be_bytes();

        let msg_size_offset = msg_type_bytes.len();
        let msg_offset = msg_size_offset + std::mem::size_of::<u32>(); // msg size is always u32

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

    // channel closed.
    Ok(())
}

impl<S: MessageType + std::fmt::Debug + 'static, R: MessageType + std::fmt::Debug + 'static>
    Client<S, R>
{
    fn new(id: u32, addr: SocketAddr, stream: TcpStream, channel: WorldChannel<R, S>) -> Self {
        Self {
            id,
            addr,
            stream,
            channel,
            send_queue: vec![],
            recv_buffer: vec![0; S::MAX_MESSAGE_SIZE],
            send_buffer: vec![0; R::MAX_MESSAGE_SIZE],
            _marker: Default::default(),
        }
    }

    pub fn queue(&mut self, boxed: BoxedMessage<S>) -> Result<(), MessageError> {
        let msg_type = boxed.msg_type();
        let msg_size = msg_type.serialize_boxed(boxed, &mut self.send_buffer)?;

        let msg_type_bytes = msg_type.code().to_be_bytes();
        let msg_size_bytes = msg_size.to_be_bytes();

        let msg_type_offset = 0;
        let msg_size_offset = msg_type_offset + msg_type_bytes.len();
        let msg_offset = msg_size_offset + msg_size_bytes.len();

        let buffer_size = msg_type_bytes.len() + msg_size_bytes.len() + msg_size as usize;
        let mut buffer = vec![0; buffer_size];

        buffer[msg_type_offset..].copy_from_slice(&msg_type_bytes);
        buffer[msg_size_offset..].copy_from_slice(&msg_size_bytes);
        buffer[msg_offset..].copy_from_slice(&self.send_buffer[..msg_size as usize]);

        self.send_queue.push(buffer);

        Ok(())
    }

    async fn send(&mut self) -> Result<(), MessageError> {
        for buffer in self.send_queue.drain(..) {
            self.stream.write_all(&buffer).await?;
            self.stream.flush().await?;
        }

        Ok(())
    }

    async fn recv(&mut self) -> Result<BoxedMessage<R>, MessageError> {
        let mut msg_code = [0; std::mem::size_of::<u16>()];
        self.stream.read_exact(&mut msg_code).await?;
        let msg_type = R::try_from_code(u16::from_be_bytes(msg_code))?;

        let mut msg_len = [0; std::mem::size_of::<u16>()];
        self.stream.read_exact(&mut msg_len).await?;
        let msg_len = u16::from_be_bytes(msg_len) as usize;

        if msg_len == 0 {
            return Err(MessageError::Io(std::io::ErrorKind::BrokenPipe.into()));
        }

        let buffer = &mut self.recv_buffer[0..msg_len];

        self.stream.read_exact(buffer).await?;

        msg_type.deserialize_boxed(buffer)
    }
}

#[derive(Clone)]
struct ConnectedClients(Arc<RwLock<HashMap<u32, Client<ServerMessage, ClientMessage>>>>);

impl ConnectedClients {
    async fn add(&self, client: Client<ServerMessage, ClientMessage>) {
        let id = client.id;
        if let Some(existing_client) = self.0.write().await.insert(id, client) {
            panic!("A previous client with id {id} was overwritten. Client: {existing_client:?}");
        }
    }

    async fn remove(&self, id: u32) {
        self.0.write().await.remove(&id);
    }

    async fn poll_clients(&self) {
        // self.0.read().await
    }
}

async fn start_server(
    channel: WorldChannel<ClientMessage, ServerMessage>,
    clients: ConnectedClients,
) -> Result<(), io::Error> {
    let listener = TcpListener::bind("127.0.0.1:11223").await?;

    let mut incoming = listener.incoming();

    let mut client_idx = 0;
    while let Some(stream) = incoming.next().await {
        let stream = stream?;
        stream.set_nodelay(true)?;

        let addr = stream.peer_addr()?;

        client_idx += 1;
        let id = client_idx;
        let client = Client::<ServerMessage, ClientMessage>::new(id, addr, stream, channel.clone());
        clients.add(client).await;
    }

    Ok(())
}
