use crate::movement::{step, unsupported};
use crate::trace::{State, Step, Trace};

pub const TOL: f32 = 1e-3;

pub struct Report {
    pub steps: usize,
    pub failures: usize,
    pub max_origin: f32,
    pub max_velocity: f32,
    pub first_failure: Option<(Step, State)>,
    pub unsupported: Option<(u32, &'static str)>,
    pub trajectory_max_origin: f32,
    pub trajectory_max_velocity: f32,
}

fn err(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3).map(|i| (a[i] - b[i]).abs()).fold(0.0, f32::max)
}

pub fn replay(t: &Trace) -> Report {
    let mut r = Report {
        steps: t.steps.len(),
        failures: 0,
        max_origin: 0.0,
        max_velocity: 0.0,
        first_failure: None,
        unsupported: None,
        trajectory_max_origin: 0.0,
        trajectory_max_velocity: 0.0,
    };
    let mut chained: Option<State> = None;
    let mut prev_seq = None;
    for s in &t.steps {
        if let Some(why) = unsupported(&t.settings, &s.input) {
            r.unsupported.get_or_insert((s.cmd.seq, why));
        }
        let ours = step(&t.settings, &s.cmd, &s.input);
        let eo = err(ours.origin, s.output.origin);
        let ev = err(ours.velocity, s.output.velocity);
        r.max_origin = r.max_origin.max(eo);
        r.max_velocity = r.max_velocity.max(ev);
        if eo > TOL || ev > TOL || ours.onground != s.output.onground {
            r.failures += 1;
            r.first_failure.get_or_insert_with(|| (s.clone(), ours));
        }

        let start = match (chained.take(), prev_seq) {
            (Some(c), Some(p)) if s.cmd.seq == p + 1 => c,
            _ => s.input.clone(),
        };
        let traj = step(&t.settings, &s.cmd, &start);
        r.trajectory_max_origin = r.trajectory_max_origin.max(err(traj.origin, s.output.origin));
        r.trajectory_max_velocity = r.trajectory_max_velocity.max(err(traj.velocity, s.output.velocity));
        chained = Some(traj);
        prev_seq = Some(s.cmd.seq);
    }
    r
}
