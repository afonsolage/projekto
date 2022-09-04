use std::time::Duration;

use crate::{
    theme::{
        ApplyThemeStyle, ApplyThemeText, ThemeStyle, ThemeStyleProperty, ThemeText,
        ThemeTextProperty,
    },
    widget::{Widget, WidgetLabel, WidgetSettings},
};
use bevy::{prelude::*, ui::FocusPolicy};
use bevy_ui_navigation::prelude::{FocusState, Focusable, NavRequest};

#[derive(SystemLabel)]
struct RemoveFocus;

pub(super) struct InputTextPlugin;

impl Plugin for InputTextPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<InputText>()
            .add_system(toggle_focus_visibility.label(RemoveFocus))
            .add_system(hide_caret_when_lose_focus.after(RemoveFocus))
            .add_system(update_text_section)
            .add_system(update_text_backspace)
            .add_system(update_text_characters)
            .add_system(update_text_caret)
            .add_system(apply_theme);
    }
}

#[derive(Component, Default, Reflect)]
#[reflect(Component)]
pub struct InputText {
    text: String,
}

impl InputText {
    pub fn take(&mut self) -> String {
        std::mem::take(&mut self.text)
    }
}

#[derive(Component, Debug, Reflect, Clone)]
#[reflect(Component)]
pub struct InputTextTheme {
    pub size: Size<Val>,
    pub border: UiRect<Val>,
    pub border_color: Color,

    pub text_font: Handle<Font>,
    pub text_size: f32,
    pub text_color: Color,

    pub caret_font: Handle<Font>,
    pub caret_size: f32,
    pub caret_color: Color,

    pub bg_padding: UiRect<Val>,
    pub bg_color: Color,
}

impl InputTextTheme {
    fn apply_defaults(&mut self, settings: &WidgetSettings) {
        if self.text_font == Default::default() {
            self.text_font = settings.default_font.clone();
        }

        if self.caret_font == Default::default() {
            self.caret_font = settings.default_font.clone();
        }
    }
}

impl Default for InputTextTheme {
    fn default() -> Self {
        Self {
            size: Size::new(Val::Percent(100.0), Val::Px(20.0)),
            border: UiRect::all(Val::Px(2.0)),
            border_color: Color::rgba(0.5, 0.5, 0.5, 0.2),
            text_font: Default::default(),
            text_size: 15.0,
            text_color: Color::rgb(0.7, 0.7, 0.7),
            caret_font: Default::default(),
            caret_size: 15.0,
            caret_color: Color::rgb(0.9, 0.9, 0.9),
            bg_padding: UiRect::new(Val::Px(2.0), Val::Px(2.0), Val::Px(8.0), Val::Px(8.0)),
            bg_color: Color::rgba(0.1, 0.1, 0.1, 0.9),
        }
    }
}

#[derive(Component)]
struct InputTextMeta {
    text_panel_entity: Entity,
    text_entity: Entity,
    caret_entity: Entity,
    caret_visible: bool,
    caret_timer: Timer,
}

#[derive(Component)]
struct InputTextDisplayText;

#[derive(Component)]
struct InputTextDisplayCaret;

impl Widget for InputText {
    type Theme = InputTextTheme;

    fn build<L: WidgetLabel>(label: L, commands: &mut Commands) -> Entity {
        let input_panel = NodeBundle {
            focus_policy: FocusPolicy::Block,
            ..default()
        };

        let input_text = commands
            .spawn_bundle(TextBundle::from_section("", default()))
            .insert(InputTextDisplayText)
            .id();

        let input_caret = commands
            .spawn_bundle(TextBundle::from_section("|", default()).with_style(Style {
                display: Display::None,
                ..default()
            }))
            .insert(Focusable::default())
            .insert(InputTextDisplayCaret)
            .id();

        let panel_bg = commands
            .spawn_bundle(NodeBundle {
                style: Style {
                    size: Size::new(Val::Percent(100.0), Val::Percent(100.0)),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::FlexStart,
                    ..default()
                },
                focus_policy: FocusPolicy::Pass,
                ..default()
            })
            .add_child(input_text)
            .add_child(input_caret)
            .id();

        commands
            .spawn_bundle(input_panel)
            .add_child(panel_bg)
            .insert(Name::new(label.name()))
            .insert(label)
            .insert(Focusable::new().blocked())
            .insert(InputText::default())
            .insert(InputTextMeta {
                text_panel_entity: panel_bg,
                text_entity: input_text,
                caret_entity: input_caret,
                caret_visible: false,
                caret_timer: Timer::from_seconds(0.5, true),
            })
            .insert(Self::Theme::default())
            .id()
    }
}

