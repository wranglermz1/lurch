use bevy::prelude::*;

pub struct LimiterPlugin;

impl Plugin for LimiterPlugin {
    fn build(&self, app: &mut App) {
        crate::common(app);
    }
}
