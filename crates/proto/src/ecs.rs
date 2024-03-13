use bevy::{ecs::system::SystemId, prelude::*};

use super::{Message, MessageType};

#[derive(Resource, Default, Debug, Deref, DerefMut)]
pub(crate) struct MessageHandlers<I = (), O = ()>(Vec<SystemId<I, O>>);

#[derive(Resource, Debug, Deref, DerefMut)]
pub(crate) struct MessageHandler<I = (), O = ()>(SystemId<I, O>);

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
            .get_resource_or_insert_with(|| MessageHandlers(Vec::new()))
            .push(id);

        self
    }
}

//
pub trait RunMessageHandlers<T: MessageType> {
    fn run_handlers<M: Message<T> + Clone>(&mut self, msg: Box<dyn Message<T>>);
}

impl<T: MessageType> RunMessageHandlers<T> for World {
    fn run_handlers<M: Message<T> + Clone>(&mut self, msg: Box<dyn Message<T>>) {
        let src = msg.msg_source();

        let found_handlers = self.contains_resource::<MessageHandlers<M>>();
        let found_handler = self.contains_resource::<MessageHandler<M>>();

        if !found_handlers && !found_handler {
            warn!("No handlers found for message {msg:?}. Skipping it");
            return;
        }

        let msg = msg.downcast::<M>().expect("To downcast message {src:?}.");

        let msg = if found_handlers {
            // Clone to avoid having to use `resource_scope` due to mutable access bellow
            let handlers = self.resource::<MessageHandlers<M>>().0.clone();

            for id in handlers {
                // Only Copy types are allowed to be added on MessageHandlers
                if let Err(err) = self.run_system_with_input(id, msg.clone()) {
                    error!("Failed to execute handler for message {src:?}. Error: {err}");
                }
            }
            msg
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
