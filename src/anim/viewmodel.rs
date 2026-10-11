//! The first-person viewmodel: what the weapon sends it as its owner moves, and how the client
//! plays what it is sent.
//!
//! Per command, the weapon's code: Deploy (`0xa43b60`), WeaponIdle (`0xa439b0`), RaiseWeapon
//! from StopSprint (`0xa44520`), SendWeaponAnim (`0xa58440`, `0xa35d40`) with the weapon's
//! sequence lengths (`0xa4c820`), the viewmodel's SendViewModelMatchingSequence (`0x7e3530`),
//! and the player's events reaching the viewmodel (`0x7e84e0`). Per frame, the client's: the
//! sequence transitions (`super::transition`), the overlay layers' weights (AccumulateLayers,
//! `0x7bfe80`) and the client events crossed (`0x601a50`, `0x5f1750`).
//!
//! A mantle puts the weapon away (`holster`) and raises it after (`unholster`).
//!
//! Not modelled: firing, reloading, switching, melee and inspecting; the weapon's
//! one-handed and alternate activities (`0xa582a0`); the player's own activity modifiers
//! (crouch, air, sliding: no movement sequence of the fists names one); the draw among equally
//! good sequences; the lockout a melee leaves on the weapon's sequence lengths; the client's
//! frame advance (cycles run from when a sequence was sent, as the transitions' do).

use super::bones::{clamp_cycle, STUDIO_DELTA, STUDIO_LOOPING};
use super::model::{event_number, ModelAnims};
use super::transition::Transitioner;
use super::*;

/// ConVar `weapon_sprint_raise_delay` (image value).
pub const WEAPON_SPRINT_RAISE_DELAY: bool = true;
/// The most a sequence is sped up to fit the time the weapon script gives it.
const MAX_RATE: f32 = 12.0;
/// Where a new main sequence's events are read from, so those at 0 fire.
pub(super) const NEW_MAIN_CYCLE: f32 = -0.01;
const NEW_LAYER_CYCLE: f32 = -1.0;

/// Seconds the weapon script gives its moves (0, the registered default, leaves a sequence its
/// own length): `deploy_time`, `deployfirst_time`, `raise_time`, `raise_from_sprint_time`,
/// `sprintcycle_time`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeaponTimes {
    pub deploy: f32,
    pub deploy_first: f32,
    pub raise: f32,
    pub raise_from_sprint: f32,
    pub sprint_cycle: f32,
}

impl WeaponTimes {
    pub fn from_script(get: impl Fn(&str) -> Option<f32>) -> WeaponTimes {
        let t = |k| get(k).unwrap_or(0.0);
        WeaponTimes {
            deploy: t("deploy_time"),
            deploy_first: t("deployfirst_time"),
            raise: t("raise_time"),
            raise_from_sprint: t("raise_from_sprint_time"),
            sprint_cycle: t("sprintcycle_time"),
        }
    }
}

/// The weapon's state (`+0x1344`), as far as movement reaches it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WeaponState {
    #[default]
    Idle,
    Deploying,
    Raising,
    Sprinting,
}

/// A sequence as sent: from cycle `from` at `start`, at `rate`, with the layer's weight and
/// fades.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Layer {
    pub seq: Option<Sequence>,
    pub start: f32,
    pub from: f32,
    pub rate: f32,
    pub weight: f32,
    pub fade_in: f32,
    pub fade_out: f32,
    pub(super) parity: i32,
    fired: f32,
    seen: i32,
}

impl Layer {
    pub(super) fn restart(&mut self, seq: Sequence, now: f32, rate: f32) {
        *self = Layer { seq: Some(seq), start: now, from: 0.0, rate, weight: 1.0, parity: (self.parity + 1) & 0xf, ..*self };
    }

    /// Where the sequence is at `now`, held short of its end unless it loops.
    pub fn cycle(&self, set: &ModelAnims, now: f32) -> f32 {
        let Some(info) = self.seq.and_then(|s| set.seqs.get(s.0 as usize)) else { return 0.0 };
        clamp_cycle(self.from + (now - self.start) * info.rate * self.rate, info.flags & STUDIO_LOOPING != 0)
    }

