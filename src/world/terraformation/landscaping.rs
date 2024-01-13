use bevy::{prelude::*, utils::HashSet};

use projekto_core::{chunk, query};
use projekto_genesis::{ChunkKindRes, GenesisCommandBuffer};

use super::{TerraformationCenter, TerraformationConfig};

pub(super) struct LandscapingPlugin;

impl Plugin for LandscapingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, update_landscape);
    }
}
#[derive(Default)]
struct UpdateLandscapeMeta {
    last_pos: IVec3,
    next_sync: f32,
}

fn update_landscape(
    time: Res<Time>,
    config: Res<TerraformationConfig>,
    kinds: Res<ChunkKindRes>,
    mut meta: Local<UpdateLandscapeMeta>,
    mut cmd_buffer: ResMut<GenesisCommandBuffer>,
    q: Query<&Transform, With<TerraformationCenter>>,
) {
    let center = match q.get_single() {
        Ok(t) => chunk::to_local(t.translation),
        Err(_) => return,
    };

    meta.next_sync -= time.delta_seconds();

    if center != meta.last_pos || meta.next_sync < 0.0 {
        meta.next_sync = 1.0;
        meta.last_pos = center;

        let radius = IVec3::new(
            config.horizontal_radius as i32,
            0,
            config.horizontal_radius as i32,
        );

        let begin = center - radius;
        let end = center + radius;

        let visible_range = query::range_inclusive(begin, end).collect::<HashSet<_>>();
        let existing_chunks = HashSet::from_iter(kinds.list_chunks());

        visible_range
            .iter()
            .filter(|&i| !existing_chunks.contains(i))
            .for_each(|&v| cmd_buffer.load(v));

        existing_chunks
            .iter()
            .filter(|&i| !visible_range.contains(i))
            .for_each(|&v| cmd_buffer.unload(v));
    }
}
