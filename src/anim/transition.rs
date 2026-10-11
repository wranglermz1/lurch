//! The client's sequence transitioner: when an entity's sequence changes, the one it left plays
//! on under the new one for the shorter of its own fade-out and the new one's fade-in, its
//! weight falling along a smoothstep. Up to six are kept, newest last.
//!
//! Each frame the oracle checks for a change (`0x898d60`), records the current sequence
//! (`0x899250`) and updates the weights (`0x899600`) for every animating entity (`0x5fc3f0`);
//! StandardBlendingRules then blends the kept sequences in after the main one (`0x899720`).
//!
//! Not modelled: the reset when the model changes or the entity is told not to interpolate,
//! and the skip `0x899720` makes for a player's layers older than its last teleport.

use super::bones::{clamp_cycle, STUDIO_LOOPING, STUDIO_SNAP};
use super::model::SeqInfo;

/// ConVars `sequence_transitioner_enable` and `anim_transitionsequences` (image values).
pub const SEQUENCE_TRANSITIONER_ENABLE: bool = true;
pub const ANIM_TRANSITIONSEQUENCES: bool = true;
pub const MAX_KEPT: usize = 6;

/// A sequence as it was playing: from `cycle` at `anim_time`, at `rate`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Blend {
    pub seq: i32,
    pub cycle: f32,
    pub weight: f32,
    pub rate: f32,
    pub anim_time: f32,
    pub fade: f32,
}

#[derive(Clone, Debug, Default)]
pub struct Transitioner {
    current: Option<Blend>,
    fade: f32,
    parity: i32,
    pub kept: Vec<Blend>,
}

/// The fade between two sequences (`0xb42420`): the old one's fade-out or the new one's
/// fade-in, whichever is shorter.
fn fade_between(seqs: &[SeqInfo], from: usize, to: usize) -> f32 {
    let (out, fade_in) = (seqs[from].fade_out, seqs[to].fade_in);
    if fade_in <= out { fade_in } else { out }
}

impl Transitioner {
    /// CheckForSequenceChange (`0x898d60`): keeps the sequence being left, unless the new one
    /// snaps in.
    pub fn check_for_sequence_change(&mut self, seqs: &[SeqInfo], seq: i32, parity: i32) {
        if !SEQUENCE_TRANSITIONER_ENABLE {
            return;
        }
        let Some(cur) = self.current else { return };
        if cur.anim_time == 0.0 || (cur.seq == seq && parity == self.parity) {
            return;
        }
        let fade = match usize::try_from(seq).ok().filter(|&s| s < seqs.len()) {
            Some(s) => {
                if seqs[s].flags & STUDIO_SNAP != 0 {
                    self.kept.clear();
                    self.fade = 0.0;
                    return;
                }
                self.fade = fade_between(seqs, cur.seq.max(0) as usize, s);
                self.fade
            }
            None => {
                self.kept.clear();
                self.fade
            }
        };
        if 0.0 < fade && self.kept.last().is_none_or(|b| b.anim_time < cur.anim_time) {
            if self.kept.len() == MAX_KEPT {
                self.kept.remove(0);
            }
            self.kept.push(Blend { fade: self.fade, ..cur });
        }
    }

    /// UpdateCurrent (`0x899250`): the sequence playing now, and the kept ones whose fade has
    /// run out dropped, with every one older.
    pub fn update_current(&mut self, now: f32, seq: i32, cycle: f32, rate: f32, parity: i32) {
        if !SEQUENCE_TRANSITIONER_ENABLE {
            return;
        }
        self.parity = parity;
        self.current = Some(Blend { seq, cycle, weight: 1.0, rate, anim_time: now, fade: 0.0 });
        if let Some(i) = self.kept.iter().rposition(|b| b.fade + b.anim_time + 0.0 <= now) {
            self.kept.drain(..=i);
        }
    }

