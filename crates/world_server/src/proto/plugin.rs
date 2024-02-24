use std::sync::Arc;

use bevy::{ecs::system::SystemId, prelude::*};

use super::{
    channel::{WorldChannel, WorldChannelPair},
    ChunkLoadReq, ChunkVertexNfy, LandscapeUpdate, Message,
};

pub(crate) struct ProtocolPlugin;

impl Plugin for ProtocolPlugin {
    fn build(&self, app: &mut App) {
        let WorldChannelPair { client, server } = WorldChannel::new_pair();
        app.add_systems(PreUpdate, handle_messages);

        app.insert_resource(WorldClientChannel(client))
            .insert_resource(WorldServerChannel(server));
    }
}

fn handle_messages(world: &mut World) {
    let channel = world.resource::<WorldClientChannel>();
    for msg in channel.recv_all() {
        let source = msg.msg_source();
        match source {
            super::MessageSource::Client(msg_type) => match msg_type {
                super::ClientMessage::ChunkLoad => world.run_handlers::<ChunkLoadReq>(msg),
                super::ClientMessage::LandscapeUpdate => world.run_handlers::<LandscapeUpdate>(msg),
            },
            super::MessageSource::Server(msg_type) => match msg_type {
                super::ServerMessage::ChunkVertex => world.run_handlers::<ChunkVertexNfy>(msg),
            },
        }
    }
}

#[derive(Resource, Debug, Deref)]
pub struct WorldServerChannel(WorldChannel);

#[derive(Resource, Debug, Clone, Deref)]
pub struct WorldClientChannel(WorldChannel);

#[derive(Resource, Default, Debug, Deref, DerefMut)]
pub struct MessageHandlers<I = (), O = ()>(pub Vec<SystemId<I, O>>);

#[derive(Resource, Debug, Deref, DerefMut)]
pub struct MessageHandler<I = (), O = ()>(pub SystemId<I, O>);

pub trait RegisterMessageHandler {
    fn set_message_handler<
        I: Message + Send + Sync + 'static,
        O: 'static,
        M,
        S: IntoSystem<I, O, M> + 'static,
    >(
        &mut self,
        system: S,
    ) -> &mut Self;

    fn add_message_handler<
        I: Message + Send + Sync,
        O: 'static,
        M,
        S: IntoSystem<Arc<I>, O, M> + 'static,
    >(
        &mut self,
        system: S,
    ) -> &mut Self;
}

impl RegisterMessageHandler for App {
    fn set_message_handler<
        I: Message + Send + Sync + 'static,
        O: 'static,
        M,
        S: IntoSystem<I, O, M> + 'static,
    >(
        &mut self,
        system: S,
    ) -> &mut Self {
        let id = self.world.register_system(system);

        self.world.insert_resource(MessageHandler(id));

        self
    }

    fn add_message_handler<
        I: Message + Send + Sync,
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
pub trait RunMessageHandlers {
    fn run_handlers<T: Message + Send + Sync + 'static>(&mut self, msg: Box<dyn Message + Send>);
}

impl RunMessageHandlers for World {
    fn run_handlers<T: Message + Send + Sync + 'static>(&mut self, msg: Box<dyn Message + Send>) {
        let src = msg.msg_source();

        let found_handlers = self.contains_resource::<MessageHandlers<Arc<T>>>();
        let found_handler = self.contains_resource::<MessageHandler<T>>();

        if !found_handlers && !found_handler {
            warn!("No handlers found for message {msg:?}. Skipping it");
            return;
        }

        let msg = msg.downcast::<T>().expect("To downcast message {src:?}.");

        let msg = if found_handlers {
            // Clone to avoid having to use `resource_scope` due to mutable access bellow
            let handlers = self.resource::<MessageHandlers<Arc<T>>>().0.clone();
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

        if let Some(&MessageHandler(id)) = self.get_resource::<MessageHandler<T>>() {
            if let Err(err) = self.run_system_with_input(id, msg) {
                error!("Failed to execute handler for message {src:?}. Error: {err}");
            }
        }
    }
}
