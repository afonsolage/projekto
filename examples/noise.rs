use bevy::{
    input::mouse::{MouseMotion, MouseWheel},
    prelude::*,
    reflect::{ReflectRef, VariantField},
    render::{
        render_asset::RenderAssetUsages,
        render_resource::{
            Extent3d, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
        },
        texture::ImageSampler,
    },
    tasks::{block_on, poll_once, AsyncComputeTaskPool, Task},
    utils::HashMap,
};
use noise::NoiseFn;
use projekto_server::gen::noise::{NoiseFnSpec, NoiseStack, NoiseStackLoader};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .init_asset::<NoiseStack>()
        .init_asset_loader::<NoiseStackLoader>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                bevy::window::close_on_esc,
                update_noise_tree,
                update_noise_images,
                update_noise_specs,
                move_panel_node.run_if(on_event::<MouseMotion>()),
                zoom_root_node.run_if(on_event::<MouseWheel>()),
            ),
        )
        .run();
}

#[derive(Resource, Debug, Default)]
struct NoiseStackHandle(Handle<NoiseStack>);

#[derive(Component, Debug, Default, Reflect)]
struct NoiseImage(String);

#[derive(Component, Debug, Default, Reflect)]
struct NoiseSpec(String);

#[derive(Component, Debug, Default)]
struct PanelNode;

