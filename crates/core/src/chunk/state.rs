use std::marker::PhantomData;

use bevy::ecs::component::Component;

pub trait State {}
#[derive(Default, Component, Debug)]
pub struct ChunkState<T: State = Unloaded>(PhantomData<T>);

pub struct Unloaded;
impl State for Unloaded {}
