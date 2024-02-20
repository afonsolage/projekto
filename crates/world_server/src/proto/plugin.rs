use bevy::prelude::*;

use super::channel::{WorldChannel, WorldChannelPair};

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
