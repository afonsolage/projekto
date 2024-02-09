use bevy::prelude::*;
use crossbeam_channel::{unbounded, Receiver, Sender};

use crate::{Landscape, WorldSet};

pub enum ServerCommand {
    LandscapeUpdateCenter(IVec2),
    LandscapeUpdateRadius(u8),
    LandscapeRemove,
    LandscapeAdd(IVec2, u8),
}

pub struct WorldServerChannelPlugin;

#[derive(Resource, Deref, DerefMut)]
struct ServerCommandSender(Sender<ServerCommand>);

#[derive(Resource, Deref, DerefMut)]
struct ServerCommandReceiver(Receiver<ServerCommand>);

impl Plugin for WorldServerChannelPlugin {
    fn build(&self, app: &mut App) {
        let (sender, receiver) = unbounded();

        app.insert_resource(ServerCommandSender(sender))
            .insert_resource(ServerCommandReceiver(receiver))
            .add_systems(
                Update,
                apply_server_commands.before(WorldSet::LandscapeUpdate),
            );
    }
}

fn apply_server_commands(world: &mut World) {
    let cmds = world
        .resource_mut::<ServerCommandReceiver>()
        .try_iter()
        .collect::<Vec<_>>();

    for cmd in cmds {
        process_server_command(cmd, world);
    }
}

fn process_server_command(cmd: ServerCommand, world: &mut World) {
    match cmd {
        ServerCommand::LandscapeUpdateCenter(center) => {
            let mut landscape = world.get_resource_or_insert_with::<Landscape>(Default::default);
            landscape.center = center;
        }
        ServerCommand::LandscapeUpdateRadius(radius) => {
            let mut landscape = world.get_resource_or_insert_with::<Landscape>(Default::default);
            landscape.radius = radius;
        }
        ServerCommand::LandscapeRemove => {
            world.remove_resource::<Landscape>();
        }
        ServerCommand::LandscapeAdd(center, radius) => {
            world.insert_resource(Landscape { center, radius });
        }
    }
}
