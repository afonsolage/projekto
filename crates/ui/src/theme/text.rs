use bevy::prelude::*;

#[derive(Component, Debug, Clone)]
pub struct ApplyThemeText(pub Entity, pub ThemeText);

#[derive(Default, Debug, Clone)]
pub struct ThemeText(pub Vec<ThemeTextProperty>);

#[derive(Debug, Clone)]
pub enum ThemeTextProperty {
    VerticalAlign(VerticalAlign),
    HorizontalAlign(HorizontalAlign),
    Font(Handle<Font>),
    Size(f32),
    Color(Color),
}

pub(super) fn apply_theme_text(mut reader: EventReader<ApplyThemeText>, mut q: Query<&mut Text>) {
    for ApplyThemeText(entity, theme) in reader.iter() {
        if let Ok(ref mut text) = q.get_mut(*entity) {
            ThemeStyler::from(text).apply(theme.clone());
        }
    }
}

struct ThemeStyler<'a>(&'a mut Text);

impl<'a> ThemeStyler<'a> {
    pub fn from(text: &'a mut Text) -> ThemeStyler {
        Self(text)
    }

    pub fn apply(mut self, theme: ThemeText) {
        for property in theme.0 {
            self.apply_property(property);
        }
    }

    fn apply_property(&mut self, property: ThemeTextProperty) {
        match property {
            ThemeTextProperty::VerticalAlign(val) => {
                self.0.alignment.vertical = val;
            }
            ThemeTextProperty::HorizontalAlign(val) => {
                self.0.alignment.horizontal = val;
            }
            ThemeTextProperty::Font(val) => {
                for section in self.0.sections.iter_mut() {
                    section.style.font = val.clone();
                }
            }
            ThemeTextProperty::Size(val) => {
                for section in self.0.sections.iter_mut() {
                    section.style.font_size = val;
                }
            }
            ThemeTextProperty::Color(val) => {
                for section in self.0.sections.iter_mut() {
                    section.style.color = val;
                }
            }
        }
    }
}
