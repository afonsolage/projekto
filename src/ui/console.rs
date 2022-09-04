use bevy::prelude::*;
use projekto_widgets::{
    console::{Console, ConsoleAction},
    widget::{ToStringLabel, Widget},
};

pub struct ConsolePlugin;

impl Plugin for ConsolePlugin {
    fn build(&self, app: &mut App) {
        app.add_startup_system(setup_console_window)
            .add_system(toggle_console);
    }
}

fn setup_console_window(mut commands: Commands) {
    Console::build("Console".label(), &mut commands);
}

fn toggle_console(input: Res<Input<KeyCode>>, mut writer: EventWriter<ConsoleAction>) {
    if input.pressed(KeyCode::LControl)
        && input.any_just_pressed(vec![KeyCode::Grave, KeyCode::Apostrophe])
    {
        writer.send(ConsoleAction::Toggle);
    }
}
