//! The first-person viewmodel's answer to movement: the weapon's sprint loop, and the jump and
//! landing overlays it plays on a layer above whatever else it is doing.
//!
//! Weapon states (firing, reloading, switching, aiming) are not modelled; the weapon is idle
//! unless its owner sprints.

use super::*;

#[derive(Clone, Debug, Default)]
pub struct Viewmodel {
    pub main: Option<Activity>,
    pub playback: Playback,
    pub overlay: Option<Activity>,
    pub overlay_playback: Playback,
    /// The weapon's own activity modifiers (its class, one-handedness); none for a stand-in.
    pub modifiers: Vec<String>,
    main_until: f32,
}

impl Viewmodel {
    fn mods(modifiers: &[String]) -> Vec<&str> {
        modifiers.iter().map(String::as_str).collect()
    }

    /// Jumps and landings play once, at rate 1, on the overlay layer.
    pub fn event(&mut self, ev: PlayerEvent, set: &dyn AnimSet) {
        let act = match ev {
            PlayerEvent::Jump | PlayerEvent::WallJump => Activity::VM_JUMP,
            PlayerEvent::DoubleJump => Activity::VM_DOUBLEJUMP,
            PlayerEvent::Land => Activity::VM_LAND,
            PlayerEvent::LandLow => Activity::VM_LAND_LOW,
            PlayerEvent::LandMedium => Activity::VM_LAND_MEDIUM,
            PlayerEvent::LandHigh => Activity::VM_LAND_HIGH,
            PlayerEvent::Dodge => return,
        };
        let Some(seq) = set.select(act, &Self::mods(&self.modifiers)) else { return };
        self.overlay = Some(act);
        self.overlay_playback.play(Some(seq));
        self.overlay_playback.restart();
        self.overlay_playback.rate = 1.0;
    }

    /// While the owner sprints, the sprint loop, if the weapon has one (one-handed weapons
    /// their own); otherwise idle, once the current activity has had its time.
    pub fn update(&mut self, sprinting: bool, one_handed: bool, now: f32, dt: f32, set: &dyn AnimSet) -> Vec<u16> {
        let mods = Self::mods(&self.modifiers);
        let sprint = set.select(Activity::VM_SPRINT, &mods);
        if sprinting && sprint.is_some() {
            let act = if one_handed { Activity::VM_ONEHANDED_SPRINT } else { Activity::VM_SPRINT };
            if self.main != Some(act) {
                self.main = Some(act);
                self.playback.play(set.select(act, &mods).or(sprint));
                self.main_until = now;
            }
        } else if self.main_until <= now && self.main != Some(Activity::VM_IDLE) {
            self.main = Some(Activity::VM_IDLE);
            self.playback.play(set.select(Activity::VM_IDLE, &mods));
        }
        self.playback.rate = 1.0;
        let mut events = self.playback.advance(set, dt);
        events.extend(self.overlay_playback.advance(set, dt));
        if self.overlay_playback.finished {
            self.overlay = None;
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct OneEach;
    impl AnimSet for OneEach {
        fn select(&self, a: Activity, _: &[&str]) -> Option<Sequence> {
            Some(Sequence(a.0 as u32))
        }
        fn activity(&self, s: Sequence) -> Option<Activity> {
            Some(Activity(s.0 as u16))
        }
        fn duration(&self, _: Sequence) -> f32 {
            0.5
        }
        fn ground_speed(&self, _: Sequence) -> f32 {
            0.0
        }
        fn looping(&self, s: Sequence) -> bool {
            s.0 as u16 != Activity::VM_LAND_LOW.0
        }
        fn pose_range(&self, _: PoseParam) -> Option<(f32, f32)> {
            None
        }
        fn events(&self, _: Sequence) -> &[(f32, u16)] {
            &[]
        }
    }

    #[test]
    fn sprint_loop_and_landing_overlay() {
        let mut vm = Viewmodel::default();
        vm.update(false, false, 1.0, 0.01, &OneEach);
        assert_eq!(vm.main, Some(Activity::VM_IDLE));
        vm.update(true, false, 1.01, 0.01, &OneEach);
        assert_eq!(vm.main, Some(Activity::VM_SPRINT));
        vm.event(PlayerEvent::LandLow, &OneEach);
        assert_eq!(vm.overlay, Some(Activity::VM_LAND_LOW));
        vm.update(false, false, 1.02, 0.6, &OneEach);
        assert_eq!((vm.main, vm.overlay), (Some(Activity::VM_IDLE), None));
        assert!(NoAnims.select(Activity::VM_SPRINT, &[]).is_none());
        vm.update(true, false, 1.03, 0.01, &NoAnims);
        assert_eq!(vm.main, Some(Activity::VM_IDLE), "no sprint sequence, no sprint");
    }
}
