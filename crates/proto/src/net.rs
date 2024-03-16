use std::{
    io,
    mem::size_of,
    sync::{atomic::AtomicBool, Arc},
};

use async_net::{AsyncToSocketAddrs, SocketAddr, TcpListener, TcpStream};
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

        let boxed = if msg_type.is_unit_type() {
            // Unit type doesn't have content
            msg_type.deserialize_boxed(&[])?
        } else {
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

            msg_type.deserialize_boxed(buffer)?
        };

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

        let packet_buffer = if msg_type.is_unit_type() {
            // Unit type doesn't have content. Send only msg type
            &msg_type_bytes
        } else {
            let msg_size_offset = msg_type_bytes.len();
            let msg_offset = msg_size_offset + std::mem::size_of::<u32>();

            // First serialize at right offset (6 bytes - 2 + 4)
            let msg_size = msg_type.serialize_boxed(boxed, &mut cache_buffer[msg_offset..])?;
            let msg_size_bytes = msg_size.to_be_bytes();

            // Then prepend msg type (2 bytes) and msg size (4 bytes)
            cache_buffer[0..msg_size_offset].copy_from_slice(&msg_type_bytes);
            cache_buffer[msg_size_offset..msg_offset].copy_from_slice(&msg_size_bytes);

            // The final packet to be send is type + size + the serialized message size.
            &cache_buffer[..msg_offset + msg_size as usize]
        };

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
        self.closed.load(std::sync::atomic::Ordering::Relaxed) || self.channel().is_closed()
    }
}

struct CloseOnDrop<S, R>(Channel<S, R>, Channel<R, S>);
impl<S, R> CloseOnDrop<S, R> {
    fn is_closed(&self) -> bool {
        self.0.is_closed() || self.1.is_closed()
    }
}

impl<S, R> Drop for CloseOnDrop<S, R> {
    fn drop(&mut self) {
        self.0.close();
        self.1.close();
    }
}

pub async fn start_server<F, S: MessageType, R: MessageType>(
    addr: impl AsyncToSocketAddrs,
    on_client_connected: F,
) -> Result<(), io::Error>
where
    F: Fn(Client<S, R>),
{
    let listener = TcpListener::bind(addr).await?;

    let mut incoming = listener.incoming();

    let bind_addr = listener.local_addr()?;
    info!("[Networking] Starting to listen: {bind_addr}");

    let mut channel_guards = vec![];

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
        let recv_closed = closed.clone();
        AsyncComputeTaskPool::get_or_init(TaskPool::default)
            .spawn(async move {
                if let Err(err) = net_to_channel(stream_clone, client_clone).await {
                    debug!("[{id}] Failed to receive messages from {addr}: Error: {err}");
                    recv_closed.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            })
            .detach();

        let send_closed = closed.clone();
        let client_clone = client.clone();
        AsyncComputeTaskPool::get_or_init(TaskPool::default)
            .spawn(async move {
                if let Err(err) = channel_to_net(stream, client_clone).await {
                    debug!("[{id}] Failed to send messages to {addr}: Error: {err}");
                    send_closed.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            })
            .detach();

        on_client_connected(Client::new(id, addr, server.clone(), closed));

        channel_guards.push(CloseOnDrop(client, server));
        channel_guards.retain(|t| !t.is_closed());
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

pub async fn connect_to_server<S: MessageType, R: MessageType>(
    addr: impl AsyncToSocketAddrs,
) -> Result<Server<S, R>, io::Error> {
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

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use bevy::tasks::{AsyncComputeTaskPool, TaskPool};
    use futures_lite::future::block_on;
    use projekto_proto_macros::message_source;

    use crate::{
        self as projekto_proto, connect_to_server, start_server, Client, Message, MessageSource,
    };

    #[message_source(MessageSource::Client)]
    enum TestMsg {
        A,
        B(u32),
        #[no_copy]
        C {
            v: Vec<u8>,
            s: bool,
        },
    }

    #[test]
    fn connection() {
        let bind_addr = "127.0.0.1:11225";
        let clients = Arc::new(Mutex::new(vec![]));
        let connected_clients = clients.clone();
        let server_task = AsyncComputeTaskPool::get_or_init(TaskPool::default).spawn(async move {
            let _ = start_server(bind_addr, |client: Client<TestMsg, TestMsg>| {
                connected_clients.lock().unwrap().push(client);
            })
            .await;
        });

        // Wait server open socket
        std::thread::sleep(std::time::Duration::from_millis(10));

        let client_task = AsyncComputeTaskPool::get_or_init(TaskPool::default)
            .spawn(async move { connect_to_server::<TestMsg, TestMsg>(bind_addr).await });

        let result = block_on(client_task);
        assert!(result.is_ok(), "Should be able to connect to server");

        // Wait packet be sent
        std::thread::sleep(std::time::Duration::from_millis(10));

        assert_eq!(
            clients.lock().unwrap().len(),
            1,
            "One client should be connected"
        );

        let client = clients.lock().unwrap()[0].clone();
        assert!(!client.is_closed(), "Client channel must be open");

        block_on(async move { server_task.cancel().await });

        assert!(client.is_closed(), "Client channel must be closed");
    }

    fn test_client_send_msg<M: Message<TestMsg>>(msg: M, port: u32) -> M {
        let bind_addr = format!("127.0.0.1:{port}");
        let clients = Arc::new(Mutex::new(vec![]));
        let connected_clients = clients.clone();

        let server_bind_addr = bind_addr.clone();
        let server_task = AsyncComputeTaskPool::get_or_init(TaskPool::default).spawn(async move {
            let _ = start_server(server_bind_addr, |client: Client<TestMsg, TestMsg>| {
                connected_clients.lock().unwrap().push(client);
            })
            .await;
        });

        // Wait server open socket
        std::thread::sleep(std::time::Duration::from_millis(10));

        let client_task = AsyncComputeTaskPool::get_or_init(TaskPool::default)
            .spawn(async move { connect_to_server::<TestMsg, TestMsg>(bind_addr).await });

        let server_conn = block_on(client_task).expect("Should be connected to server");

        server_conn.channel().send(msg).unwrap();

        let mut attempts = 10;
        let boxed = loop {
            let Some(client) = clients.lock().unwrap().first().cloned() else {
                continue;
            };

            if let Some(boxed) = client.channel().try_recv() {
                break boxed;
            }

            assert!(
                !client.is_closed(),
                "client should not be closed. This means a possible error on channel"
            );

            // Avoid spin lock
            std::thread::sleep(std::time::Duration::from_millis(10));

            attempts -= 1;
            if attempts <= 0 {
                panic!("Timeout while waiting from message");
            }
        };

        block_on(async move { server_task.cancel().await });
        match boxed.downcast() {
            Ok(msg) => msg,
            Err(err) => panic!("{err:?}"),
        }
    }

    #[test]
    fn client_send_unit_msg() {
        let res = test_client_send_msg(A, 11227);
        assert_eq!(res, A);
    }

    #[test]
    fn client_send_unnamed_msg() {
        let res = test_client_send_msg(B(42), 11228);
        assert_eq!(res.0, 42);
    }

    #[test]
    fn client_send_named_msg() {
        let res = test_client_send_msg(
            C {
                v: vec![10, 11, 12],
                s: true,
            },
            11229,
        );
        assert_eq!(res.v, vec![10, 11, 12]);
        assert!(res.s);
    }
}