    /// The kept sequences' weights (`0x899600`): each a smoothstep of the fade left, times
    /// every newer one's.
    pub fn update_weights(&mut self, now: f32) {
        let mut acc = 1.0f32;
        for b in self.kept.iter_mut().rev() {
            let s = if 0.0 < b.fade {
                let d = 1.0f64 - ((now - b.anim_time) / b.fade) as f64;
                let d = if !(d >= 0.0) { 0.0 } else if 1.0 < d { 1.0 } else { d };
                let s = d as f32;
                (3.0 - (s + s)) * (s * s)
            } else {
                0.0
            };
            let s = if s > 0.0 { s } else { 0.0 };
            let s = if s < 1.0 { s } else { 1.0 };
            acc *= s;
            b.weight = acc;
        }
    }

    /// MaintainSequenceTransitions (`0x899720`): what to blend in after the main sequence, in
    /// order, as (sequence, cycle, weight). A kept sequence is skipped when the one before it
    /// will cover it at full weight.
    pub fn maintain(&mut self, seqs: &[SeqInfo], now: f32) -> Vec<(usize, f32, f32)> {
        if !ANIM_TRANSITIONSEQUENCES {
            return Vec::new();
        }
        self.update_weights(now);
        let mut out = Vec::new();
        for i in (0..self.kept.len()).rev() {
            let b = self.kept[i];
            if 1 <= i && !(self.kept[i - 1].weight < 1.0) || !(0.0 < b.weight) {
                continue;
            }
            let Some(info) = usize::try_from(b.seq).ok().and_then(|s| seqs.get(s)) else { continue };
            let cycle = clamp_cycle((now - b.anim_time) * info.rate * b.rate + b.cycle, info.flags & STUDIO_LOOPING != 0);
            out.push((b.seq as usize, cycle, b.weight));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seq(fade_in: f32, fade_out: f32, flags: u32) -> SeqInfo {
        SeqInfo { fade_in, fade_out, flags, rate: 1.0, ..SeqInfo::default() }
    }

    #[test]
    fn the_sequence_left_fades_out_over_the_shorter_fade() {
        let seqs = [seq(0.2, 0.2, 0), seq(0.2, 0.35, 0), seq(0.05, 0.2, 0), seq(0.2, 0.2, STUDIO_SNAP)];
        let mut t = Transitioner::default();
        t.check_for_sequence_change(&seqs, 1, 0);
        t.update_current(1.0, 1, 0.25, 1.0, 0);
        assert!(t.kept.is_empty(), "no change before a sequence has played");
        t.check_for_sequence_change(&seqs, 2, 0);
        t.update_current(1.02, 2, 0.0, 1.0, 0);
        assert_eq!(t.kept, [Blend { seq: 1, cycle: 0.25, weight: 1.0, rate: 1.0, anim_time: 1.0, fade: 0.05 }]);
        let at = |t: &mut Transitioner, now| t.maintain(&seqs, now);
        assert_eq!(at(&mut t, 1.0), [(1, 0.25, 1.0)]);
        let half = at(&mut t, 1.025);
        assert!(half[0].0 == 1 && (half[0].2 - 0.5).abs() < 1e-5);
        assert!(at(&mut t, 1.051).is_empty(), "weight 0 past the end of the fade");
        t.update_current(1.06, 2, 0.0, 1.0, 0);
        assert!(t.kept.is_empty(), "dropped once its fade has run out");
        t.check_for_sequence_change(&seqs, 3, 0);
        assert!(t.kept.is_empty(), "a snap sequence keeps nothing");
    }

    #[test]
    fn six_are_kept_and_newer_weights_cover_older() {
        let seqs = [seq(0.2, 0.2, 0), seq(0.2, 0.2, 0)];
        let mut t = Transitioner::default();
        t.update_current(0.5, 0, 0.0, 1.0, 0);
        for k in 1..=8 {
            t.check_for_sequence_change(&seqs, k % 2, 0);
            t.update_current(0.5 + 0.001 * k as f32, k % 2, 0.0, 1.0, 0);
        }
        assert_eq!(t.kept.len(), MAX_KEPT);
        t.update_weights(0.6);
        assert!(t.kept.windows(2).all(|w| w[0].weight <= w[1].weight));
    }
}
