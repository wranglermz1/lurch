//! The grapple's cooldown: its weapon's clip, emptied by script while the hook holds and
//! refilled by the engine's ammo regen. The weapon is not movement, so this lives in the shell;
//! recovered behaviour is in `docs/ui.md` ("The grapple's cooldown").

use lurch::sim::cue::Cue;
use lurch::sim::math::V3;
use lurch::sim::pilot::Pilot;

/// `mp_ability_grapple.txt`.
pub const CLIP_SIZE: i32 = 300;
pub const AMMO_MIN_TO_FIRE: i32 = 300;
pub const AMMO_PER_SHOT: i32 = 300;
const REGEN_AMMO_REFILL_RATE: f32 = 10.0;
const REGEN_AMMO_REFILL_START_DELAY: f32 = 0.0;
const FIRE_DURATION: f32 = 0.001;
const COOLDOWN_TIME: f32 = 0.0;
/// Playlist vars `pathfinder_grapple_drain_base_cost`, `_min_dist` and `_max_dist`, as
/// `mp_ability_grapple.nut` defaults them.
const DRAIN_BASE_COST: i32 = 100;
const DRAIN_MIN_DIST: f32 = 300.0;
const DRAIN_MAX_DIST: f32 = 4500.0;
/// `GrappleDecreaseAmmo`'s loop: its wait, the speed that keeps it going after the grapple,
/// and its time limit.
const DRAIN_WAIT: f32 = 0.1;
const DRAIN_SPEED: f32 = 500.0;
const DRAIN_MAX_TIME: f32 = 5.0;
/// The regen timer's marks: `RegenerateAmmoReset` (`0xa5a680`) and not regenerating
/// (`0xa3f46e`).
const REGEN_RESET: f32 = -2.0;
const REGEN_IDLE: f32 = -1.0;

#[derive(Clone, Debug)]
pub struct GrappleClip {
    pub clip: i32,
    /// The mod `grapple_regen_stop`: a refill rate of 0.
    regen_stop: bool,
    /// When the weapon last fired, as the regen reads it (`+0x1304`).
    last_fire: f32,
    /// The regen's timer (`+0x1658`): when the last round came, less the part of the next.
    regen_time: f32,
    drains: Vec<Drain>,
}

/// A `GrappleDecreaseAmmo` thread.
#[derive(Clone, Debug)]
struct Drain {
    start: f32,
    wake: f32,
    start_ammo: i32,
    last_pos: V3,
    path: f32,
    was_ziplining: bool,
}

impl Default for GrappleClip {
    fn default() -> Self {
        GrappleClip { clip: CLIP_SIZE, regen_stop: false, last_fire: 0.0, regen_time: REGEN_IDLE, drains: Vec::new() }
    }
}

/// `GraphCapped`: `x` from `[x0, x1]` onto `[y0, y1]`, clamped.
fn graph_capped(x: f32, x0: f32, x1: f32, y0: f32, y1: f32) -> f32 {
    let t = ((x - x0) / (x1 - x0)).clamp(0.0, 1.0);
    y0 + t * (y1 - y0)
}

fn distance_2d(a: V3, b: V3) -> f32 {
    ((a[0] - b[0]) * (a[0] - b[0]) + (a[1] - b[1]) * (a[1] - b[1])).sqrt()
}

impl GrappleClip {
    /// Enough in the clip to fire (`ammo_min_to_fire`).
    pub fn ready(&self) -> bool {
        AMMO_MIN_TO_FIRE <= self.clip
    }

    /// `RUI_TRACK_WEAPON_CLIP_AMMO_FRACTION`.
    pub fn fraction(&self) -> f32 {
        self.clip as f32 / CLIP_SIZE as f32
    }

    pub fn refill_rate(&self) -> f32 {
        if self.regen_stop { 0.0 } else { REGEN_AMMO_REFILL_RATE }
    }

    /// `RegenerateAmmoReset` (`0xa5a680`).
    fn regen_reset(&mut self) {
        if 0.0 < self.refill_rate() {
            self.regen_time = REGEN_RESET;
        }
    }