    /// The client events crossed since the last call, as (event, options).
    pub(super) fn events(&mut self, set: &ModelAnims, cycle: f32, fresh: f32) -> Vec<(u16, String)> {
        let Some(info) = self.seq.and_then(|s| set.seqs.get(s.0 as usize)) else { return Vec::new() };
        if self.seen != self.parity {
            self.seen = self.parity;
            self.fired = fresh;
        }
        let (from, to) = (self.fired, cycle);
        let ranges: &[(f32, f32)] = if from < to {
            &[(from, to)]
        } else if 0.5 < from - to {
            &[(from, 1.0), (NEW_MAIN_CYCLE, to)]
        } else {
            &[]
        };
        self.fired = to;
        let mut out = Vec::new();
        for &(a, b) in ranges {
            for (c, name, options) in &info.named_events {
                if let Some(id) = event_number(name).filter(|_| a < *c && *c <= b) {
                    out.push((id, options.clone()));
                }
            }
        }
        out
    }
}

/// What to blend this frame, in order: the main sequence, the sequences it left, the layers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Blends {
    pub main: Option<(usize, f32)>,
    pub transitions: Vec<(usize, f32, f32)>,
    pub layers: Vec<(usize, f32, f32)>,
}

#[derive(Clone, Debug, Default)]
pub struct Viewmodel {
    /// The weapon's activity modifiers: the legend's class, for the fists.
    pub modifiers: Vec<String>,
    pub times: WeaponTimes,
    /// Whether the weapon goes to idle once its activity has run; the fists hold their pose.
    pub idles: bool,
    pub state: WeaponState,
    pub activity: Option<Activity>,
    ideal: Option<Sequence>,
    deployed: bool,
    next_idle: f32,
    /// While put away (Holster, `0xa44340`): when the lowering ends and the viewmodel goes.
    holstered: Option<f32>,
    pub main: Layer,
    /// Layer 0 takes the weapon's delta sequences, layer 1 the player's jumps and landings.
    pub layers: [Layer; 2],
    pub transitions: Transitioner,
}

impl Viewmodel {
    /// A weapon that has not been drawn yet.
    pub fn new(modifiers: Vec<String>, times: WeaponTimes) -> Viewmodel {
        Viewmodel { modifiers, times, ..Viewmodel::default() }
    }

    fn mods(&self) -> Vec<&str> {
        self.modifiers.iter().map(String::as_str).collect()
    }

    /// The weapon's length for a sequence (`0xa4c820`): the script's time for its activity, or
    /// the sequence's own.
    fn sequence_duration(&self, set: &ModelAnims, seq: Sequence) -> f32 {
        let Some(info) = set.seqs.get(seq.0 as usize) else { return 0.0 };
        let t = &self.times;
        let given = match info.activity {
            Some(Activity::VM_DRAW) => t.deploy,
            Some(Activity::VM_DRAWFIRST) => t.deploy_first,
            Some(Activity::VM_RAISE) => t.raise,
            Some(Activity::VM_RAISE_FROM_SPRINT) => t.raise_from_sprint,
            Some(Activity::VM_SPRINT | Activity::VM_ONEHANDED_SPRINT) => t.sprint_cycle,
            _ => 0.0,
        };
        let d = if 0.0 < given { given } else { info.duration };
        if d <= 0.0 { 0.0 } else { d }
    }

    /// SendWeaponAnim (`0xa35d40`): false when the weapon has no sequence for the activity.
    /// The sequence is sped up to fit the script's time, and the weapon goes idle after it.
    fn send_weapon_anim(&mut self, set: &ModelAnims, act: Activity, now: f32) -> bool {
        let Some(seq) = set.select(act, &self.mods(), self.main.seq) else { return false };
        let idling = self.activity.is_some_and(|a| (Activity::VM_IDLE.0..Activity::VM_IDLE.0 + 4).contains(&a.0));
        if Some(seq) == self.ideal && idling {
            return true;
        }
        self.activity = Some(act);
        self.ideal = Some(seq);
        let t = self.sequence_duration(set, seq);
        let r = if 0.0 < t { set.seqs[seq.0 as usize].duration / t } else { 1.0 };
        self.send_viewmodel(set, seq, if r <= MAX_RATE { r } else { MAX_RATE }, now);
        self.next_idle = now + t;
        true
    }

