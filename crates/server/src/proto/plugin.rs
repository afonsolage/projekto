use std::sync::Arc;

use bevy::{ecs::system::SystemId, prelude::*};

use super::{
    channel::{WorldChannel, WorldChannelPair},
    client::{self},
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

#[derive(Resource, Debug, Deref)]
pub(crate) struct WorldServerChannel(WorldChannel<server::ServerMessage, client::ClientMessage>);

#[derive(Resource, Debug, Clone, Deref)]
pub struct WorldClientChannel(WorldChannel<client::ClientMessage, server::ServerMessage>);

#[derive(Resource, Default, Debug, Deref, DerefMut)]
pub struct MessageHandlers<I = (), O = ()>(pub Vec<SystemId<I, O>>);

#[derive(Resource, Debug, Deref, DerefMut)]
pub struct MessageHandler<I = (), O = ()>(pub SystemId<I, O>);

pub trait RegisterMessageHandler<T: MessageType> {
    fn set_message_handler<I: Message<T>, O: 'static, M, S: IntoSystem<I, O, M> + 'static>(
        &mut self,
        system: S,
    ) -> &mut Self;

    fn add_message_handler<I: Message<T>, O: 'static, M, S: IntoSystem<Arc<I>, O, M> + 'static>(
        &mut self,
        system: S,
    ) -> &mut Self;
}

impl<T: MessageType> RegisterMessageHandler<T> for App {
    fn set_message_handler<I: Message<T>, O: 'static, M, S: IntoSystem<I, O, M> + 'static>(
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

    fn add_message_handler<I: Message<T>, O: 'static, M, S: IntoSystem<Arc<I>, O, M> + 'static>(
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
    fn run_handlers<M: Message<T>>(&mut self, msg: Box<dyn Message<T>>);
}

impl<T: MessageType> RunMessageHandlers<T> for World {
    fn run_handlers<M: Message<T>>(&mut self, msg: Box<dyn Message<T>>) {
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
