//! The first-person viewmodel: what the weapon sends it as its owner moves, and how the client
//! plays what it is sent.
//!
//! Per command, the weapon's code: Deploy (`0xa43b60`), WeaponIdle (`0xa439b0`), RaiseWeapon
//! from StopSprint (`0xa44520`), SendWeaponAnim (`0xa58440`, `0xa35d40`) with the weapon's
//! sequence lengths (`0xa4c820`) and the node graph's transitions (`0x5f9d40`), the
//! viewmodel's SendViewModelMatchingSequence (`0x7e3530`), the player's activity modifiers
//! (`0x854bc0`, `0xa396c0`) and the player's events reaching the viewmodel (`0x7e84e0`). Per frame, the client's: the
//! sequence transitions (`super::transition`) of the main sequence and of overlay layers 0 and
//! 1, the overlay layers' weights (AccumulateLayers,
//! `0x7bfe80`) and the client events crossed (`0x601a50`, `0x5f1750`).
//!
//! A mantle puts the weapon away (`holster`) and raises it after (`unholster`).
//!
//! Not modelled: firing, reloading, switching, melee and inspecting; the weapon's
//! one-handed and alternate activities (`0xa582a0`); the modifiers `airfast`, `althand` and
//! `onehand_was_sprinting`; the draw among equally good sequences; the lockout a melee leaves on
//! the weapon's sequence lengths; the client's frame advance (cycles run from when a sequence
//! was sent, as the transitions' do, and a transition ends when its cycle reaches 1 at a
//! command).

use super::bones::{clamp_cycle, STUDIO_DELTA, STUDIO_LOOPING};
use super::model::{event_number, ModelAnims};
use super::transition::Transitioner;
use super::*;
use crate::sim::Player;

/// ConVar `weapon_sprint_raise_delay` (image value).
pub const WEAPON_SPRINT_RAISE_DELAY: bool = true;
/// ConVar `weapon_doIdleForSurvivalMelee` (image value).
pub const WEAPON_DO_IDLE_FOR_SURVIVAL_MELEE: bool = false;

/// Whether a weapon goes to idle once its activity has run (`+0x16ac`, set at `0xa3c702`): every
/// weapon but the fists, whose class is `mp_weapon_melee_survival`.
pub fn idles(class: &str) -> bool {
    WEAPON_DO_IDLE_FOR_SURVIVAL_MELEE || class != crate::assets::FISTS
}

/// The player's activity modifiers, bit n for number n of `MODIFIERS`, as `0x854bc0` keeps them
/// each command (a byte each from `+0x32b0`).
pub type Modifiers = u64;

/// `0x854bc0`'s modifiers from the player after movement: `ads` while zoomed (`+0x1881`),
/// `crouch` while the duck is going down or down, `wallrun` while the player's up is tilted,
/// `wallhang`, `sprinting`, `air` off the ground, `sliding`, `grapple` while it is active, and
/// `airsprint` in the air when the sprint ended within 0.05 s of leaving the ground.
pub fn player_modifiers(p: &Player) -> Modifiers {
    let air = p.ground.is_none();
    [
        (0, p.sprint_toggle_lock),
        (MOD_CROUCH, p.traversal_state.wrapping_sub(1) < 2),
        (20, p.prev_up[2] <= 1.0 && 1.0 != p.prev_up[2]),
        (21, p.hanging),
        (MOD_SPRINTING, p.sprinting),
        (MOD_AIR, air),
        (MOD_SLIDING, p.sliding),
        (30, p.grappling),
        (32, air && !(0.05 < (p.sprint_end_time - p.left_ground_time).abs())),
    ]
    .into_iter()
    .fold(0, |m, (n, on)| m | (on as u64) << n)
}
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

/// What to blend this frame, in order: the main sequence, the sequences it left, then each layer
/// followed by the sequences it left.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Blends {
    pub main: Option<(usize, f32)>,
    pub transitions: Vec<(usize, f32, f32)>,
    pub layers: Vec<(usize, f32, f32)>,
}