    /// SendViewModelMatchingSequence (`0x7e3530`): a delta sequence plays on layer 0 at full
    /// weight; any other replaces the main sequence and takes layer 0's weight away.
    fn send_viewmodel(&mut self, set: &ModelAnims, seq: Sequence, rate: f32, now: f32) {
        if set.seqs[seq.0 as usize].flags & STUDIO_DELTA == 0 {
            self.layers[0].weight = 0.0;
            self.main.restart(seq, now, rate);
        } else {
            self.layers[0].restart(seq, now, rate);
        }
    }

    /// Deploy (`0xa43b60`): drawn into the sprint if the owner sprints, else the first draw.
    pub fn deploy(&mut self, set: &ModelAnims, sprinting: bool, now: f32) {
        if self.state == WeaponState::Deploying {
            return;
        }
        self.next_idle = 0.0;
        self.weapon_idle(set, sprinting, now);
        let sent = if sprinting {
            self.send_weapon_anim(set, Activity::VM_DRAW_TO_SPRINT, now)
        } else {
            (!self.deployed && self.send_weapon_anim(set, Activity::VM_DRAWFIRST, now)) || self.send_weapon_anim(set, Activity::VM_DRAW, now)
        };
        if sent || !sprinting {
            self.deployed = true;
            self.state = WeaponState::Deploying;
        }
    }

    /// WeaponIdle (`0xa439b0`), each command: the sprint loop while the owner sprints, if the
    /// weapon has one; otherwise, once the current activity has run, idle for a weapon that
    /// idles.
    pub fn weapon_idle(&mut self, set: &ModelAnims, sprinting: bool, now: f32) {
        if sprinting && set.select(Activity::VM_SPRINT, &[], None).is_some() {
            self.state = WeaponState::Sprinting;
            if self.activity != Some(Activity::VM_SPRINT) {
                self.send_weapon_anim(set, Activity::VM_SPRINT, now);
                self.next_idle = now;
            }
            return;
        }
        if self.next_idle <= now {
            self.state = WeaponState::Idle;
            if self.idles || self.activity == Some(Activity::VM_WEAPON_INSPECT) {
                self.send_weapon_anim(set, Activity::VM_IDLE, now);
            }
        }
    }

    /// RaiseWeapon from StopSprint (`0x8653c0`, `0xa44520`), for a weapon that was sprinting:
    /// the raise from the sprint, else the plain raise, else idle at once.
    pub fn sprint_end(&mut self, set: &ModelAnims, now: f32) {
        if self.state != WeaponState::Sprinting || !self.deployed {
            return;
        }
        self.state = WeaponState::Raising;
        let raised = WEAPON_SPRINT_RAISE_DELAY && self.send_weapon_anim(set, Activity::VM_RAISE_FROM_SPRINT, now)
            || WEAPON_SPRINT_RAISE_DELAY && self.send_weapon_anim(set, Activity::VM_RAISE, now);
        if !raised {
            self.send_weapon_anim(set, Activity::VM_IDLE, now);
            self.next_idle = now;
        }
    }

    /// The player's events on the viewmodel (`0x7e84e0`): layer 1 plays the sequence once at
    /// rate 1, fading in and out over the sequence's own fade times.
    pub fn event(&mut self, ev: PlayerEvent, set: &ModelAnims, now: f32) {
        let act = match ev {
            PlayerEvent::Jump | PlayerEvent::WallJump => Activity::VM_JUMP,
            PlayerEvent::DoubleJump => Activity::VM_DOUBLEJUMP,
            PlayerEvent::Land => Activity::VM_LAND,
            PlayerEvent::LandLow => Activity::VM_LAND_LOW,
            PlayerEvent::LandMedium => Activity::VM_LAND_MEDIUM,
            PlayerEvent::LandHigh => Activity::VM_LAND_HIGH,
            PlayerEvent::Dodge => return,
        };
        let Some(seq) = set.select(act, &self.mods(), self.main.seq) else { return };
        let info = &set.seqs[seq.0 as usize];
        let layer = &mut self.layers[1];
        layer.restart(seq, now, 1.0);
        (layer.fade_in, layer.fade_out) = (info.fade_in, info.fade_out);
    }

