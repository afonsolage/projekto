use bevy::prelude::*;
use bevy_ecss::StyleSheet;
use projekto_widgets::{
    console::{CommandIssued, Console, ConsoleAction},
    widget::{ToStringLabel, Widget},
};

pub struct ConsolePlugin;

impl Plugin for ConsolePlugin {
    fn build(&self, app: &mut App) {
        app.add_startup_system(setup_console_window)
            .add_system(toggle_console)
            .add_system(process_command);
    }
}

fn setup_console_window(mut commands: Commands, asset_server: Res<AssetServer>) {
    let console = Console::build("Console".label(), &mut commands);
    commands
        .entity(console)
        .insert(StyleSheet::new(asset_server.load("sheets/ui/console.css")));
}

fn toggle_console(input: Res<Input<KeyCode>>, mut writer: EventWriter<ConsoleAction>) {
    if input.pressed(KeyCode::LControl)
        && input.any_just_pressed(vec![KeyCode::Grave, KeyCode::Apostrophe])
    {
        writer.send(ConsoleAction::Toggle);
    }
}

fn process_command(mut cmds: EventReader<CommandIssued>, mut q_sheet: Query<&mut StyleSheet>) {
    for CommandIssued(entity, _cmd) in cmds.iter() {
        if let Ok(mut sheet) = q_sheet.get_mut(*entity) {
            sheet.refresh();
        }
    }
}
