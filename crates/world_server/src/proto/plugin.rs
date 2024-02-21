use bevy::prelude::*;

use super::{
    channel::{WorldChannel, WorldChannelPair},
    Message,
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
pub struct WorldServerChannel(WorldChannel);

#[derive(Resource, Debug, Clone, Deref)]
pub struct WorldClientChannel(WorldChannel);

#[derive(Resource, Debug, Deref, DerefMut)]
pub struct MessageQueue<T: Message + Send + Sync>(Vec<T>);

pub fn has_messages<T: Message + Send + Sync>(maybe_queue: Option<Res<MessageQueue<T>>>) -> bool {
    maybe_queue.is_some_and(|queue| !queue.is_empty())
}

impl<T: Message + Send + Sync> Default for MessageQueue<T> {
    fn default() -> Self {
        Self(Default::default())
    }
}
