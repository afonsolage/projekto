use bevy::{ecs::system::SystemId, prelude::*};

use crate::net::ClientId;

use super::{Message, MessageType};

#[derive(Resource, Default, Clone, Debug, Deref, DerefMut)]
pub(crate) struct CopyHandlers<I: Copy>(Vec<SystemId<I, ()>>);

pub trait NoCopy {}
impl<M> NoCopy for (ClientId, M) where M: NoCopy {}

#[derive(Resource, Debug, Clone, Copy, Deref, DerefMut)]
pub(crate) struct MoveHandler<I: NoCopy>(SystemId<I, ()>);

pub trait MessageHandlerInput<T, M> {}

impl<T: MessageType, M> MessageHandlerInput<T, M> for M where M: Message<T> {}
impl<T: MessageType, M> MessageHandlerInput<T, M> for (ClientId, M) where M: Message<T> {}

pub trait RegisterMessageHandler<T: MessageType, M: Message<T>> {
    fn set_message_handler<
        I: MessageHandlerInput<T, M> + NoCopy + 'static,
        Marker,
        S: IntoSystem<I, (), Marker> + 'static,
    >(
        &mut self,
        system: S,
    ) -> &mut Self
    where
        M: NoCopy;

    fn add_message_handler<
        I: MessageHandlerInput<T, M> + Copy + 'static,
        Marker,
        S: IntoSystem<I, (), Marker> + 'static,
    >(
        &mut self,
        system: S,
    ) -> &mut Self;
}

impl<T: MessageType, M: Message<T>> RegisterMessageHandler<T, M> for App {
    fn set_message_handler<
        I: MessageHandlerInput<T, M> + NoCopy + 'static,
        Marker,
        S: IntoSystem<I, (), Marker> + 'static,
    >(
        &mut self,
        system: S,
    ) -> &mut Self
    where
        M: NoCopy,
    {
        let id = self.world.register_system(system);

        if self.world.contains_resource::<MoveHandler<M>>()
            || self.world.contains_resource::<MoveHandler<(ClientId, M)>>()
        {
            panic!("Already exists a message handler. Duplicated handler id: {id:?}");
        }

        self.world.insert_resource(MoveHandler(id));

        self
    }

    fn add_message_handler<
        I: MessageHandlerInput<T, M> + Copy + 'static,
        Marker,
        S: IntoSystem<I, (), Marker> + 'static,
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
    fn run_handlers<M: Message<T> + Copy>(&mut self, client_id: ClientId, msg: Box<dyn Message<T>>);
    fn run_handler<M: Message<T> + NoCopy>(
        &mut self,
        client_id: ClientId,
        msg: Box<dyn Message<T>>,
    );
}

impl<T: MessageType> RunMessageHandlers<T> for World {
    fn run_handlers<M: Message<T> + Copy>(
        &mut self,
        client_id: ClientId,
        msg: Box<dyn Message<T>>,
    ) {
        let src = msg.msg_source();

        let (copy_handlers, copy_id_handlers) = (
            self.get_resource::<CopyHandlers<M>>().cloned(),
            self.get_resource::<CopyHandlers<(ClientId, M)>>().cloned(),
        );

        if copy_handlers.is_none() && copy_id_handlers.is_none() {
            warn!("No handlers found for message {msg:?}. Skipping it");
            return;
        }

        let msg = msg
            .downcast::<M>()
            .expect("To be able to downcast message {src:?}.");

        if let Some(CopyHandlers(system_ids)) = copy_handlers {
            for system_id in system_ids {
                // Only Copy types are allowed to be added on MessageHandlers
                if let Err(err) = self.run_system_with_input(system_id, msg) {
                    error!("Failed to execute handler for message {src:?}. Error: {err}");
                }
            }
        }

        if let Some(CopyHandlers(system_ids)) = copy_id_handlers {
            for system_id in system_ids {
                // Only Copy types are allowed to be added on MessageHandlers
                if let Err(err) = self.run_system_with_input(system_id, (client_id, msg)) {
                    error!(
                        "Failed to execute handler for message {src:?}({client_id}). Error: {err}"
                    );
                }
            }
        }
    }

    fn run_handler<M: Message<T> + NoCopy>(
        &mut self,
        client_id: ClientId,
        msg: Box<dyn Message<T>>,
    ) {
        let src = msg.msg_source();

        let msg = msg
            .downcast::<M>()
            .expect("To be able to downcast message {src:?}.");

        if let Some(&MoveHandler(system_id)) = self.get_resource::<MoveHandler<M>>() {
            if let Err(err) = self.run_system_with_input(system_id, msg) {
                error!("Failed to execute handler for message {src:?}. Error: {err}");
            }
        } else if let Some(&MoveHandler(system_id)) =
            self.get_resource::<MoveHandler<(ClientId, M)>>()
        {
            if let Err(err) = self.run_system_with_input(system_id, (client_id, msg)) {
                error!("Failed to execute handler for message {src:?}({client_id}). Error: {err}");
            }
        } else {
            warn!("No handlers found for message {msg:?}. Skipping it");
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::{app::App, ecs::system::In};
    use projekto_proto_macros::message_source;

    use crate::{
        self as projekto_proto, ecs::MoveHandler, ClientId, MessageSource, RegisterMessageHandler,
    };

    use super::CopyHandlers;

    #[message_source(MessageSource::Client)]
    enum TestMsg {
        A,
        #[no_copy]
        B(u32),
    }

    #[test]
    fn add_message_handler() {
        // Arrange
        let mut app = App::new();

        // Act
        for _ in 0..10 {
            app.add_message_handler(|_: In<A>| todo!());
        }
        for _ in 0..9 {
            app.add_message_handler(|_: In<(ClientId, A)>| todo!());
        }

        // Assert
        let handlers = app
            .world
            .get_resource::<CopyHandlers<A>>()
            .expect("Should add a CopyHandlers");

        assert_eq!(handlers.len(), 10, "10 handlers should be added");

        let handlers = app
            .world
            .get_resource::<CopyHandlers<(ClientId, A)>>()
            .expect("Should add a CopyHandlers");

        assert_eq!(handlers.len(), 9, "9 handlers should be added");
    }

    #[test]
    fn set_message_handler() {
        // Arrange
        let mut app = App::new();

        // Act
        app.set_message_handler(|_: In<B>| todo!());

        // Assert
        let _ = app
            .world
            .get_resource::<MoveHandler<B>>()
            .expect("Should add a MoveHandler");
    }

    #[test]
    fn set_message_handler_client_id() {
        // Arrange
        let mut app = App::new();

        // Act
        app.set_message_handler(|_: In<(ClientId, B)>| todo!());

        // Assert
        let _ = app
            .world
            .get_resource::<MoveHandler<(ClientId, B)>>()
            .expect("Should add a MoveHandler");
    }

    #[test]
    #[should_panic]
    fn set_message_handler_duplicated() {
        // Arrange
        let mut app = App::new();

        // Act
        app.set_message_handler(|_: In<B>| todo!());
        app.set_message_handler(|_: In<B>| todo!());

        // Assert
    }

    #[test]
    #[should_panic]
    fn set_message_handler_client_id_duplicated() {
        // Arrange
        let mut app = App::new();

        // Act
        app.set_message_handler(|_: In<B>| todo!());
        app.set_message_handler(|_: In<(ClientId, B)>| todo!());

        // Assert
    }
}
