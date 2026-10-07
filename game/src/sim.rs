use bevy::prelude::*;

pub struct SimPlugin;

impl Plugin for SimPlugin {
    fn build(&self, app: &mut App) {
        crate::common(app);
    }
}
