use bevy::{
    ecs::{
        query::{QueryItem, ReadOnlyWorldQuery, WorldQuery},
        system::SystemParam,
    },
    prelude::*,
    utils::HashMap,
};
use projekto_core::chunk::Chunk;

#[derive(Resource, Default, Debug, Clone, Deref, DerefMut)]
pub(crate) struct ChunkMap(HashMap<Chunk, Entity>);

#[derive(SystemParam)]
pub(crate) struct ChunkQuery<'w, 's, Q: WorldQuery + 'static, F: ReadOnlyWorldQuery + 'static = ()>
{
    map: Res<'w, ChunkMap>,
    query: Query<'w, 's, Q, F>,
}

impl<'w, 's, Q: WorldQuery + 'static, F: ReadOnlyWorldQuery + 'static> ChunkQuery<'w, 's, Q, F> {
    // fn get_chunk_entity(&self, chunk: IVec3) -> Option<Entity> {
    //     self.map.0.get(&chunk).copied()
    // }

    pub fn get_chunk(&self, chunk: Chunk) -> Option<QueryItem<'_, <Q as WorldQuery>::ReadOnly>> {
        self.map.0.get(&chunk).map(|&entity| {
            self.query
                .get(entity)
                .expect("All entities inside the map must exists")
        })
    }

    pub fn get_chunk_mut(&mut self, chunk: Chunk) -> Option<Q::Item<'_>> {
        self.map.0.get(&chunk).map(|&entity| {
            self.query
                .get_mut(entity)
                .expect("All entities inside the map must exists")
        })
    }

    pub fn get_chunk_component<T: Component>(&self, chunk: Chunk) -> Option<&T> {
        if let Some(&entity) = self.map.0.get(&chunk) {
            if let Ok(component) = self.query.get_component::<T>(entity) {
                return Some(component);
            }
        }
        None
    }

    pub fn chunk_exists(&self, chunk: Chunk) -> bool {
        self.map.0.contains_key(&chunk)
    }

    // fn get_chunk_component_mut<T: Component>(&mut self, chunk: IVec3) -> Option<Mut<'_, T>> {
    //     if let Some(&entity) = self.map.0.get(&chunk) {
    //         if let Ok(component) = self.query.get_component_mut::<T>(entity) {
    //             return Some(component);
    //         }
    //     }
    //     None
    // }
}

impl<'w, 's, Q: WorldQuery + 'static, F: ReadOnlyWorldQuery + 'static> std::ops::Deref
    for ChunkQuery<'w, 's, Q, F>
{
    type Target = Query<'w, 's, Q, F>;

    fn deref(&self) -> &Self::Target {
        &self.query
    }
}

impl<'w, 's, Q: WorldQuery + 'static, F: ReadOnlyWorldQuery + 'static> std::ops::DerefMut
    for ChunkQuery<'w, 's, Q, F>
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.query
    }
}
