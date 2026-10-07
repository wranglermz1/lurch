use bevy::prelude::*;

pub struct CmdPlugin;

impl Plugin for CmdPlugin {
    fn build(&self, app: &mut App) {
        crate::common(app);
    }
}
