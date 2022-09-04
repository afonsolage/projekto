use bevy::{prelude::*, ui::UiSystem};

pub(super) struct FocusPlugin;

impl Plugin for FocusPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActiveFocus>()
            .register_type::<Focus>()
            .add_system_to_stage(
                CoreStage::PreUpdate,
                update_focus_on_interaction_changed.after(UiSystem::Focus),
            )
            .add_system_to_stage(CoreStage::PostUpdate, update_active_focus);
    }
}

#[derive(Default, Debug)]
pub struct ActiveFocus(Option<Entity>);

impl ActiveFocus {
    pub fn clear(&mut self) {
        self.0 = None;
    }

    pub fn set(&mut self, entity: Entity) {
        self.0 = Some(entity);
    }
}

#[derive(Component, Default, Reflect)]
#[reflect(Component)]
pub struct Focus(bool);

impl Focus {
    pub fn is_focused(&self) -> bool {
        self.0
    }
}

#[derive(Default, Debug)]
struct LastFocus(Option<Entity>);

fn update_active_focus(
    mut q_focus: Query<&mut Focus>,
    active_focus: Res<ActiveFocus>,
    mut last_focus: Local<LastFocus>,
) {
    if active_focus.is_changed() {
        if last_focus.0.is_some() && active_focus.0.is_none() {
            // Clear focus;
            if let Ok(mut focus) = q_focus.get_mut(last_focus.0.unwrap()) {
                focus.0 = false;
            }
        } else if last_focus.0.is_some() && active_focus.0.is_some() {
            // Swap focus;
            if let Ok(mut focus) = q_focus.get_mut(last_focus.0.unwrap()) {
                focus.0 = false;
            }
            if let Ok(mut focus) = q_focus.get_mut(active_focus.0.unwrap()) {
                focus.0 = true;
            }
        } else if last_focus.0.is_none() && active_focus.0.is_some() {
            // Set focus;
            if let Ok(mut focus) = q_focus.get_mut(active_focus.0.unwrap()) {
                focus.0 = true;
            }
        }

        last_focus.0 = active_focus.0;
    }
}

fn update_focus_on_interaction_changed(
    q: Query<(Entity, &Interaction), (With<Focus>, Changed<Interaction>)>,
    mut active_focus: ResMut<ActiveFocus>,
) {
    for (e, interaction) in &q {
        match interaction {
            Interaction::Clicked => active_focus.set(e),
            _ => continue,
        }
    }
}
