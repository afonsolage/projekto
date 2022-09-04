use bevy::{
    prelude::*,
    render::{RenderApp, RenderStage, extract_component::{ExtractComponentPlugin, ExtractComponent}, Extract},
    sprite::Rect,
    ui::{ExtractedUiNode, ExtractedUiNodes, RenderUiSystem},
};

pub struct UiClipAreaPlugin;

impl Plugin for UiClipAreaPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<UiClipArea>()
        .add_plugin(ExtractComponentPlugin::<UiClipArea>::extract_visible());

        hook_ui_render_pipeline(app);
    }
}

#[derive(Debug, Default, SystemLabel)]
pub struct UiClipAreaLabel;

fn hook_ui_render_pipeline(app: &mut App) {
    let render_app = match app.get_sub_app_mut(RenderApp) {
        Ok(render_app) => render_app,
        Err(_) => panic!("No render app found!"),
    };

    render_app.add_system_to_stage(
        RenderStage::Extract,
        calc_clip_area
            .label(UiClipAreaLabel)
            .after(RenderUiSystem::ExtractNode),
    );
}

#[derive(Component, Default, Reflect, Clone, Copy)]
#[reflect(Component)]
pub struct UiClipArea(pub Rect);

impl ExtractComponent for UiClipArea {
    type Query = &'static Self;

    type Filter = With<Node>;

    fn extract_component(item: bevy::ecs::query::QueryItem<Self::Query>) -> Self {
        *item
    }
}

fn calc_clip_area(
    mut extracted_uinodes: ResMut<ExtractedUiNodes>,
    clip_area_query: Extract<Query<(&GlobalTransform, &UiClipArea), With<Node>>>,
) {
    for (transform, clip_area) in clip_area_query.iter() {
        if let Some(node) = find_extracted_ui_node(transform, &mut extracted_uinodes) 
            && node.clip.is_none() {
            node.clip = Some(clip_area.0);
        }
    }
}

// There is no current way to identify which Entity is which ExtractUiNode.
// So let's use transform to do out best.
fn find_extracted_ui_node<'a>(
    transform: &'a GlobalTransform,
    nodes: &'a mut ExtractedUiNodes,
) -> Option<&'a mut ExtractedUiNode> {
    let mat = transform.compute_matrix();
    for node in nodes.uinodes.iter_mut() {
        if node.transform == mat {
            return Some(node);
        }
    }

    None
}