    /// Holster (`0xa44340`) as a mantle takes the weapon: `ACT_VM_LOWER` in half its time,
    /// and the viewmodel is not drawn once it has played.
    pub fn holster(&mut self, set: &ModelAnims, now: f32) {
        if self.holstered.is_some() {
            return;
        }
        let Some(seq) = set.select(Activity::VM_LOWER, &self.mods(), self.main.seq) else {
            self.holstered = Some(now);
            return;
        };
        let t = self.sequence_duration(set, seq) * 0.5;
        self.activity = Some(Activity::VM_LOWER);
        self.ideal = Some(seq);
        let r = if 0.0 < t { set.seqs[seq.0 as usize].duration / t } else { 1.0 };
        self.send_viewmodel(set, seq, if r <= MAX_RATE { r } else { MAX_RATE }, now);
        self.holstered = Some(now + t);
    }

    /// Back out after the mantle (RaiseWeapon, `0xa44520`): `ACT_VM_RAISE`.
    pub fn unholster(&mut self, set: &ModelAnims, now: f32) {
        if self.holstered.take().is_some() {
            self.state = WeaponState::Raising;
            if !self.send_weapon_anim(set, Activity::VM_RAISE, now) {
                self.send_weapon_anim(set, Activity::VM_IDLE, now);
                self.next_idle = now;
            }
        }
    }

    /// Whether the viewmodel is put away and not drawn.
    pub fn hidden(&self, now: f32) -> bool {
        self.holstered.is_some_and(|t| t <= now)
    }

    /// One command's cues and the sprint state it left.
    pub fn command(&mut self, set: &ModelAnims, cues: &[Cue], sprinting: bool, now: f32) {
        if self.holstered.is_some() {
            return;
        }
        for cue in cues {
            if matches!(cue, Cue::SprintEnd) {
                self.sprint_end(set, now);
            } else if let Some(ev) = PlayerEvent::of(cue) {
                self.event(ev, set, now);
            }
        }
        self.weapon_idle(set, sprinting, now);
    }

    /// A layer's weight with its fades (AccumulateLayers, `0x7bfe80`): in over the first
    /// `fade_in` seconds' share of its length, out over the last `fade_out`'s.
    fn layer_weight(set: &ModelAnims, l: &Layer, cycle: f32) -> f32 {
        let mut w = if l.weight <= 1.0 { l.weight } else { 1.0 };
        let Some(info) = l.seq.and_then(|s| set.seqs.get(s.0 as usize)) else { return w };
        if info.flags & STUDIO_LOOPING != 0 || !(0.0 < l.fade_in || 0.0 < l.fade_out) {
            return w;
        }
        let len = if l.rate == 0.0 { info.duration } else { info.duration / l.rate };
        if !(0.0 < len) {
            return w;
        }
        let unit = |x: f32| if x <= 0.0 { 0.0 } else if 1.0 <= x { 1.0 } else { x };
        if 0.0 < l.fade_in {
            w *= unit(cycle / unit(l.fade_in / len));
        }
        if 0.0 < l.fade_out {
            w *= unit((1.0 - cycle) / unit(l.fade_out / len));
        }
        w
    }

    /// The client's frame: transitions recorded, events crossed, and what to blend.
    pub fn frame(&mut self, set: &ModelAnims, now: f32) -> (Blends, Vec<(u16, String)>) {
        let mut blends = Blends::default();
        let mut events = Vec::new();
        if let Some(seq) = self.main.seq {
            let cycle = self.main.cycle(set, now);
            self.transitions.check_for_sequence_change(&set.seqs, seq.0 as i32, self.main.parity);
            self.transitions.update_current(now, seq.0 as i32, cycle, self.main.rate, self.main.parity);
            events.extend(self.main.events(set, cycle, NEW_MAIN_CYCLE));
            blends.main = Some((seq.0 as usize, cycle));
            blends.transitions = self.transitions.maintain(&set.seqs, now);
        }
        for l in &mut self.layers {
            let Some(seq) = l.seq.filter(|_| l.weight != 0.0) else { continue };
            let cycle = l.cycle(set, now);
            events.extend(l.events(set, cycle, NEW_LAYER_CYCLE));
            let w = Self::layer_weight(set, l, cycle);
            if 0.0 < w {
                blends.layers.push((seq.0 as usize, cycle, w));
            }
        }
        (blends, events)
    }
}

