use bevy::{
    ecs::{event::Event, system::SystemParam},
    prelude::*,
};
use bevy_ui_navigation::DefaultNavigationPlugins;

use crate::{
    console::ConsolePlugin, input_text::InputTextPlugin, item_list::ItemListPlugin,
    theme::ThemePlugin,
};

pub struct WidgetPlugin;

impl Plugin for WidgetPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(DefaultNavigationPlugins)
            .add_plugin(ItemListPlugin)
            .add_plugin(InputTextPlugin)
            .add_plugin(ConsolePlugin)
            .add_plugin(ThemePlugin)
            // .add_plugin(ButtonPlugin)
            .init_resource::<WidgetSettings>()
            .register_type::<StringLabel>();
    }
}

#[derive(Default, Reflect, Debug)]
pub struct WidgetSettings {
    pub default_font: Handle<Font>,
}

pub trait Widget {
    type Theme: Component + Reflect + Default;

    fn build<L: WidgetLabel>(label: L, commands: &mut Commands) -> Entity;
}

pub trait WidgetLabel: Component + Reflect + Default {
    fn name(&self) -> String {
        self.get_type_info()
            .type_name()
            .split(':')
            .last()
            .unwrap()
            .to_string()
    }
}

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
pub struct StringLabel(String);

impl WidgetLabel for StringLabel {
    fn name(&self) -> String {
        self.0.clone()
    }
}

impl From<&str> for StringLabel {
    fn from(str: &str) -> Self {
        Self(str.to_string())
    }
}

pub trait ToStringLabel {
    fn label(self) -> StringLabel;
}

impl ToStringLabel for &str {
    fn label(self) -> StringLabel {
        self.into()
    }
}

pub trait WidgetEvent: Event {
    fn entity(&self) -> Entity;
}

pub struct WidgetEventIter<'s, E> {
    iter: Box<dyn Iterator<Item = &'s E>>,
}

impl<'s, E> Iterator for WidgetEventIter<'s, E> {
    type Item = &'s E;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}

#[derive(SystemParam)]
pub struct WidgetEventReader<'w, 's, T, E>
where
    T: WidgetLabel,
    E: WidgetEvent,
{
    reader: EventReader<'w, 's, E>,
    q: Query<'w, 's, &'static T, With<T>>,
    #[system_param(ignore)]
    _data: std::marker::PhantomData<T>,
}

impl<'w, 's, T: WidgetLabel, E: WidgetEvent> WidgetEventReader<'w, 's, T, E> {
    pub fn iter(&mut self) -> impl Iterator<Item = &E> {
        self.reader
            .iter()
            .filter(|evt| self.q.contains(evt.entity()))
    }
}

#[allow(dead_code)]
impl<'w, 's, E: WidgetEvent> WidgetEventReader<'w, 's, StringLabel, E> {
    pub fn filter(&mut self, event: &str) -> impl Iterator<Item = &E> {
        self.reader
            .iter()
            .filter(|evt| {
                if let Ok(label) = self.q.get(evt.entity()) {
                    label.0 == event
                } else {
                    false
                }
            })
            .collect::<Vec<&E>>()
            .into_iter()
    }
}
