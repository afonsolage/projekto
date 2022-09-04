use bevy::{
    prelude::{App, CoreStage, Plugin, SystemLabel, SystemSet},
    ui::UiSystem,
};

use self::{style::apply_theme_style, text::apply_theme_text};

mod style;
mod text;

pub use style::{ApplyThemeStyle, ThemeStyle, ThemeStyleProperty};
pub use text::{ApplyThemeText, ThemeText, ThemeTextProperty};

#[derive(SystemLabel)]
pub struct ThemeSystem;

pub struct ThemePlugin;

impl Plugin for ThemePlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<ApplyThemeStyle>()
            .add_event::<ApplyThemeText>()
            .add_system_set_to_stage(
                CoreStage::PostUpdate,
                SystemSet::new()
                    .label(ThemeSystem)
                    .with_system(apply_theme_style)
                    .with_system(apply_theme_text)
                    .before(UiSystem::Flex),
            );
    }
}
