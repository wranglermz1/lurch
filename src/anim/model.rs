//! `AnimSet` over a model loaded from the game's files: its sequences' activities, modifiers,
//! weights, lengths and events, read from the sequence descriptions.

use super::bones::{self, Skeleton, STUDIO_LOOPING};
use super::{Activity, AnimSet, PoseParam, Sequence};
use crate::assets::model::Model;

/// Event names sequences use, by the number the oracle registers them under (`tools/tables.py`).
const EVENTS: &[(&str, u16)] = &[
    ("AE_CL_PLAYSOUND", super::AE_CL_PLAYSOUND),
    ("AE_CL_PLAYSOUND_FOR_TYPE", super::AE_CL_PLAYSOUND_FOR_TYPE),
    ("AE_SV_PLAYSOUND", super::AE_SV_PLAYSOUND),
    ("AE_CL_STOPSOUND", super::AE_CL_STOPSOUND),
    ("AE_WPN_RUMBLE", super::AE_WPN_RUMBLE),
    ("AE_CL_VSCRIPT_CALLBACK", super::AE_CL_VSCRIPT_CALLBACK),
    ("AE_FOOTSTEP_L", super::AE_FOOTSTEP_L),
    ("AE_FOOTSTEP_R", super::AE_FOOTSTEP_R),
    ("AE_GROUND_CONTACT_L", super::AE_GROUND_CONTACT_L),
    ("AE_GROUND_CONTACT_R", super::AE_GROUND_CONTACT_R),
];

/// The number the oracle registers an event name under.
pub fn event_number(name: &str) -> Option<u16> {
    EVENTS.iter().find(|(e, _)| *e == name).map(|(_, id)| *id)
}

#[derive(Clone, Debug, Default)]
pub struct SeqInfo {
    pub label: String,
    pub activity: Option<Activity>,
    pub activity_name: String,
    pub weight: i32,
    pub flags: u32,
    /// (name, negated): a negated modifier rules the sequence out when the player has it.
    pub modifiers: Vec<(String, bool)>,
    pub fade_in: f32,
    pub fade_out: f32,
    pub entry_node: i32,
    pub exit_node: i32,
    /// Cycles per second at rate 1, and seconds.
    pub rate: f32,
    pub duration: f32,
    pub events: Vec<(f32, u16)>,
    /// Every event, by name, with its options: what sound or rumble it names.
    pub named_events: Vec<(f32, String, String)>,
}

pub struct ModelAnims {
    pub sk: Skeleton,
    pub seqs: Vec<SeqInfo>,
    pose_params: Vec<(String, f32, f32)>,
}

impl ModelAnims {
    #[cfg(test)]
    pub fn from_seqs(seqs: Vec<SeqInfo>) -> ModelAnims {
        ModelAnims { sk: Skeleton::default(), seqs, pose_params: Vec::new() }
    }

    pub fn new(m: &Model) -> ModelAnims {
        let sk = Skeleton::new(m);
        let seqs = (0..sk.seqs.len())
            .map(|i| match sk.seq(i) {
                None => SeqInfo { label: "ref".into(), fade_in: 0.2, fade_out: 0.2, ..SeqInfo::default() },
                Some((_, s)) => {
                    let d = &s.desc;
                    let activity_name = d.cstr(d.rel(0, 8)).to_string();
                    let modifiers = (0..d.i32(0xbc) as usize)
                        .map(|k| {
                            let o = d.i32(0xb8) as usize + k * 8;
                            (d.cstr(d.rel(o, 0)).to_string(), d.u8(o + 4) != 0)
                        })
                        .collect();
                    let named_events: Vec<(f32, String, String)> = (0..d.i32(0x18) as usize)
                        .map(|k| {
                            let o = d.i32(0x1c) as usize + k * 0x110;
                            (d.f32(o), d.cstr(d.rel(o, 0x10c)).to_string(), d.cstr(o + 12).to_string())
                        })
                        .collect();
                    let events = named_events.iter().filter_map(|(c, n, _)| event_number(n).map(|id| (*c, id))).collect();
                    SeqInfo {
                        label: bones::seq_label(s).to_string(),
                        activity: Activity::by_name(&activity_name),
                        activity_name,
                        weight: d.i32(0x14),
                        flags: bones::seq_flags(s),
                        modifiers,
                        fade_in: d.f32(0x68),
                        fade_out: d.f32(0x6c),
                        entry_node: d.i32(0x70),
                        exit_node: d.i32(0x74),
                        rate: bones::cycle_rate(s),
                        duration: bones::duration(s),
                        events,
                        named_events,
                    }
                }
            })
            .collect();
        let pose_params = m.studio.pose_params().into_iter().map(|p| (p.name, p.start, p.end)).collect();
        ModelAnims { sk, seqs, pose_params }
    }