#[derive(Component, Debug, Default)]
struct RootNode;

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn(Camera2dBundle::default());
    commands
        .spawn((
            NodeBundle {
                style: Style {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    ..Default::default()
                },
                ..Default::default()
            },
            Name::new("Root"),
            RootNode,
        ))
        .with_children(|parent| {
            parent.spawn((
                NodeBundle {
                    style: Style {
                        display: Display::Grid,
                        grid_template_columns: vec![
                            GridTrack::flex(1.0),
                            GridTrack::px(5.0),
                            GridTrack::min_content(),
                        ],
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        left: Val::Px(0.0),
                        top: Val::Px(0.0),
                        position_type: PositionType::Absolute,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                Name::new("Panel"),
                PanelNode,
            ));
        });

    let stack = asset_server.load("noises/world_surface.ron");
    commands.insert_resource(NoiseStackHandle(stack));
}

fn update_noise_tree(
    mut commands: Commands,
    q_panel: Query<Entity, With<PanelNode>>,
    mut events: EventReader<AssetEvent<NoiseStack>>,
    assets: Res<Assets<NoiseStack>>,
) {
    let Ok(panel) = q_panel.get_single() else {
        return;
    };

    for evt in events.read() {
        match evt {
            AssetEvent::Added { id } | AssetEvent::Modified { id } => {
                commands.entity(panel).despawn_descendants();
                let Some(stack) = assets.get(*id) else {
                    continue;
                };

                let tree = Tree::new(stack, "main");
                spawn_noise_ui_dependency_tree(panel, &tree, &mut commands);
            }
            _ => continue,
        }
    }
}

fn spawn_noise_ui_dependency_tree(parent: Entity, tree: &Tree, commands: &mut Commands) {
    let card_width = 512.0;
    let card_height = 300.0;

    for node in &tree.nodes {
        commands.entity(parent).with_children(|parent| {
            parent
                .spawn((
                    NodeBundle {
                        style: Style {
                            position_type: PositionType::Absolute,
                            width: Val::Auto,
                            height: Val::Auto,
                            left: Val::Px(card_width * node.x),
                            top: Val::Px(card_height * node.y),
                            display: Display::Grid,
                            grid_template_columns: vec![
                                GridTrack::min_content(),
                                GridTrack::px(5.0),
                                GridTrack::min_content(),
                            ],
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    Name::new(format!("Noise {}", node.name).to_string()),
                ))
                .with_children(|parent| {
                    parent.spawn((
                        NodeBundle {
                            style: Style {
                                width: Val::Px(200.0),
                                height: Val::Auto,
                                ..Default::default()
                            },
                            background_color: Color::BLACK.into(),
                            ..Default::default()
                        },
                        Name::new("Noise Spec"),
                        NoiseSpec(node.name.to_string()),
                    ));

                    parent.spawn((
                        NodeBundle {
                            style: Style {
                                height: Val::Auto,
                                ..Default::default()
                            },
                            background_color: Color::GRAY.into(),
                            ..Default::default()
                        },
                        Name::new("Noise Spacer"),
                    ));

                    parent.spawn((
                        NodeBundle {
                            style: Style {
                                width: Val::Px(256.0),
                                height: Val::Px(256.0),
                                ..Default::default()
                            },
                            background_color: Color::DARK_GRAY.into(),
                            ..Default::default()
                        },
                        Name::new("Noise Image"),
                        NoiseImage(node.name.to_string()),
                    ));
                });

            // TODO: Figureout why we need this 26 offset to center the connection
            let origin = Vec2::new(
                node.x * card_width + card_width / 2.0 - 26.0,
                node.y * card_height + card_height / 2.0,
            );

            for child in &node.children {
                let dest = Vec2::new(
                    tree.nodes[*child].x * card_width + card_width / 2.0 - 26.0,
                    tree.nodes[*child].y * card_height + card_height / 2.0,
                );
                spawn_connection(parent, origin, dest);
            }
        });
    }
}

fn spawn_connection(parent: &mut ChildBuilder, origin: Vec2, dest: Vec2) {
    fn spawn_line(parent: &mut ChildBuilder, left: f32, top: f32, width: f32, height: f32) {
        parent.spawn(NodeBundle {
            style: Style {
                position_type: PositionType::Absolute,
                left: Val::Px(left),
                top: Val::Px(top),
                width: Val::Px(width),
                height: Val::Px(height),
                ..Default::default()
            },
            // z_index: ZIndex::Global(-10),
            background_color: Color::LIME_GREEN.into(),
            ..Default::default()
        });
    }

    let height = dest.y - origin.y;
    if (origin.x - dest.x).abs() >= f32::EPSILON {
        // Draw two connections

        let left = f32::min(origin.x, dest.x);
        let width = (origin.x - dest.x).abs();
        spawn_line(parent, left - 0.5, origin.y, width, 5.0);

        spawn_line(parent, dest.x - 0.5, origin.y, 5.0, height);
    } else {
        spawn_line(parent, origin.x - 0.5, origin.y, 5.0, height);
    }
}

fn zoom_root_node(
    mut mouse_wheel: EventReader<MouseWheel>,
    mut q: Query<&mut Transform, With<RootNode>>,
) {
    let Ok(ref mut transform) = q.get_single_mut() else {
        return;
    };

    for MouseWheel { y, .. } in mouse_wheel.read() {
        let scaled = transform.scale + y * 0.1;
        transform.scale = scaled.clamp(Vec3::splat(0.1), Vec3::splat(10.0));
    }
}

fn move_panel_node(
    mut q: Query<&mut Style, With<PanelNode>>,
    mut q_root: Query<&Transform, With<RootNode>>,
    input: Res<ButtonInput<MouseButton>>,
    mut mouse_motion: EventReader<MouseMotion>,
) {
    let Ok(ref mut style) = q.get_single_mut() else {
        return;
    };

    let Ok(root_transform) = q_root.get_single_mut() else {
        return;
    };

    if input.pressed(MouseButton::Middle) {
        for &MouseMotion { delta } in mouse_motion.read() {
            let scale_factor = 1.0 / root_transform.scale.x;

            let left = -delta.x * scale_factor;
            let top = -delta.y * scale_factor;

            move_left_top(style, left, top);
        }
    }
}

fn move_left_top(style: &mut Style, left: f32, top: f32) {
    let Val::Px(current_left) = style.left else {
        unreachable!()
    };

    let Val::Px(current_top) = style.top else {
        unreachable!()
    };

    style.left = Val::Px(current_left - left);
    style.top = Val::Px(current_top - top);
}

fn update_noise_specs(
    mut commands: Commands,
    q: Query<(Entity, &NoiseSpec), Added<NoiseSpec>>,
    handle: Res<NoiseStackHandle>,
    assets: Res<Assets<NoiseStack>>,
) {
    let Some(stack) = assets.get(&handle.0) else {
        return;
    };

    for (entity, NoiseSpec(name)) in &q {
        commands.entity(entity).despawn_descendants();

        let spec = stack.get_spec(name).unwrap();

        create_spec_node(&mut commands, entity, name, spec);
    }
}

fn create_spec_node(commands: &mut Commands, parent: Entity, name: &str, spec: &NoiseFnSpec) {
    let panel = commands
        .spawn((
            NodeBundle {
                style: Style {
                    width: Val::Percent(100.0),
                    display: Display::Grid,
                    grid_template_columns: vec![GridTrack::auto()],
                    grid_template_rows: vec![GridTrack::min_content()],
                    margin: UiRect::all(Val::Px(3.0)),
                    align_self: AlignSelf::Start,
                    ..Default::default()
                },
                ..Default::default()
            },
            Name::new("Noise Spec Panel"),
        ))
        .with_children(|parent| {
            parent.spawn(TextBundle {
                style: Style {
                    width: Val::Percent(100.0),
                    ..Default::default()
                },
                text: Text::from_section(
                    name,
                    TextStyle {
                        font_size: 20.0,
                        color: Color::WHITE,
                        ..Default::default()
                    },
                ),
                ..Default::default()
            });

            parent.spawn((
                NodeBundle {
                    style: Style {
                        width: Val::Percent(100.0),
                        height: Val::Px(2.0),
                        ..Default::default()
                    },
                    background_color: Color::GRAY.into(),
                    ..Default::default()
                },
                Name::new("Spacer"),
            ));
        })
        .id();

    for item in create_spec_items(commands, spec) {
        commands.entity(panel).add_child(item);
    }

    commands.entity(parent).add_child(panel);
}

fn create_spec_items(commands: &mut Commands, spec: &NoiseFnSpec) -> Vec<Entity> {
    let ReflectRef::Enum(spec_ref) = spec.reflect_ref() else {
        unreachable!();
    };

    let type_item = commands
        .spawn(TextBundle {
            style: Style {
                display: Display::Grid,
                width: Val::Percent(100.0),
                ..Default::default()
            },
            text: Text::from_section(
                format!("type: {}", spec_ref.variant_name()),
                TextStyle {
                    ..Default::default()
                },
            ),
            ..Default::default()
        })
        .id();

    std::iter::once(type_item)
        .chain(
            spec_ref
                .iter_fields()
                .map(|field| create_spec_item_field(commands, field)),
        )
        .collect()
}

fn create_spec_item_field(commands: &mut Commands, field: VariantField) -> Entity {
    let name = field.name().unwrap_or("value");

    let mut sections = match field.value().reflect_ref() {
        ReflectRef::List(list) => list
            .iter()
            .map(|value| TextSection::new(format!("\n\t{:?}", value), TextStyle::default()))
            .collect(),
        _ => {
            vec![TextSection::new(
                format!("{:?}", field.value()),
                TextStyle::default(),
            )]
        }
    };

    sections.insert(
        0,
        TextSection::new(format!("{name}: "), TextStyle::default()),
    );

    commands
        .spawn(TextBundle {
            style: Style {
                display: Display::Grid,
                width: Val::Percent(100.0),
                ..Default::default()
            },
            text: Text::from_sections(sections),
            ..Default::default()
        })
        .id()
}

fn update_noise_images(
    mut commands: Commands,
    q: Query<(Entity, &NoiseImage), Added<NoiseImage>>,
    mut images: ResMut<Assets<Image>>,
    handle: Res<NoiseStackHandle>,
    assets: Res<Assets<NoiseStack>>,
    mut running_tasks: Local<HashMap<Entity, Task<Vec<u8>>>>,
) {
    for (entity, task) in running_tasks.iter_mut() {
        if let Some(buffer) = block_on(poll_once(task)) {
            let handle = images.add(create_image(512, 512, buffer));

            commands
                .entity(*entity)
                .insert((UiImage::new(handle), BackgroundColor(Color::WHITE)));
        }
    }

    running_tasks.retain(|_, task| !task.is_finished());

    let Some(stack) = assets.get(&handle.0) else {
        return;
    };

    for (entity, NoiseImage(name)) in &q {
        let noise = stack.build(name);

        let task = AsyncComputeTaskPool::get().spawn(async move {
            // 512w, 512h, 1 byte per color, 4 color channel (RGBA)
            let mut buffer = vec![0; 4 * 512 * 512];
            let mut min = u8::MAX;
            let mut max = 0;

            for w in 0..512 {
                for h in 0..512 {
                    let i = ((w * 512 + h) * 4) as usize;
                    let b = &mut buffer[i..i + 4];
                    let x = w as f64 / 512.0;
                    let y = h as f64 / 512.0;

                    let noise = noise.get([x * 0.001, y * 0.001, 0.0]) as f32;
                    let height = (((noise + 1.0) / 2.0) * 255.0) as u8;

                    if height > max {
                        max = height;
                    } else if height < min {
                        min = height;
                    }

                    b.copy_from_slice(&[height, height, height, u8::MAX]);
                }
            }

            buffer
        });

        running_tasks.insert(entity, task);
    }
}

fn create_image(width: u32, height: u32, buffer: Vec<u8>) -> Image {
    Image {
        data: buffer,
        texture_descriptor: TextureDescriptor {
            label: None,
            size: Extent3d {
                width,
                height,
                ..Default::default()
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8UnormSrgb,
            view_formats: &[],
            usage: TextureUsages::TEXTURE_BINDING,
        },
        sampler: ImageSampler::nearest(),
        asset_usage: RenderAssetUsages::RENDER_WORLD,
        ..Default::default()
    }
}

#[derive(Debug, Default)]
struct Tree<'n> {
    nodes: Vec<TreeNode<'n>>,
}

#[derive(Default, Debug, Clone)]
struct TreeNode<'n> {
    pub name: &'n str,
    pub x: f32,
    pub y: f32,
    pub children: Vec<usize>,
    pub parent: Option<usize>,
    x_offset: f32,
}

#[derive(Copy, Clone)]
enum ContourDir {
    Left,
    Right,
}

impl<'n> Tree<'n> {
    pub fn new(stack: &'n NoiseStack, root: &'n str) -> Tree<'n> {
        let mut tree = Tree::default();

        tree.add_spec_node(stack, root, None, 0.0, 0.0);

        tree.sort_post_order(root);
        tree.compute_layout();

        tree
    }

    fn compute_layout(&mut self) {
        if self.nodes.is_empty() {
            return;
        }

        let root_id = self.nodes.len() - 1;
        self.compute_initial_x(root_id);
        self.compute_apply_x_offset(0.0, root_id);
    }

    fn compute_initial_x(&mut self, id: usize) {
        let children = self.nodes[id].children.clone();

        for child in &children {
            self.compute_initial_x(*child);
        }

        let left_sibling_offset = self.get_left_sibling(id).map(|n| n.x + 1.0);

        match children.len() {
            0 => {
                if let Some(offset) = left_sibling_offset {
                    self.nodes[id].x = offset;
                } else {
                    self.nodes[id].x = 0.0;
                }
            }
            1 => {
                if let Some(offset) = left_sibling_offset {
                    self.nodes[id].x = offset;
                    self.nodes[id].x_offset = self.nodes[id].x - self.nodes[children[0]].x;
                } else {
                    self.nodes[id].x = self.nodes[children[0]].x;
                }
            }
            _ => {
                let left_most = self.nodes[*children.first().unwrap()].x;
                let right_most = self.nodes[*children.last().unwrap()].x;
                let mid = (left_most + right_most) / 2.0;

                if let Some(offset) = left_sibling_offset {
                    self.nodes[id].x = offset;
                    self.nodes[id].x_offset = self.nodes[id].x - mid;
                } else {
                    self.nodes[id].x = mid;
                }
            }
        }

        if !children.is_empty() && left_sibling_offset.is_some() {
            self.fix_overlapping(id);
        }
    }

    fn fix_overlapping(&mut self, id: usize) {
        let left_siblings = self.nodes[id]
            .parent
            .map(|parent| {
                self.nodes[parent]
                    .children
                    .iter()
                    .take_while(|&&c| c != id)
                    .copied()
                    .collect::<Vec<_>>()
            })
            .unwrap();

        assert!(!left_siblings.is_empty());

        let mut node_left_contour = HashMap::new();
        self.get_contour(ContourDir::Left, id, 0.0, &mut node_left_contour);

        let node_depth = self.nodes[id].y as usize;

        for sibling in left_siblings {
            assert_ne!(sibling, id);

            let mut sibling_right_contour = HashMap::new();
            self.get_contour(ContourDir::Right, sibling, 0.0, &mut sibling_right_contour);

            let max_contour_depth = usize::min(
                *node_left_contour.keys().max().unwrap(),
                *sibling_right_contour.keys().max().unwrap(),
            );

            let shift_distance = (node_depth + 1..=max_contour_depth).fold(0.0, |acc, depth| {
                let left = node_left_contour.get(&depth).unwrap();
                let right = sibling_right_contour.get(&depth).unwrap();
                let distance = left - right;

                if distance + acc < 1.0 {
                    1.0 - distance
                } else {
                    acc
                }
            });

            if shift_distance > 0.0 {
                self.nodes[id].x += shift_distance;
                self.nodes[id].x_offset += shift_distance;

                self.center_nodes_between(id, sibling);
            }
        }
    }

    fn center_nodes_between(&mut self, left: usize, right: usize) {
        assert_ne!(left, right);
        assert_eq!(self.nodes[left].parent, self.nodes[right].parent);

        let siblings = self.nodes[self.nodes[left].parent.unwrap()]
            .children
            .clone();
        let left_idx = siblings.iter().position(|c| *c == left).unwrap();
        let right_idx = siblings.iter().position(|c| *c == left).unwrap();

        let nodes_between = right_idx - left_idx;

        if nodes_between <= 1 {
            return;
        }

        let step_distance = (self.nodes[left].x - self.nodes[right].x) / nodes_between as f32;

        for (i, mid_node_idx) in (left_idx + 1..right_idx).enumerate() {
            let mid_node = siblings[mid_node_idx];

            let target_x = self.nodes[right].x + (step_distance * i as f32);
            let offset = target_x - self.nodes[mid_node].x;

            self.nodes[mid_node].x += offset;
            self.nodes[mid_node].x_offset += offset;
        }

        self.fix_overlapping(left);
    }

    fn get_contour(
        &self,
        dir: ContourDir,
        id: usize,
        parent_offset: f32,
        depth_map: &mut HashMap<usize, f32>,
    ) {
        let TreeNode {
            y,
            x,
            x_offset,
            children,
            ..
        } = &self.nodes[id];

        let depth = *y as usize;
        let contour = *x + parent_offset;

        if let Some(existing) = depth_map.get(&depth) {
            let new_contour = match dir {
                ContourDir::Left => f32::min(*existing, contour),
                ContourDir::Right => f32::max(*existing, contour),
            };
            depth_map.insert(depth, new_contour);
        } else {
            depth_map.insert(depth, contour);
        }

        for &child in children {
            self.get_contour(dir, child, parent_offset + *x_offset, depth_map);
        }
    }

    fn get_left_sibling(&self, id: usize) -> Option<&TreeNode> {
        let siblings = self.nodes[id].parent.map(|p| &self.nodes[p].children)?;

        if siblings[0] == id {
            return None;
        }

        let index = siblings
            .iter()
            .position(|sib| *sib == id)
            .expect("id must exists on parent children");

        assert!(index > 0);

        Some(&self.nodes[siblings[index - 1]])
    }

    fn compute_apply_x_offset(&mut self, offset: f32, id: usize) {
        self.nodes[id].x += offset;
        let children = self.nodes[id].children.clone();

        for child in children {
            self.compute_apply_x_offset(offset + self.nodes[id].x_offset, child);
        }
    }

    #[cfg(test)]
    fn add_node(&mut self, parent: Option<&'n str>, name: &'n str) {
        let parent = parent.and_then(|p| self.nodes.iter().position(|n| n.name == p));

        self.nodes.push(TreeNode {
            name,
            parent,
            ..Default::default()
        });

        if let Some(parent) = parent {
            let child = self.nodes.len() - 1;
            self.nodes[parent].children.push(child);
        }
    }

    fn sort_post_order(&mut self, root: &'n str) {
        fn add_post_order(tree: &Tree, id: usize, nodes: &mut Vec<usize>) {
            for child in &tree.nodes[id].children {
                add_post_order(tree, *child, nodes);
            }

            nodes.push(id);
        }

        fn update_children(tree: &mut Tree, id: usize, ordered_nodes: &[usize]) {
            for child in tree.nodes[id].children.clone() {
                update_children(tree, child, ordered_nodes);
                tree.nodes[child].parent = ordered_nodes.iter().position(|new_id| id == *new_id);
            }

            tree.nodes[id].children.iter_mut().for_each(|child| {
                *child = ordered_nodes
                    .iter()
                    .position(|new_id| *new_id == *child)
                    .unwrap();
            });
        }

        let root = self
            .nodes
            .iter()
            .position(|n| n.name == root)
            .expect("root should exists");

        let mut nodes = vec![];
        add_post_order(self, root, &mut nodes);
        let mut ordered_nodes = Vec::with_capacity(nodes.len());

        update_children(self, root, &nodes);

        for id in nodes {
            ordered_nodes.push(std::mem::take(&mut self.nodes[id]));
        }

        self.nodes = ordered_nodes;
    }

    fn add_spec_node(
        &mut self,
        stack: &'n NoiseStack,
        name: &'n str,
        parent: Option<usize>,
        x: f32,
        y: f32,
    ) -> usize {
        let spec = stack.get_spec(name).unwrap();

        let node = TreeNode {
            name,
            parent,
            x,
            y,
            ..Default::default()
        };

        let children = spec
            .dependencies()
            .into_iter()
            .enumerate()
            .map(|(x, dep)| self.add_spec_node(stack, dep, None, x as f32, y + 1.0))
            .collect::<Vec<_>>();

        let id = self.nodes.len();
        self.nodes.push(node);

        for child in children.iter() {
            self.nodes[*child].parent = Some(id);
        }

        self.nodes[id].children = children;

        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_tree<'a>() -> Tree<'a> {
        let mut tree = Tree::default();

        tree.add_node(None, "O");

        tree.add_node(Some("O"), "E");
        tree.add_node(Some("O"), "F");
        tree.add_node(Some("O"), "N");

        tree.add_node(Some("E"), "A");
        tree.add_node(Some("E"), "D");

        tree.add_node(Some("D"), "B");
        tree.add_node(Some("D"), "C");

        tree.add_node(Some("N"), "G");
        tree.add_node(Some("N"), "M");

        tree.add_node(Some("M"), "H");
        tree.add_node(Some("M"), "I");
        tree.add_node(Some("M"), "J");
        tree.add_node(Some("M"), "K");
        tree.add_node(Some("M"), "L");

        tree.sort_post_order("O");

        tree
    }

    #[test]
    fn sort_post_order() {
        let mut tree = create_test_tree();

        let expected = [
            "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O",
        ];

        for (id, name) in expected.into_iter().enumerate() {
            assert_eq!(tree.nodes[id].name, name);
        }
    }

    #[test]
    fn compute_initial_x() {
        let mut tree = create_test_tree();

        tree.compute_layout();
        // while (tree.fix_overlapping(tree.nodes.len() - 1)) {}

        for &TreeNode { name, x, .. } in &tree.nodes {
            println!("{name} = {x}");
        }

        assert!(false);
    }
}