#[derive(Clone, Debug, Default)]
pub struct Viewmodel {
    /// The weapon's own activity modifiers: the legend's class.
    pub modifiers: Vec<String>,
    /// The player's, as the last command left them.
    pub player: Modifiers,
    pub times: WeaponTimes,
    /// Whether the weapon goes to idle once its activity has run (`idles`); the fists hold
    /// their pose.
    pub idles: bool,
    pub state: WeaponState,
    /// The activity playing (`+0x132c`): `TRANSITION` while a node transition leads to the ideal.
    pub activity: Option<Activity>,
    /// The activity and sequence asked for (`+0x1328`, `+0x1324`).
    ideal_activity: Option<Activity>,
    ideal: Option<Sequence>,
    deployed: bool,
    next_idle: f32,
    /// While put away (Holster, `0xa44340`): when the lowering ends and the viewmodel goes.
    holstered: Option<f32>,
    pub main: Layer,
    /// Layer 0 takes the weapon's delta sequences, layer 1 the player's jumps and landings.
    pub layers: [Layer; 2],
    pub transitions: Transitioner,
    /// Layers 0 and 1 keep their own transitions: the viewmodel's class answers true at vtable
    /// `+0x140`, so `0x5fc3f0` runs them and AccumulateLayers blends them (`0x7c0744`).
    pub layer_transitions: [Transitioner; 2],
}

impl Viewmodel {
    /// A weapon that has not been drawn yet.
    pub fn new(modifiers: Vec<String>, times: WeaponTimes) -> Viewmodel {
        Viewmodel { modifiers, times, ..Viewmodel::default() }
    }

    /// The modifiers a sequence is chosen with (`0xa396c0`): the player's, then the weapon's.
    fn mods(&self) -> Vec<&str> {
        let player = MODIFIERS.iter().enumerate().filter(|(n, _)| self.player >> n & 1 != 0).map(|(_, m)| *m);
        player.chain(self.modifiers.iter().map(String::as_str)).collect()
    }