    /// The sequence the node graph plays between `from` and `to` (FindTransitionSequence,
    /// `0x5f9d40` → `0xb41910`): when `from` exits at a node other than the one `to` enters
    /// at, and neither is 0, the transition from the first node to the second. Ours finds it
    /// among the sequences without an activity by their entry and exit nodes; the model's own
    /// node table (studiohdr `+0x10c`) is not read.
    pub fn transition(&self, from: Sequence, to: Sequence) -> Option<Sequence> {
        let (a, b) = (self.seqs.get(from.0 as usize)?, self.seqs.get(to.0 as usize)?);
        let (exit, entry) = (a.exit_node, b.entry_node);
        if exit == entry || exit == 0 || entry == 0 {
            return None;
        }
        self.seqs.iter().position(|s| s.activity.is_none() && s.entry_node == exit && s.exit_node == entry).map(|i| Sequence(i as u32))
    }
}

impl AnimSet for ModelAnims {
    /// SelectWeightedSequence (`0xb40b50`): `current` stays if it has the activity and a
    /// negative weight. Otherwise the best score (`0xb40550`): a point for each modifier the
    /// sequence lists, less 1001 for each it wants that the player lacks, or rules out that the
    /// player has. With no modifiers the oracle draws among all by weight, and among equal
    /// scores too; the first is taken here.
    fn select(&self, activity: Activity, modifiers: &[&str], current: Option<Sequence>) -> Option<Sequence> {
        if let Some(c) = current.filter(|c| self.seqs.get(c.0 as usize).is_some_and(|s| s.activity == Some(activity) && s.weight < 0)) {
            return Some(c);
        }
        let mut best: Option<(usize, i32)> = None;
        for (i, s) in self.seqs.iter().enumerate().filter(|(_, s)| s.activity == Some(activity)) {
            if modifiers.is_empty() {
                return Some(Sequence(i as u32));
            }
            let mut score = s.modifiers.len() as i32;
            for (m, negated) in &s.modifiers {
                if modifiers.iter().any(|p| p.eq_ignore_ascii_case(m)) == *negated {
                    score -= 1001;
                }
            }
            if best.is_none_or(|(_, b)| b < score) {
                best = Some((i, score));
            }
        }
        best.map(|(i, _)| Sequence(i as u32))
    }

    fn activity(&self, seq: Sequence) -> Option<Activity> {
        self.seqs.get(seq.0 as usize)?.activity
    }

    fn duration(&self, seq: Sequence) -> f32 {
        self.seqs.get(seq.0 as usize).map_or(0.0, |s| s.duration)
    }

    fn ground_speed(&self, _: Sequence) -> f32 {
        0.0
    }

    fn looping(&self, seq: Sequence) -> bool {
        self.seqs.get(seq.0 as usize).is_some_and(|s| s.flags & STUDIO_LOOPING != 0)
    }

    fn pose_range(&self, param: PoseParam) -> Option<(f32, f32)> {
        self.pose_params.iter().find(|(n, _, _)| n == param.name()).map(|&(_, a, b)| (a, b))
    }

    fn events(&self, seq: Sequence) -> &[(f32, u16)] {
        self.seqs.get(seq.0 as usize).map_or(&[], |s| &s.events)
    }
}
