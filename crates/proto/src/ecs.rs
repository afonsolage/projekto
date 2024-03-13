use bevy::{ecs::system::SystemId, prelude::*};

use crate::net::ClientId;

use super::{Message, MessageType};

#[derive(Resource, Default, Clone, Debug, Deref, DerefMut)]
pub(crate) struct CopyHandlers<I = (), O = ()>(Vec<SystemId<I, O>>);

#[derive(Resource, Debug, Clone, Copy, Deref, DerefMut)]
pub(crate) struct MoveHandler<I = (), O = ()>(SystemId<I, O>);

pub trait RegisterMessageHandler<T: MessageType> {
    fn set_message_handler<I: Message<T>, O: 'static, M, S: IntoSystem<I, O, M> + 'static>(
        &mut self,
        system: S,
    ) -> &mut Self;

    fn add_message_handler<I: Message<T> + Copy, O: 'static, M, S: IntoSystem<I, O, M> + 'static>(
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

        if self
            .world
            .contains_resource::<MoveHandler<SystemId<I, O>>>()
            || self
                .world
                .contains_resource::<MoveHandler<SystemId<(u32, I), O>>>()
        {
            panic!("Already exists a message handler. Duplicated handler id: {id:?}");
        }

        self.world.insert_resource(MoveHandler(id));

        self
    }

    fn add_message_handler<
        I: Message<T> + Copy,
        O: 'static,
        M,
        S: IntoSystem<I, O, M> + 'static,
    >(
        &mut self,
        system: S,
    ) -> &mut Self {
        let id = self.world.register_system(system);

        self.world
            .get_resource_or_insert_with(|| CopyHandlers(Vec::new()))
            .push(id);

        self
    }
}

//
pub trait RunMessageHandlers<T: MessageType> {
    fn run_handlers<M: Message<T> + Clone>(
        &mut self,
        client_id: ClientId,
        msg: Box<dyn Message<T>>,
    );
}

impl<T: MessageType> RunMessageHandlers<T> for World {
    fn run_handlers<M: Message<T> + Clone>(
        &mut self,
        client_id: ClientId,
        msg: Box<dyn Message<T>>,
    ) {
        let src = msg.msg_source();

        let (copy_handlers, copy_id_handlers, move_handler, move_id_handler) = (
            self.get_resource::<CopyHandlers<M>>().cloned(),
            self.get_resource::<CopyHandlers<(ClientId, M)>>().cloned(),
            self.get_resource::<MoveHandler<M>>().cloned(),
            self.get_resource::<MoveHandler<(ClientId, M)>>().cloned(),
        );

        if copy_handlers.is_none()
            && copy_id_handlers.is_none()
            && move_handler.is_none()
            && move_id_handler.is_none()
        {
            warn!("No handlers found for message {msg:?}. Skipping it");
            return;
        }

        assert!(
            !(move_handler.is_some() && move_id_handler.is_some()),
            "There can't be two move handlers"
        );

        let msg = msg
            .downcast::<M>()
            .expect("To be able to downcast message {src:?}.");

        if let Some(CopyHandlers(system_ids)) = copy_handlers {
            for system_id in system_ids {
                // Only Copy types are allowed to be added on MessageHandlers
                if let Err(err) = self.run_system_with_input(system_id, msg.clone()) {
                    error!("Failed to execute handler for message {src:?}. Error: {err}");
                }
            }
        }

        if let Some(CopyHandlers(system_ids)) = copy_id_handlers {
            for system_id in system_ids {
                // Only Copy types are allowed to be added on MessageHandlers
                if let Err(err) = self.run_system_with_input(system_id, (client_id, msg.clone())) {
                    error!("Failed to execute handler for message {src:?}. Error: {err}");
                }
            }
        }

        if let Some(MoveHandler(system_id)) = move_handler {
            if let Err(err) = self.run_system_with_input(system_id, msg) {
                error!("Failed to execute handler for message {src:?}. Error: {err}");
            }
        } else if let Some(MoveHandler(system_id)) = move_id_handler {
            if let Err(err) = self.run_system_with_input(system_id, (client_id, msg)) {
                error!("Failed to execute handler for message {src:?}. Error: {err}");
            }
        }
    }
}
