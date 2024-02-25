use std::sync::Arc;

use bevy::{ecs::system::SystemId, prelude::*};

use super::{
    channel::{WorldChannel, WorldChannelPair},
    client::{self, ClientMessage},
    server, Message, MessageType,
};

pub(crate) struct ProtocolPlugin;

impl Plugin for ProtocolPlugin {
    fn build(&self, app: &mut App) {
        let WorldChannelPair { client, server } = WorldChannel::new_pair();
        app.insert_resource(WorldClientChannel(client))
            .insert_resource(WorldServerChannel(server));
    }
}

pub fn handle_server_messages(world: &mut World) {
    let channel = world.resource::<WorldClientChannel>();
    for msg in channel.recv_all() {
        let msg_type = msg.msg_type();

        trace!("[Client] Received message: {msg_type:?}");

        match msg_type {
            server::ServerMessage::ChunkVertex => world.run_handlers::<server::ChunkVertex>(msg),
        }
    }
}

pub(crate) fn handle_client_messages(world: &mut World) {
    let channel = world.resource::<WorldServerChannel>();
    for msg in channel.recv_all() {
        let msg_type = msg.msg_type();

        trace!("[Server] Received message: {msg_type:?}");

        match msg_type {
            ClientMessage::ChunkLoad => world.run_handlers::<client::ChunkLoad>(msg),
            ClientMessage::LandscapeUpdate => world.run_handlers::<client::LandscapeUpdate>(msg),
        }
    }
}

#[derive(Resource, Debug, Deref)]
pub(crate) struct WorldServerChannel(WorldChannel<server::ServerMessage, client::ClientMessage>);

#[derive(Resource, Debug, Clone, Deref)]
pub struct WorldClientChannel(WorldChannel<client::ClientMessage, server::ServerMessage>);

#[derive(Resource, Default, Debug, Deref, DerefMut)]
pub struct MessageHandlers<I = (), O = ()>(pub Vec<SystemId<I, O>>);

#[derive(Resource, Debug, Deref, DerefMut)]
pub struct MessageHandler<I = (), O = ()>(pub SystemId<I, O>);

pub trait RegisterMessageHandler<T: MessageType> {
    fn set_message_handler<
        I: Message<T> + Send + Sync + 'static,
        O: 'static,
        M,
        S: IntoSystem<I, O, M> + 'static,
    >(
        &mut self,
        system: S,
    ) -> &mut Self;

    fn add_message_handler<
        I: Message<T> + Send + Sync,
        O: 'static,
        M,
        S: IntoSystem<Arc<I>, O, M> + 'static,
    >(
        &mut self,
        system: S,
    ) -> &mut Self;
}

impl<T: MessageType> RegisterMessageHandler<T> for App {
    fn set_message_handler<
        I: Message<T> + Send + Sync + 'static,
        O: 'static,
        M,
        S: IntoSystem<I, O, M> + 'static,
    >(
        &mut self,
        system: S,
    ) -> &mut Self {
        let id = self.world.register_system(system);

        #[cfg(debug_assertions)]
        if self
            .world
            .contains_resource::<MessageHandler<SystemId<I, O>>>()
        {
            panic!("Already exists a message handler. Duplicated handler id: {id:?}");
        }

        self.world.insert_resource(MessageHandler(id));

        self
    }

    fn add_message_handler<
        I: Message<T> + Send + Sync,
        O: 'static,
        M,
        S: IntoSystem<Arc<I>, O, M> + 'static,
    >(
        &mut self,
        system: S,
    ) -> &mut Self {
        let id = self.world.register_system(system);

        self.world
            .get_resource_or_insert_with(|| MessageHandlers(Vec::new()))
            .push(id);

        self
    }
}

//
pub trait RunMessageHandlers<T: MessageType> {
    fn run_handlers<M: Message<T> + Send + Sync + 'static>(
        &mut self,
        msg: Box<dyn Message<T> + Send>,
    );
}

impl<T: MessageType + 'static> RunMessageHandlers<T> for World {
    fn run_handlers<M: Message<T> + Send + Sync + 'static>(
        &mut self,
        msg: Box<dyn Message<T> + Send>,
    ) {
        let src = msg.msg_source();

        let found_handlers = self.contains_resource::<MessageHandlers<Arc<M>>>();
        let found_handler = self.contains_resource::<MessageHandler<M>>();

        if !found_handlers && !found_handler {
            warn!("No handlers found for message {msg:?}. Skipping it");
            return;
        }

        let msg = msg.downcast::<M>().expect("To downcast message {src:?}.");

        let msg = if found_handlers {
            // Clone to avoid having to use `resource_scope` due to mutable access bellow
            let handlers = self.resource::<MessageHandlers<Arc<M>>>().0.clone();
            let msg = Arc::new(msg);

            for id in handlers {
                if let Err(err) = self.run_system_with_input(id, msg.clone()) {
                    error!("Failed to execute handler for message {src:?}. Error: {err}");
                }
            }

            Arc::into_inner(msg).expect("Only one strong ref to Arc")
        } else {
            msg
        };

        if let Some(&MessageHandler(id)) = self.get_resource::<MessageHandler<M>>() {
            if let Err(err) = self.run_system_with_input(id, msg) {
                error!("Failed to execute handler for message {src:?}. Error: {err}");
            }
        }
    }
}
