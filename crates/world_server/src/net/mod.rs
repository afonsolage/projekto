use std::{
    io::{self, BufWriter, Read, Write},
    mem::size_of,
    num::NonZeroU32,
    sync::Arc,
};

use async_lock::{Mutex, RwLock};
use async_net::{SocketAddr, TcpListener, TcpStream};
use bevy::{asset::io::Writer, prelude::*, utils::HashMap};
use futures_lite::{AsyncReadExt, AsyncWrite, AsyncWriteExt, StreamExt};

use crate::proto::{
    client::ClientMessage, server::ServerMessage, BoxedMessage, Message, MessageError, MessageType,
};

pub(super) struct NetPlugin;

impl Plugin for NetPlugin {
    fn build(&self, app: &mut App) {
        //
    }
}

#[derive(Debug)]
struct Client<S, R> {
    _marker: std::marker::PhantomData<(S, R)>,
    id: u32,
    addr: SocketAddr,
    stream: TcpStream,
    send_buffer: Vec<u8>,
    send_index: usize,
    recv_buffer: Vec<u8>,
}

impl<S: MessageType, R: MessageType> Write for Client<S, R> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.append_to_send_buffer(&buf.len().to_be_bytes());
        self.append_to_send_buffer(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<S: MessageType, R: MessageType> Client<S, R> {
    fn new(id: u32, addr: SocketAddr, stream: TcpStream) -> Self {
        Self {
            id,
            addr,
            stream,
            send_buffer: vec![0; S::MAX_MESSAGE_SIZE],
            send_index: 0,
            recv_buffer: vec![0; R::MAX_MESSAGE_SIZE],
            _marker: Default::default(),
        }
    }

    fn append_to_send_buffer(&mut self, buf: &[u8]) {
        if self.send_index + buf.len() > self.send_buffer.len() {
            panic!(
                "Buffer overflow: index: {}, buf: {}",
                self.send_index,
                buf.len()
            );
        }
        self.send_buffer[self.send_index..buf.len()].copy_from_slice(buf);
        self.send_index += buf.len();
    }

    pub fn queue<M: Message<S> + serde::Serialize>(&mut self, msg: &M) -> Result<(), MessageError> {
        let msg_code = msg.msg_type().to_u32();
        self.append_to_send_buffer(&msg_code.to_le_bytes());
        bincode::serialize_into(self, &msg)?;

        Ok(())
    }

    async fn send(&mut self) -> Result<(), MessageError> {
        self.stream
            .write_all(&self.send_buffer[0..self.send_index])
            .await?;
        self.stream.flush().await?;
        self.send_index = 0;

        Ok(())
    }

    async fn recv(&mut self) -> Result<BoxedMessage<R>, MessageError> {
        let mut msg_code = [0; std::mem::size_of::<u32>()];
        self.stream.read_exact(&mut msg_code).await?;
        let msg_type = R::try_from_u32(u32::from_be_bytes(msg_code))?;

        let mut msg_len = [0; std::mem::size_of::<u32>()];
        self.stream.read_exact(&mut msg_len).await?;
        let msg_len = u32::from_be_bytes(msg_len) as usize;

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

async fn start_server(clients: ConnectedClients) -> Result<(), io::Error> {
    let listener = TcpListener::bind("127.0.0.1:11223").await?;

    let mut incoming = listener.incoming();

    let mut client_idx = 0;
    while let Some(stream) = incoming.next().await {
        let stream = stream?;
        stream.set_nodelay(true)?;

        let addr = stream.peer_addr()?;

        client_idx += 1;
        let id = client_idx;
        let client = Client::<ServerMessage, ClientMessage>::new(id, addr, stream);
        clients.add(client).await;
    }

    Ok(())
}