#[cfg(test)]
mod tests {
    use super::super::model::SeqInfo;
    use super::*;

    fn seq(act: Activity, flags: u32, duration: f32, mods: &[(&str, bool)]) -> SeqInfo {
        SeqInfo {
            activity: Some(act),
            weight: 1,
            flags,
            modifiers: mods.iter().map(|(m, n)| (m.to_string(), *n)).collect(),
            fade_in: 0.2,
            fade_out: 0.2,
            rate: 1.0 / duration,
            duration,
            ..SeqInfo::default()
        }
    }

    fn fists() -> ModelAnims {
        let mut land = seq(Activity::VM_LAND_MEDIUM, STUDIO_DELTA, 0.667, &[]);
        land.named_events = vec![(0.0, "AE_CL_PLAYSOUND_FOR_TYPE".into(), "effort_land_1p".into())];
        (land.fade_in, land.fade_out) = (0.05, 0.35);
        ModelAnims::from_seqs(vec![
            SeqInfo::default(),
            seq(Activity::VM_DRAWFIRST, 0, 0.333, &[]),
            seq(Activity::VM_IDLE, 0, 0.333, &[]),
            seq(Activity::VM_SPRINT, STUDIO_LOOPING, 0.556, &[("pilot_survival_gunner", true)]),
            seq(Activity::VM_SPRINT, STUDIO_LOOPING, 0.714, &[]),
            seq(Activity::VM_RAISE_FROM_SPRINT, 0, 0.333, &[]),
            land,
        ])
    }

    #[test]
    fn the_fists_draw_sprint_raise_and_hold() {
        let set = fists();
        let mut vm = Viewmodel { times: WeaponTimes { deploy_first: 1.25, ..WeaponTimes::default() }, ..Viewmodel::default() };
        vm.modifiers.push("pilot_survival_gunner".into());
        vm.deploy(&set, false, 1.0);
        assert_eq!((vm.activity, vm.main.seq, vm.state), (Some(Activity::VM_DRAWFIRST), Some(Sequence(1)), WeaponState::Deploying));
        assert!((vm.main.rate - 0.333 / 1.25).abs() < 1e-6, "slowed to the script's deployfirst_time");
        vm.command(&set, &[], false, 2.0);
        assert_eq!((vm.state, vm.activity), (WeaponState::Deploying, Some(Activity::VM_DRAWFIRST)), "still drawing");
        vm.command(&set, &[], true, 2.3);
        assert_eq!((vm.state, vm.main.seq), (WeaponState::Sprinting, Some(Sequence(4))), "the gunner's sprint rules out the first");
        vm.command(&set, &[Cue::SprintEnd], false, 3.0);
        assert_eq!(vm.activity, Some(Activity::VM_RAISE_FROM_SPRINT));
        vm.command(&set, &[], false, 3.5);
        assert_eq!((vm.state, vm.activity), (WeaponState::Idle, Some(Activity::VM_RAISE_FROM_SPRINT)), "the fists never idle");
        let (b, _) = vm.frame(&set, 3.5);
        assert_eq!(b.main.map(|(s, c)| (s, c > 0.99)), Some((5, true)), "holding the raise's last frame");
    }

    #[test]
    fn a_landing_plays_on_layer_one_with_its_sound() {
        let set = fists();
        let mut vm = Viewmodel::default();
        vm.deploy(&set, false, 0.0);
        vm.frame(&set, 0.5);
        vm.event(PlayerEvent::LandMedium, &set, 1.0);
        let (b, ev) = vm.frame(&set, 1.0);
        assert_eq!(ev, [(AE_CL_PLAYSOUND_FOR_TYPE, "effort_land_1p".to_string())]);
        assert!(b.layers.is_empty(), "weight 0 at the start of the fade-in");
        let (b, ev) = vm.frame(&set, 1.2);
        assert!(ev.is_empty() && b.layers[0].0 == 6 && b.layers[0].2 > 0.0);
        let (b, _) = vm.frame(&set, 1.7);
        assert!(b.layers.is_empty() || b.layers[0].2 < 0.01, "faded out by its end");
    }
}
