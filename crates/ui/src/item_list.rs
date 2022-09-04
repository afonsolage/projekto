use bevy::{prelude::*, ui::FocusPolicy};

use crate::{
    theme::{
        ApplyThemeStyle, ApplyThemeText, ThemeStyle, ThemeStyleProperty, ThemeText,
        ThemeTextProperty,
    },
    widget::{Widget, WidgetLabel, WidgetSettings},
};

const ITEM_HEIGHT: f32 = 20.0;

pub(super) struct ItemListPlugin;

impl Plugin for ItemListPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<ItemList>()
            .register_type::<ItemListTheme>()
            .register_type::<ItemIndex>()
            .add_system(update_item_list_items)
            .add_system(update_item_list_max_visible_items)
            .add_system(apply_theme);
    }
}

#[derive(Component, Debug, Reflect, Clone)]
#[reflect(Component)]
pub struct ItemListTheme {
    item_size: Size<Val>,
    item_font_size: f32,
    item_font_color: Color,
    item_font: Handle<Font>,

    background_border: UiRect<Val>,
    background_color: Color,

    border: UiRect<Val>,
    border_color: Color,
}

impl ItemListTheme {
    fn apply_defaults(&mut self, settings: &WidgetSettings) {
        if self.item_font == Default::default() {
            self.item_font = settings.default_font.clone();
        }
    }
}

impl Default for ItemListTheme {
    fn default() -> Self {
        Self {
            item_size: Size::new(Val::Percent(100.0), Val::Px(20.0)),
            item_font_size: 15.0,
            item_font_color: Color::rgb(0.9, 0.9, 0.9),
            item_font: Default::default(),
            background_border: UiRect::all(Val::Px(5.0)),
            background_color: Color::rgba(0.5, 0.5, 0.5, 0.1),
            border: UiRect::all(Val::Px(2.0)),
            border_color: Color::rgba(0.5, 0.5, 0.5, 0.1),
        }
    }
}

#[derive(Component)]
struct ItemListMeta {
    container_entity: Entity,
    max_visible_items: usize,
}

#[derive(Component, Default, Reflect)]
#[reflect(Component)]
struct ItemIndex(usize);

#[derive(Component, Default, Reflect)]
#[reflect(Component)]
pub struct ItemList {
    pub items: Vec<String>,
}

#[derive(Component)]
struct ItemListContainer;

impl Widget for ItemList {
    type Theme = ItemListTheme;

    fn build<L: WidgetLabel>(label: L, commands: &mut Commands) -> Entity {
        let list_bg = commands
            .spawn_bundle(NodeBundle {
                style: Style {
                    size: Size::new(Val::Percent(100.0), Val::Percent(100.0)),
                    border: UiRect::all(Val::Px(5.0)),
                    flex_direction: FlexDirection::Column,
                    flex_shrink: 0.0,
                    ..default()
                },
                focus_policy: FocusPolicy::Pass,
                color: Color::rgba(0.1, 0.1, 0.1, 0.9).into(),
                ..default()
            })
            .insert(ItemListContainer)
            .id();

        commands
            .spawn_bundle(NodeBundle {
                style: Style {
                    size: Size::new(Val::Percent(100.0), Val::Percent(100.0)),
                    border: UiRect::all(Val::Px(2.0)),
                    flex_direction: FlexDirection::Column,
                    ..default()
                },
                focus_policy: FocusPolicy::Pass,
                color: Color::rgba(0.5, 0.5, 0.5, 0.1).into(),
                ..default()
            })
            .add_child(list_bg)
            .insert(Name::new(label.name()))
            .insert(label)
            .insert(ItemList::default())
            .insert(ItemListMeta {
                container_entity: list_bg,
                max_visible_items: 0,
            })
            .insert(Self::Theme::default())
            .id()
    }
}

fn update_item_list_items(
    mut commands: Commands,
    q: Query<(&ItemList, &ItemListMeta, &ItemListTheme), Changed<ItemList>>,
    q_containers: Query<&Children, With<ItemListContainer>>,
    mut q_items: Query<(Entity, &mut Text), With<ItemIndex>>,
) {
    for (item_list, meta, theme) in &q {
        let children = q_containers.get(meta.container_entity).ok();

        // Sync children with item list items
        for (index, item) in item_list.items.iter().rev().enumerate() {
            if index >= meta.max_visible_items {
                break;
            }

            let item_entity = match children {
                Some(children) if index < children.len() => {
                    let (entity, mut text) = q_items
                        .get_mut(children[index])
                        .expect("Child item should exists");
                    text.sections[0].value = item.clone();
                    entity
                }
                _ => {
                    let item = commands
                        .spawn_bundle(create_item_bundle(item.clone(), theme))
                        .id();
                    commands.entity(meta.container_entity).add_child(item);
                    item
                }
            };

            commands
                .entity(item_entity)
                .insert(ItemIndex(index))
                .insert(Name::new(format!("Item {index}")));
        }

        // Remove unused children
        if let Some(children) = children {
            for i in (item_list.items.len() - 1)..children.len() {
                commands.entity(children[i]).despawn();
            }
        }
    }
}

fn create_item_bundle(content: String, theme: &ItemListTheme) -> TextBundle {
    TextBundle::from_section(
        content,
        TextStyle {
            font: theme.item_font.clone(),
            font_size: theme.item_font_size,
            color: theme.item_font_color,
        },
    )
    .with_style(Style {
        flex_shrink: 0.0,
        size: theme.item_size,
        ..default()
    })
}

fn update_item_list_max_visible_items(
    mut q: Query<&mut ItemListMeta, (With<ItemList>, Changed<Node>)>,
    q_containers: Query<&Node, With<ItemListContainer>>,
) {
    for mut meta in &mut q {
        if let Ok(container_node) = q_containers.get(meta.container_entity) {
            meta.max_visible_items = (container_node.size.y / ITEM_HEIGHT) as usize;
        }
    }
}

fn apply_theme(
    mut commands: Commands,
    q_themes: Query<
        (Entity, &Children, &ItemListMeta, &ItemListTheme),
        (With<ItemList>, Changed<ItemListTheme>),
    >,
    q_items: Query<With<ItemIndex>>,
    settings: Res<WidgetSettings>,
    mut style_theme_writer: EventWriter<ApplyThemeStyle>,
    mut text_theme_writer: EventWriter<ApplyThemeText>,
) {
    for (entity, children, meta, theme) in &q_themes {
        let mut theme = theme.clone();
        theme.apply_defaults(&settings);

        for &child in children {
            if !q_items.contains(child) {
                continue;
            }

            text_theme_writer.send(ApplyThemeText(
                child,
                ThemeText(vec![
                    ThemeTextProperty::Font(theme.item_font.clone()),
                    ThemeTextProperty::Size(theme.item_font_size),
                    ThemeTextProperty::Color(theme.item_font_color),
                ]),
            ));

            style_theme_writer.send(ApplyThemeStyle(
                child,
                ThemeStyle(vec![ThemeStyleProperty::Size(theme.item_size)]),
            ));
        }

        style_theme_writer.send(ApplyThemeStyle(
            meta.container_entity,
            ThemeStyle(vec![ThemeStyleProperty::Border(theme.background_border)]),
        ));

        style_theme_writer.send(ApplyThemeStyle(
            entity,
            ThemeStyle(vec![ThemeStyleProperty::Border(theme.border)]),
        ));

        commands
            .entity(meta.container_entity)
            .insert(UiColor(theme.background_color));
        commands.entity(entity).insert(UiColor(theme.border_color));
    }
}