    /// One command's worth after movement ran: a hook that caught starts a drain
    /// (`CodeCallback_OnGrappleAttach`), each drain whose wait is over runs, then the regen.
    pub fn think(&mut self, now: f32, frametime: f32, pilot: &Pilot) {
        let ziplining = pilot.player.zipline.is_some();
        if pilot.cues.iter().any(|c| matches!(c, Cue::GrappleCatch { .. })) {
            self.catch(now, pilot.origin, ziplining);
        }
        let mut i = 0;
        while i < self.drains.len() {
            if self.drains[i].wake <= now && self.drain(i, now, pilot, ziplining) {
                let d = self.drains.remove(i);
                self.reduce(d.path + distance_2d(d.last_pos, pilot.origin), d.start_ammo);
            } else {
                i += 1;
            }
        }
        self.regen(now, frametime);
    }

    /// `GrappleDecreaseAmmo` up to its loop.
    fn catch(&mut self, now: f32, origin: V3, ziplining: bool) {
        let start_ammo = self.clip;
        self.clip = 0;
        self.regen_stop = true;
        self.regen_reset();
        self.drains.push(Drain { start: now, wake: now, start_ammo, last_pos: origin, path: 0.0, was_ziplining: ziplining });
    }

    /// One pass of `GrappleDecreaseAmmo`'s loop; true when the thread ends.
    fn drain(&mut self, i: usize, now: f32, pilot: &Pilot, ziplining: bool) -> bool {
        let v = pilot.velocity;
        if !(pilot.player.grappling || (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt() > DRAIN_SPEED) {
            return true;
        }
        self.clip = 0;
        let d = &mut self.drains[i];
        d.path += distance_2d(d.last_pos, pilot.origin);
        d.last_pos = pilot.origin;
        if ziplining && !d.was_ziplining {
            return true;
        }
        if !ziplining {
            d.was_ziplining = false;
        }
        if now - d.start > DRAIN_MAX_TIME {
            return true;
        }
        d.wake = now + DRAIN_WAIT;
        false
    }

    /// `ReduceAmmoBasedOnDistance`, which the thread's end runs with the path it drew in the
    /// plane.
    fn reduce(&mut self, path: f32, start_ammo: i32) {
        let xy = graph_capped(path, DRAIN_MIN_DIST, DRAIN_MAX_DIST, 0.0, 1.0);
        let amount = graph_capped(xy, 0.0, 1.0, 0.0, AMMO_MIN_TO_FIRE as f32) as i32 + DRAIN_BASE_COST;
        self.clip = (start_ammo - amount).clamp(0, AMMO_MIN_TO_FIRE);
        self.regen_stop = false;
        self.regen_reset();
    }

    /// The weapon's clip regen (`0xa5a520`, `0xa59ac0`, `0xa59c40`) for a weapon without
    /// `regen_ammo_stockpile_max_fraction`. A new run starts its timer one frame back when the
    /// timer is marked; the engine tests a field written only there (`+0x2aac`), which would
    /// refill a later run at once, so that test is taken to be the timer's.
    #[allow(clippy::neg_cmp_op_on_partial_ord)] // the engine's test, false for a NaN
    fn regen(&mut self, now: f32, frametime: f32) {
        if self.regen_time == REGEN_RESET {
            self.last_fire = now - FIRE_DURATION;
            self.regen_time = REGEN_IDLE;
        }
        let rate = self.refill_rate();
        let waited = !(now <= self.last_fire + FIRE_DURATION + REGEN_AMMO_REFILL_START_DELAY + COOLDOWN_TIME);
        if !(0.0 < rate && self.last_fire != 0.0 && self.clip < CLIP_SIZE && waited) {
            if REGEN_IDLE < self.regen_time {
                self.regen_time = REGEN_IDLE;
            }
            return;
        }
        if self.regen_time <= 0.0 {
            self.regen_time = now - frametime;
        }
        let x = (now - self.regen_time) * rate;
        if 1.0 <= x {
            self.regen_time = now - (x - (x as i32) as f32) / rate;
            self.clip = (self.clip + x as i32).min(CLIP_SIZE);
        }
    }
}

