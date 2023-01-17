use std::time::Duration;

use crate::widget::{Widget, WidgetLabel};
use bevy::{prelude::*, ui::FocusPolicy};
use bevy_ecss::{Class, RegisterComponentSelector};
use bevy_ui_navigation::prelude::{FocusState, Focusable, NavRequest};

#[derive(SystemLabel)]
struct RemoveFocus;

pub(super) struct InputTextPlugin;

impl Plugin for InputTextPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<InputText>()
            .register_component_selector::<InputText>("input-text")
            .add_system(toggle_focus_visibility.label(RemoveFocus))
            .add_system(hide_caret_when_lose_focus.after(RemoveFocus))
            .add_system(update_text_section)
            .add_system(update_text_backspace)
            .add_system(update_text_characters)
            .add_system(update_text_caret);
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

#[derive(Component)]
struct InputTextMeta {
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
    fn build<L: WidgetLabel>(label: L, commands: &mut Commands) -> Entity {
        let input_panel = NodeBundle {
            focus_policy: FocusPolicy::Block,
            ..Default::default()
        };

        let input_text = commands
            .spawn(TextBundle::from_section("", default()))
            .insert(InputTextDisplayText)
            .insert(Class::new("value"))
            .id();

        let input_caret = commands
            .spawn(TextBundle::from_section("|", default()).with_style(Style {
                display: Display::None,
                ..Default::default()
            }))
            .insert(Focusable::default())
            .insert(InputTextDisplayCaret)
            .insert(Class::new("caret"))
            .id();

        let panel_bg = commands
            .spawn(NodeBundle {
                style: Style {
                    size: Size::new(Val::Percent(100.0), Val::Percent(100.0)),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::FlexStart,
                    ..Default::default()
                },
                focus_policy: FocusPolicy::Pass,
                ..Default::default()
            })
            .add_child(input_text)
            .add_child(input_caret)
            .insert(Class::new("background"))
            .id();

        commands
            .spawn(input_panel)
            .add_child(panel_bg)
            .insert(Name::new(label.name()))
            .insert(label)
            .insert(Focusable::new().blocked())
            .insert(InputText::default())
            .insert(InputTextMeta {
                text_entity: input_text,
                caret_entity: input_caret,
                caret_visible: false,
                caret_timer: Timer::from_seconds(0.5, TimerMode::Repeating),
            })
            .id()
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