fn apply_theme(
    mut commands: Commands,
    q_themes: Query<
        (Entity, &InputTextMeta, &InputTextTheme),
        (With<InputText>, Changed<InputTextTheme>),
    >,
    settings: Res<WidgetSettings>,
    mut style_theme_writer: EventWriter<ApplyThemeStyle>,
    mut text_theme_writer: EventWriter<ApplyThemeText>,
) {
    for (entity, meta, theme) in &q_themes {
        let mut theme = theme.clone();
        theme.apply_defaults(&settings);

        text_theme_writer.send(ApplyThemeText(
            meta.caret_entity,
            ThemeText(vec![
                ThemeTextProperty::Font(theme.caret_font),
                ThemeTextProperty::Size(theme.caret_size),
                ThemeTextProperty::Color(theme.caret_color),
            ]),
        ));

        text_theme_writer.send(ApplyThemeText(
            meta.text_entity,
            ThemeText(vec![
                ThemeTextProperty::Font(theme.text_font),
                ThemeTextProperty::Size(theme.text_size),
                ThemeTextProperty::Color(theme.text_color),
            ]),
        ));

        style_theme_writer.send(ApplyThemeStyle(
            entity,
            ThemeStyle(vec![
                ThemeStyleProperty::Size(theme.size),
                ThemeStyleProperty::Border(theme.border),
            ]),
        ));

        commands
            .entity(meta.text_panel_entity)
            .insert(UiColor(theme.bg_color));

        commands.entity(entity).insert(UiColor(theme.border_color));
    }
}

fn toggle_focus_visibility(
    mut q: Query<
        (&mut Focusable, &ComputedVisibility, &InputTextMeta),
        (With<InputText>, Changed<ComputedVisibility>),
    >,
    mut writer: EventWriter<NavRequest>,
) {
    for (mut focus, visibility, meta) in &mut q {
        if !visibility.is_visible() && focus.state() != FocusState::Blocked {
            if !focus.block() {
                // TODO: Change it later on when it's possible to remove focus.
                writer.send(NavRequest::FocusOn(meta.caret_entity));
            }
        } else if visibility.is_visible() && focus.state() == FocusState::Blocked {
            focus.unblock();
        }
    }
}

fn hide_caret_when_lose_focus(
    mut q: Query<(&InputTextMeta, &Focusable), (With<InputText>, Changed<Focusable>)>,
    mut q_caret: Query<&mut Style, With<InputTextDisplayCaret>>,
) {
    for (meta, focus) in &mut q {
        if let Ok(mut style) = q_caret.get_mut(meta.caret_entity) {
            if focus.state() != FocusState::Focused && style.display == Display::Flex {
                style.display = Display::None;
            }
        }
    }
}

fn update_text_section(
    q: Query<(&InputText, &InputTextMeta), Changed<InputText>>,
    mut q_child: Query<&mut Text, With<InputTextDisplayText>>,
) {
    for (input_text, meta) in &q {
        q_child
            .get_mut(meta.text_entity)
            .expect("Every InputText should have a text child")
            .sections[0]
            .value = input_text.text.clone();
    }
}

fn update_text_characters(
    mut q: Query<(&Focusable, &mut InputText)>,
    mut events: EventReader<ReceivedCharacter>,
) {
    for (focus, mut input_text) in &mut q {
        if focus.state() == FocusState::Focused {
            for evt in events.iter() {
                input_text.text.push(evt.char);
            }
        }
    }
}

fn update_text_backspace(
    mut q: Query<(&Focusable, &mut InputText)>,
    input_keycode: Res<Input<KeyCode>>,
    mut timer: Local<Timer>,
    time: Res<Time>,
) {
    for (focus, mut input_text) in &mut q {
        if focus.state() == FocusState::Focused {
            timer.tick(time.delta());

            let backspace = if input_keycode.pressed(KeyCode::Back) && timer.finished() {
                timer.set_duration(Duration::from_millis(100));
                timer.reset();
                true
            } else {
                input_keycode.just_pressed(KeyCode::Back)
            };

            if backspace {
                input_text.text.pop();
            }
        }
    }
}

fn update_text_caret(
    mut q: Query<(&Focusable, &mut InputTextMeta), With<InputText>>,
    mut q_caret: Query<&mut Style, With<InputTextDisplayCaret>>,
    time: Res<Time>,
) {
    for (focus, mut meta) in &mut q {
        if focus.state() == FocusState::Focused {
            meta.caret_timer.tick(time.delta());

            if meta.caret_timer.just_finished() {
                let style = &mut q_caret
                    .get_mut(meta.caret_entity)
                    .expect("Every InputText should have a caret child");

                meta.caret_visible = !meta.caret_visible;

                style.display = if meta.caret_visible {
                    Display::Flex
                } else {
                    Display::None
                };
            }
        }
    }
}