    /// `0xa393d0`: whether the transition the weapon plays has run out (the viewmodel's
    /// sequence finished); true when no activity plays.
    fn transition_done(&self, set: &ModelAnims, now: f32) -> bool {
        if self.activity.is_none_or(|a| a.0 == 0) {
            return true;
        }
        let Some(info) = self.main.seq.and_then(|s| set.seqs.get(s.0 as usize)) else { return true };
        info.flags & STUDIO_LOOPING == 0 && 1.0 <= self.main.from + (now - self.main.start) * info.rate * self.main.rate
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
    /// The same idle again is left playing, and while a transition plays toward the ideal
    /// sequence, a new one it also leads to only becomes the ideal. Otherwise, unless it is a
    /// draw or the viewmodel is not drawn, the node graph's transition from the sequence
    /// playing plays first, as `TRANSITION`. What plays is sped up to fit the script's time,
    /// and the weapon goes idle after it.
    fn send_weapon_anim(&mut self, set: &ModelAnims, act: Activity, now: f32) -> bool {
        let Some(seq) = set.select(act, &self.mods(), self.main.seq) else { return false };
        let idle = self.ideal_activity.is_some_and(|a| (Activity::VM_IDLE.0..Activity::VM_IDLE.0 + 4).contains(&a.0));
        if Some(seq) == self.ideal && idle && self.activity != Some(Activity::TRANSITION) {
            return true;
        }
        let current = self.main.seq;
        if self.activity == Some(Activity::TRANSITION) && !self.transition_done(set, now) {
            if Some(seq) == self.ideal {
                return true;
            }
            if current.map(|c| set.seqs[c.0 as usize].exit_node) == Some(set.seqs[seq.0 as usize].entry_node) {
                (self.ideal_activity, self.ideal) = (Some(act), Some(seq));
                return true;
            }
        }
        (self.ideal_activity, self.ideal) = (Some(act), Some(seq));
        let via = match current {
            Some(c) if c != seq => set.transition(c, seq).unwrap_or(seq),
            _ => seq,
        };
        let draw = (Activity::VM_DRAW.0..=Activity::VM_DRAWCATCH.0).contains(&act.0)
            || (Activity::VM_ONEHANDED_DRAW.0..=Activity::VM_ONEHANDED_DRAWFIRST.0).contains(&act.0);
        let play = if !draw && !self.hidden(now) && via != seq {
            self.activity = Some(Activity::TRANSITION);
            via
        } else {
            self.activity = Some(act);
            seq
        };
        let t = self.sequence_duration(set, play);
        let r = if 0.0 < t { set.seqs[play.0 as usize].duration / t } else { 1.0 };
        self.send_viewmodel(set, play, if r <= MAX_RATE { r } else { MAX_RATE }, now);
        let charging = self.activity.is_some_and(|a| (Activity::VM_CHARGE.0..=Activity::VM_CHARGE_VER4.0).contains(&a.0));
        self.next_idle = now + if charging { 0.0 } else { t };
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

    /// `0x854bc0` with the player's modifiers after a command: when any changed, an idle
    /// weapon idles again at once, so the idle that suits them is chosen.
    pub fn set_player_modifiers(&mut self, m: Modifiers) {
        if m != self.player {
            self.player = m;
            if self.state == WeaponState::Idle {
                self.next_idle = 0.0;
            }
        }
    }

    /// One command's cues and the state it left: the sprint and the player's modifiers.
    pub fn command(&mut self, set: &ModelAnims, cues: &[Cue], sprinting: bool, modifiers: Modifiers, now: f32) {
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
        self.set_player_modifiers(modifiers);
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
        for (l, t) in self.layers.iter_mut().zip(&mut self.layer_transitions) {
            let Some(seq) = l.seq else { continue };
            let cycle = l.cycle(set, now);
            t.check_for_sequence_change(&set.seqs, seq.0 as i32, l.parity);
            t.update_current(now, seq.0 as i32, cycle, l.rate, l.parity);
            if l.weight == 0.0 {
                continue;
            }
            events.extend(l.events(set, cycle, NEW_LAYER_CYCLE));
            let w = Self::layer_weight(set, l, cycle);
            if 0.0 < w {
                blends.layers.push((seq.0 as usize, cycle, w));
            }
            blends.layers.extend(t.maintain(&set.seqs, now));
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
        let mut jump = seq(Activity::VM_JUMP, STUDIO_DELTA, 1.0, &[]);
        jump.fade_out = 0.35;
        ModelAnims::from_seqs(vec![
            SeqInfo::default(),
            seq(Activity::VM_DRAWFIRST, 0, 0.333, &[]),
            seq(Activity::VM_IDLE, 0, 0.333, &[]),
            seq(Activity::VM_SPRINT, STUDIO_LOOPING, 0.556, &[("pilot_survival_gunner", true)]),
            seq(Activity::VM_SPRINT, STUDIO_LOOPING, 0.714, &[]),
            seq(Activity::VM_RAISE_FROM_SPRINT, 0, 0.333, &[]),
            land,
            jump,
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
        vm.command(&set, &[], false, Modifiers::default(), 2.0);
        assert_eq!((vm.state, vm.activity), (WeaponState::Deploying, Some(Activity::VM_DRAWFIRST)), "still drawing");
        vm.command(&set, &[], true, Modifiers::default(), 2.3);
        assert_eq!((vm.state, vm.main.seq), (WeaponState::Sprinting, Some(Sequence(4))), "the gunner's sprint rules out the first");
        vm.command(&set, &[Cue::SprintEnd], false, Modifiers::default(), 3.0);
        assert_eq!(vm.activity, Some(Activity::VM_RAISE_FROM_SPRINT));
        vm.command(&set, &[], false, Modifiers::default(), 3.5);
        assert_eq!((vm.state, vm.activity), (WeaponState::Idle, Some(Activity::VM_RAISE_FROM_SPRINT)), "the fists never idle");
        let (b, _) = vm.frame(&set, 3.5);
        assert_eq!(b.main.map(|(s, c)| (s, c > 0.99)), Some((5, true)), "holding the raise's last frame");
    }

    /// The kunai's graph: idle at node 1, its crouched idle at 2, the sprint at 3, and the
    /// transitions between them, which have no activity.
    fn kunai() -> ModelAnims {
        let node = |mut s: SeqInfo, entry, exit| {
            (s.entry_node, s.exit_node) = (entry, exit);
            s
        };
        let trans = |duration| SeqInfo { activity: None, ..seq(Activity::VM_IDLE, 0, duration, &[]) };
        ModelAnims::from_seqs(vec![
            SeqInfo::default(),
            node(seq(Activity::VM_DRAWFIRST, 0, 0.5, &[]), 0, 1),
            node(seq(Activity::VM_IDLE, STUDIO_LOOPING, 1.567, &[]), 1, 1),
            node(seq(Activity::VM_IDLE, STUDIO_LOOPING, 1.567, &[("crouch", false)]), 2, 2),
            node(seq(Activity::VM_SPRINT, STUDIO_LOOPING, 0.476, &[]), 3, 3),
            node(trans(0.525), 1, 3),
            node(trans(0.933), 1, 2),
            node(seq(Activity::VM_RAISE_FROM_SPRINT, 0, 0.556, &[]), 3, 1),
        ])
    }

    #[test]
    fn a_weapon_that_idles_regrips_through_its_transitions() {
        let set = kunai();
        let mut vm = Viewmodel { idles: idles("mp_weapon_wraith_kunai_primary"), ..Viewmodel::default() };
        assert!(vm.idles && !idles(crate::assets::FISTS));
        let none = Modifiers::default();
        vm.deploy(&set, false, 0.0);
        vm.command(&set, &[], false, none, 0.6);
        assert_eq!((vm.activity, vm.main.seq), (Some(Activity::VM_IDLE), Some(Sequence(2))), "idles after the draw");
        vm.command(&set, &[], true, none, 1.0);
        assert_eq!((vm.activity, vm.main.seq), (Some(Activity::TRANSITION), Some(Sequence(5))), "idle to sprint first");
        vm.command(&set, &[], true, none, 1.3);
        assert_eq!(vm.main.seq, Some(Sequence(5)), "the transition keeps playing");
        vm.command(&set, &[], true, none, 1.6);
        assert_eq!((vm.activity, vm.main.seq), (Some(Activity::VM_SPRINT), Some(Sequence(4))), "then the sprint");
        vm.command(&set, &[Cue::SprintEnd], false, none, 2.0);
        assert_eq!((vm.activity, vm.main.seq), (Some(Activity::VM_RAISE_FROM_SPRINT), Some(Sequence(7))), "the raise leads to node 1 itself");
        vm.command(&set, &[], false, none, 2.6);
        assert_eq!(vm.main.seq, Some(Sequence(2)));
        vm.command(&set, &[], false, 1 << MOD_CROUCH, 3.0);
        assert_eq!((vm.activity, vm.main.seq), (Some(Activity::TRANSITION), Some(Sequence(6))), "crouching regrips");
        vm.command(&set, &[], false, 1 << MOD_CROUCH, 4.0);
        assert_eq!((vm.activity, vm.main.seq), (Some(Activity::VM_IDLE), Some(Sequence(3))), "into the crouched idle");
    }

    #[test]
    fn modifiers_follow_the_player() {
        let mut p = Player { ground: None, sprint_end_time: 2.0, left_ground_time: 2.04, traversal_state: 2, prev_up: [0.0, 0.0, 1.0], ..Player::default() };
        let m = player_modifiers(&p);
        assert_eq!(m, 1 << MOD_CROUCH | 1 << MOD_AIR | 1 << 32, "crouched, in the air, sprint ended on leaving the ground");
        p.left_ground_time = 2.1;
        p.ground = Some(0);
        p.sprinting = true;
        assert_eq!(player_modifiers(&p), 1 << MOD_CROUCH | 1 << MOD_SPRINTING);
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

    #[test]
    fn a_landing_cut_into_a_jump_fades_the_jump_out() {
        let set = fists();
        let mut vm = Viewmodel::default();
        vm.deploy(&set, false, 0.0);
        vm.event(PlayerEvent::Jump, &set, 1.0);
        vm.frame(&set, 1.0);
        vm.frame(&set, 1.5);
        vm.event(PlayerEvent::LandMedium, &set, 1.5);
        let jump = |b: &Blends| b.layers.iter().find(|l| l.0 == 7).map(|l| l.2);
        let (b, _) = vm.frame(&set, 1.5);
        assert_eq!(jump(&b), Some(1.0), "the jump kept at full weight as the landing starts");
        let (b, _) = vm.frame(&set, 1.525);
        assert!(jump(&b).is_some_and(|w| 0.0 < w && w < 1.0), "over the landing's 0.05 s fade-in");
        let (b, _) = vm.frame(&set, 1.56);
        assert_eq!(jump(&b), None);
    }
}
