//! The first-person proxy: the legend's arms model as an animating entity of its own, which
//! the traversal move shows and drives. start_traversal (`0x8100f0`) shows it, end_traversal
//! (`0x815210`) hides it, and traversal_move (`0x8139e0`) gives it the mantle activity for the
//! traversal's type and sets its cycle. The client reads its events as the viewmodel's, on the
//! main sequence only (`0x5f15c0`, `0x5f1750`), and keeps its sequence transitions as for every
//! animating entity. Each view frame places it so its `ref` attachment sits on the ledge
//! (`0x680170`), held back until the hand time, and moves the view onto its `camera`
//! attachment (`0x9c9c70`), easing back out after.
//!
//! A wall climb shows it too (`0x866880`): it plays `wall_climb` while the player rises, at
//! the player's feet facing the wall, slid in from behind and below as the view's blend onto
//! it runs in (`0x9cad50`), and the view follows how its `CAMERA` moves against `CAMERA_BASE`
//! (`0x86a8d0`), blended in by the same amount (`0x67eff0`).
//!
//! Not modelled: the `ledgeAngle` and `aim_yaw` pose parameters (0; the player's yaw and its
//! eyes' agree in every recording, which puts `aim_yaw` at 0), windows (the view's lerp in for
//! them), and the weighted draw among clips of one activity (the first is taken).

use super::bones::{Mat, STUDIO_LOOPING};
use super::model::ModelAnims;
use super::transition::Transitioner;
use super::viewmodel::{Blends, Layer, NEW_MAIN_CYCLE};
use super::{Activity, AnimSet, Sequence};
use crate::sim::cue::GrappleAnim;
use crate::sim::mantle::{JUMP_OFF, REST};
use crate::sim::math::{angle_vectors, TINY, V3};

/// The climb's sequence (`wall_climb_pose_paramteter_hands_enabled` 1 picks it over
/// `ptpov_wall_climb`).
const WALL_CLIMB: &str = "wall_climb";
/// ConVar `first_person_proxy_blend_distance`: how far back the climbing arms start.
const BLEND_DISTANCE: f32 = 50.0;

/// The traversal type's activity is this plus the type (`ACT_VM_MANTLE_BELOW` for type 3).
const TRAVERSAL_ACTIVITY: u16 = 0x236;
/// The longest the hands wait after a mantle starts while the weapon's viewmodel is drawn.
pub const HAND_DELAY: f32 = 1.0;
/// After the hand time the hands slide in over this long, from this far back.
const SLIDE_TIME: f32 = 0.15;
const SLIDE_BACK: f32 = 16.0;
/// ConVars `traversal_viewLerpInDuration`, `traversal_viewLerpOut`, `traversal_viewLerpOutPos`
/// and `traversal_viewLerpOutAngle` (image values).
pub const TRAVERSAL_VIEW_LERP_IN_DURATION: f32 = 0.15;
pub const TRAVERSAL_VIEW_LERP_OUT: bool = true;
pub const TRAVERSAL_VIEW_LERP_OUT_POS: bool = true;
pub const TRAVERSAL_VIEW_LERP_OUT_ANGLE: bool = true;
/// How long the view eases out after the traversal's view ends.
const VIEW_LERP_OUT: f32 = 0.2;
/// How far short of the end, past half the standing hull, the view must stay.
const VIEW_END_MARGIN: f32 = 10.0;
/// The traversal activities, `ACT_VM_TRAVERSAL_WINDOW_LEFT` to `ACT_VM_MANTLE_JUMPOFF`.
const TRAVERSAL_ACTIVITIES: std::ops::RangeInclusive<u16> = 0x237..=0x242;

/// A mantle as one movement call leaves it: type and cycle (`+0x26e8`, `+0x1eb4`), whether
/// the gun is still up (`+0x2764`), when it started, and the ledge point, forward, mid and
/// end of its path in the world.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Traversal {
    pub ty: i32,
    pub cycle: f32,
    pub gun_up: bool,
    pub start_time: f32,
    pub ledge: V3,
    pub forward: V3,
    pub mid: V3,
    pub end: V3,
}

/// The view's offset from where the player's own view would be, kept for the ease out
/// (`+0x1380`, `+0x138c`), until `end` (`+0x1398`), last scaled at `last` (`+0x139c`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct ViewOffset {
    origin: V3,
    angles: V3,
    end: f32,
    last: f32,
}

/// A wall climb as one movement call leaves it: the arms are on (`+0x276c`), on the wall
/// with this normal (the up vector), and the player was rising as the call began (its
/// absolute velocity's z then above `FLT_EPSILON`), which plays the climb.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Climb {
    pub normal: V3,
    pub rising: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Proxy {
    /// Shown (`+0x3e8`): through a mantle, and through a climb's arms.
    pub visible: bool,
    traversing: bool,
    /// The wall's normal while the climb's arms are on.
    wall: Option<V3>,
    /// How far the climbing arms have slid in (`+0x1358`).
    slide: f32,
    /// The view the climbing arms last gave (`+0x2874`, `+0x2880` on the player).
    entity_view: (V3, V3),
    ty: i32,
    /// When the hands come in (`+0x2738`).
    pub hand_time: f32,
    path: Traversal,
    view: ViewOffset,
    pub main: Layer,
    pub transitions: Transitioner,
    /// Playing the grapple's sequences (`0x9d5290`), and the ones queued after the current
    /// (the player's `+0x2a54`, `+0x2a58`).
    grappling: bool,
    queue: [Option<Sequence>; 2],
    /// The entity's pose parameters, normalised: its model's at 0 until the grapple aims them.
    pub poses: Vec<f32>,
    /// The grapple's aim, pitch and yaw each with its rate (the player's `+0x3118` to
    /// `+0x3124`).
    aim: [f32; 4],
}

impl Proxy {
    /// After each movement call: the mantle while the player is in one, `None` once it has
    /// ended. `viewmodel_drawn` is whether the active weapon's viewmodel is (`0xa50fe0`);
    /// `jumpoff_duration` is ConVar `automantle_jumpoff_duration`.
    pub fn command(&mut self, set: &ModelAnims, traversal: Option<Traversal>, viewmodel_drawn: bool, jumpoff_duration: f32, now: f32) {
        let Some(t @ Traversal { ty, cycle, gun_up, .. }) = traversal else {
            if self.traversing {
                self.traversing = false;
                self.hide();
            }
            return;
        };
        let first = !self.traversing;
        if first {
            self.traversing = true;
            self.show();
            self.hand_time = if viewmodel_drawn { now + HAND_DELAY } else { now };
        }
        self.path = t;
        if first || ty != self.ty {
            self.ty = ty;
            if let Some(seq) = set.select(Activity(TRAVERSAL_ACTIVITY + ty as u16), &[], None) {
                self.reset_sequence(set, seq, now);
                if ty == REST {
                    self.main.rate = 1.0;
                } else if ty == JUMP_OFF {
                    self.main.rate = set.seqs[seq.0 as usize].duration / jumpoff_duration;
                }
            }
        }
        if (ty - REST) as u32 > 1 {
            (self.main.from, self.main.rate, self.main.start) = (cycle, 0.0, now);
        }
        if self.hand_time > now && (!gun_up || !viewmodel_drawn) {
            self.hand_time = now;
        }
    }

    /// `0x9caa00`: shown, its transitions dropped if it was not.
    fn show(&mut self) {
        if !self.visible {
            self.visible = true;
            self.transitions = Transitioner::default();
        }
    }

    fn hide(&mut self) {
        self.visible = false;
        self.transitions = Transitioner::default();
    }

    /// After each movement call: the climb's arms while they are on (`0x866880`), `None` once
    /// they stop (`0x866e00`, `end_climb` `0x866760`), which hides them unless a mantle has
    /// taken them over. The rate follows whether the player rises, so the hands hold still
    /// on a wall they slide down; the first call plays the climb from its start.
    pub fn climb(&mut self, set: &ModelAnims, climb: Option<Climb>, now: f32) {
        let Some(c) = climb else {
            if self.wall.take().is_some() && !self.traversing {
                self.hide();
            }
            return;
        };
        let rate = if c.rising { 1.0 } else { 0.0 };
        if self.wall.is_none() {
            self.show();
            self.transitions = Transitioner::default();
            if let Some(i) = set.seqs.iter().position(|s| s.label.eq_ignore_ascii_case(WALL_CLIMB)) {
                self.reset_sequence(set, super::Sequence(i as u32), now);
            }
        } else if self.main.rate != rate {
            (self.main.from, self.main.rate, self.main.start) = (self.main.cycle(set, now), rate, now);
        }
        self.wall = Some(c.normal);
    }

    /// The grapple's animation (`0x9d5290`, `0x9d5650`): the start plays
    /// `ACT_VM_GRAPPLE_START` with `ACT_VM_GRAPPLE_IDLE` queued, the stop
    /// `ACT_VM_GRAPPLE_STOP`, and the end hides the arms.
    pub fn grapple(&mut self, set: &ModelAnims, cue: GrappleAnim, now: f32) {
        let pick = |a| set.select(a, &[], None);
        match cue {
            GrappleAnim::Start => match (pick(Activity::VM_GRAPPLE_START), pick(Activity::VM_GRAPPLE_IDLE)) {
                (Some(start), idle) => self.grapple_play(set, start, idle, now),
                (None, Some(idle)) => self.grapple_play(set, idle, None, now),
                (None, None) => {}
            },
            GrappleAnim::Stop => {
                if let Some(stop) = pick(Activity::VM_GRAPPLE_STOP) {
                    self.grapple_play(set, stop, None, now);
                }
            }
            GrappleAnim::End => {
                self.aim = [0.0; 4];
                if self.grappling {
                    self.grappling = false;
                    self.queue = [None; 2];
                    self.hide();
                }
            }
        }
    }

    /// `0x9d5290` on the proxy: shown, it plays `seq` from its start, through the node
    /// graph's transition first when there is one, with `next` queued after.
    fn grapple_play(&mut self, set: &ModelAnims, seq: Sequence, next: Option<Sequence>, now: f32) {
        let via = self.main.seq.and_then(|cur| set.transition(cur, seq)).filter(|&t| t != seq);
        self.show();
        self.grappling = true;
        match via {
            Some(t) => {
                self.main.restart(t, now, 1.0);
                self.queue = [Some(seq), next];
            }
            None => {
                self.main.restart(seq, now, 1.0);
                self.queue = [next, None];
            }
        }
    }

    /// PostThink's turn for the grapple's animation (`0x86ef40`), once a command: when the
    /// sequence playing has finished the next queued plays, and with none queued the
    /// animation ends, which it says: the player's state (`Player::grapple_anim_end`) is the
    /// caller's to end with it.
    pub fn grapple_think(&mut self, set: &ModelAnims, now: f32) -> bool {
        if !self.grappling {
            return false;
        }
        let Some(info) = self.main.seq.and_then(|q| set.seqs.get(q.0 as usize)) else { return false };
        let finished = info.flags & STUDIO_LOOPING == 0 && 1.0 <= self.main.from + (now - self.main.start) * info.rate * self.main.rate;
        if !finished {
            return false;
        }
        match self.queue[0] {
            Some(q) => {
                self.grapple_play(set, q, self.queue[1], now);
                false
            }
            None => {
                self.grapple(set, GrappleAnim::End, now);
                true
            }
        }
    }

    /// Whether the grapple's arms are drawn.
    pub fn grappling(&self) -> bool {
        self.visible && self.grappling
    }

    /// Where the proxy is drawn outside a traversal or a climb (`0x680170`): its `camera`
    /// attachment (in the model's space as the current pose has it) on the eye.
    pub fn eye_place(&self, camera: &Mat, eye: V3, angles: V3) -> Mat {
        let mut e = angle_matrix(angles);
        for (i, row) in e.iter_mut().enumerate() {
            row[3] = eye[i];
        }
        concat(&e, &invert(camera))
    }

    /// Where the proxy is drawn while the grapple aims it (view setup `0x67eff0`): turned to
    /// the view's `angles`, moved so its `CAMERA_BASE` attachment (`base`, in the model's space
    /// as the current pose has it) sits on the eye.
    pub fn grapple_place(&self, base: &Mat, eye: V3, angles: V3) -> Mat {
        let mut e = angle_matrix(angles);
        let a = [base[0][3], base[1][3], base[2][3]];
        for (i, row) in e.iter_mut().enumerate() {
            let at = eye[i] + (row[0] * a[0] + row[1] * a[1] + row[2] * a[2]);
            row[3] = eye[i] - (at - eye[i]);
        }
        e
    }

    /// The grapple's aim (view setup `0x67eff0`, while `0x9d55c0` holds: the grapple's
    /// animation plays, the view holds the proxy and the grapple is out or reeling in): the
    /// rope's first point from the `eye`, in the view's frame (`angles`), as pitch and yaw, each
    /// eased toward by a critically damped spring of rate 5 over the frame's time, set on the
    /// proxy's `traversalPitch` and `traversalYaw`. The grapple's sequences auto-layer
    /// `ptpov_cloudcity_grapple_poses`, a blend grid over those two, so the arm points along
    /// the rope.
    pub fn grapple_aim(&mut self, set: &ModelAnims, target: V3, eye: V3, angles: V3, frametime: f32) {
        let sk = &set.sk;
        if self.poses.len() != sk.pose_params.len() {
            self.poses = sk.default_poses();
        }
        let (pitch_at, yaw_at) = (sk.pose_param("traversalPitch"), sk.pose_param("traversalYaw"));
        if pitch_at.is_none() && yaw_at.is_none() {
            return;
        }
        let (dx, dy, dz) = (target[0] - eye[0], target[1] - eye[1], target[2] - eye[2]);
        let l = ((dy * dy + dx * dx) + dz * dz).sqrt();
        let inv = 1.0 / if l > TINY { l } else { TINY };
        let (dx, dy, dz) = (dx * inv, dy * inv, dz * inv);
        let m = angle_imatrix(angles);
        let local = [0, 1, 2].map(|r| ((dy * m[r][1] + dx * m[r][0]) + dz * m[r][2]) + m[r][3]);
        let (pitch, yaw) = vector_angles(local);
        let wrap = |a: f32| {
            let a = a % 360.0;
            if a > 180.0 {
                a + -360.0
            } else if -180.0 > a {
                a + 360.0
            } else {
                a
            }
        };
        let [p, pv, y, yv] = &mut self.aim;
        smooth(p, pv, wrap(pitch), frametime);
        smooth(y, yv, wrap(yaw), frametime);
        if let Some(i) = pitch_at {
            self.poses[i] = sk.set_pose_parameter(i, self.aim[0]);
        }
        if let Some(i) = yaw_at {
            self.poses[i] = sk.set_pose_parameter(i, self.aim[2]);
        }
    }

    /// Whether the climb's arms are drawn: they are the player's view-offset entity.
    pub fn climbing(&self) -> bool {
        self.visible && self.wall.is_some()
    }

    /// The proxy's frame in the view setup (`0x9cad50`): how far the climbing arms have slid
    /// in follows the player's blend `blend` up while it has a view-offset entity, and drops
    /// to 0 once it has none (its lerp out is 0, and no other entity holds the view).
    pub fn slide_frame(&mut self, view_entity: bool, blend: f32) {
        let s = self.slide;
        self.slide = if view_entity {
            let s = if s <= blend { blend } else { s };
            if s <= 0.0 { 0.0 } else { s }
        } else {
            let s = if blend <= s { blend } else { s };
            if 0.0 <= s { 0.0 } else { s }
        };
    }

    /// Where the climbing arms are drawn (`0x680170` and the render offset `0x9c93f0`): at the
    /// player's `origin`, turned to face into the wall (VectorAngles of the wall's reversed
    /// normal, its pitch taken modulo 2.5), less what is left of the slide: the blend
    /// distance back along the camera's forward and the eye height down its up (`angles`,
    /// the camera's last angles, `+0x2178`).
    pub fn climb_place(&self, origin: V3, angles: V3, eye_height: f32) -> Mat {
        let n = self.wall.unwrap_or([-1.0, 0.0, 0.0]);
        let f = [-n[0], -n[1], -n[2]];
        let xy = (f[0] * f[0] + f[1] * f[1]).sqrt();
        let (pitch, yaw) = if xy > 0.001 { ((-f[2]).atan2(xy).to_degrees(), f[1].atan2(f[0]).to_degrees()) } else { ((-f[2]).atan2(xy).to_degrees(), 0.0) };
        let mut t = angle_matrix([pitch % 2.5, yaw, 0.0]);
        let (fwd, _, up) = angle_vectors(angles);
        let back = (1.0 - self.slide) * BLEND_DISTANCE;
        let down = (1.0 - self.slide) * eye_height;
        for (i, row) in t.iter_mut().enumerate() {
            row[3] = origin[i] + (-(fwd[i] * back) + -(up[i] * down));
        }
        t
    }

    /// The view through a climb, from the eye's `origin` and `angles` before punch and tilt:
    /// while the arms hold the view (`view_entity`) it is the eye moved as their `camera`
    /// sits against `base` (`0x86a8d0`; both in the model's space as the current pose has
    /// them), and the eye goes `blend` of the way there, its angles by slerp (`0x67eff0`).
    pub fn climb_view(&mut self, camera: &Mat, base: &Mat, view_entity: bool, blend: f32, origin: V3, angles: V3) -> (V3, V3) {
        if view_entity {
            let mut eye = angle_matrix(angles);
            for (i, row) in eye.iter_mut().enumerate() {
                row[3] = origin[i];
            }
            let m = concat(&eye, &concat(&invert(base), camera));
            self.entity_view = ([m[0][3], m[1][3], m[2][3]], matrix_angles(&m));
        }
        if !(blend > 0.0) {
            return (origin, angles);
        }
        let (o, a) = self.entity_view;
        let to = [(o[0] - origin[0]) * blend + origin[0], (o[1] - origin[1]) * blend + origin[1], (o[2] - origin[2]) * blend + origin[2]];
        if a == angles {
            return (to, angles);
        }
        let p = matrix_quaternion(&angle_matrix(angles));
        let q = matrix_quaternion(&angle_matrix(a));
        (to, matrix_angles(&quaternion_matrix(quaternion_slerp(p, q, blend))))
    }

    /// ResetSequence (`0x7c4d20`): the new sequence from cycle 0 at rate 1, its events read
    /// afresh; the same sequence again restarts unless it loops. The keep-the-cycle case for
    /// sequences sharing a group (seqdesc `+0xc4`) never arises: the arms' are all 0.
    fn reset_sequence(&mut self, set: &ModelAnims, seq: super::Sequence, now: f32) {
        if self.main.seq == Some(seq) && set.seqs[seq.0 as usize].flags & STUDIO_LOOPING != 0 {
            return;
        }
        self.main.restart(seq, now, 1.0);
    }

    /// Whether a mantle's hands are in: the weapon's own viewmodel is not shown meanwhile.
    pub fn hands_in(&self, now: f32) -> bool {
        self.traversing && self.visible && self.hand_time <= now
    }

    /// The proxy's transform (`0x680170`): its `ref` attachment as the current pose has it
    /// (`reference`, in the model's space), pinned to the ledge frame, and for `SLIDE_TIME`
    /// after the hand time slid back along the view's forward.
    pub fn place(&self, reference: &Mat, model_scale: f32, view_forward: V3, now: f32) -> Mat {
        let mut t = self.pinned(reference, model_scale);
        let since = now - self.hand_time;
        if since < SLIDE_TIME {
            let k = (1.0 - if since / SLIDE_TIME > 0.0 { since / SLIDE_TIME } else { 0.0 }) * SLIDE_BACK;
            let f = view_forward;
            let d = [f[1] * 0.5 + f[0], f[1] - f[0] * 0.5, f[2] + 1.0];
            for (row, d) in t.iter_mut().zip(d) {
                row[3] -= k * d;
            }
        }
        t
    }

    /// The placement without the slide: `ref` exactly on the ledge frame.
    pub fn pinned(&self, reference: &Mat, model_scale: f32) -> Mat {
        let mut a = *reference;
        let s = 1.0 / model_scale;
        for row in &mut a {
            for x in &mut row[..3] {
                *x *= s;
            }
        }
        concat(&ledge_frame(self.path.ledge, self.path.forward), &invert(&a))
    }

    /// The view (`0x9c9c70`), from the player's own `eye` and `angles` before punch and tilt.
    /// While the proxy plays a traversal activity the origin is its `camera` attachment on the
    /// ledge frame, lerped in from the eye since the start (the eye's lead along the ledge's
    /// forward dropped), and kept short of the path's end by half the standing hull
    /// (`half_hull`) less 10; the angles are the player's turned by how `camera` sits against
    /// `camera_base`. Afterwards the difference eases out over 0.2 s. `pinned` is the
    /// placement without the slide, `camera` and `base` the attachments in the model's space
    /// as the current pose has them.
    #[allow(clippy::too_many_arguments)]
    pub fn view(&mut self, set: &ModelAnims, pinned: &Mat, camera: &Mat, base: &Mat, half_hull: f32, eye: V3, angles: V3, now: f32) -> (V3, V3) {
        let traversing = self.visible && self.main.seq.and_then(|q| set.seqs.get(q.0 as usize)?.activity).is_some_and(|a| TRAVERSAL_ACTIVITIES.contains(&a.0));
        if !traversing {
            let v = &mut self.view;
            if !(v.end > now) || !TRAVERSAL_VIEW_LERP_OUT {
                (v.origin, v.angles) = ([0.0; 3], [0.0; 3]);
                return (eye, angles);
            }
            if v.last != now {
                let k = (v.end - now) / (v.end - v.last);
                v.last = now;
                v.origin = v.origin.map(|x| k * x);
                v.angles = matrix_angles(&quaternion_matrix(quaternion_scale(matrix_quaternion(&angle_matrix(v.angles)), k)));
            }
            let mut out = (eye, angles);
            if TRAVERSAL_VIEW_LERP_OUT_POS {
                out.0 = [out.0[0] + v.origin[0], out.0[1] + v.origin[1], out.0[2] + v.origin[2]];
            }
            if TRAVERSAL_VIEW_LERP_OUT_ANGLE {
                out.1 = [v.angles[0] + out.1[0], v.angles[1] + out.1[1], v.angles[2] + out.1[2]];
            }
            return out;
        }
        let cam = concat(pinned, camera);
        let rb = rotation(&concat(pinned, base));
        let turned = concat(&concat(&rotation(&cam), &transpose(&rb)), &angle_matrix(angles));
        let a = matrix_angles(&turned);
        let mut p = [cam[0][3], cam[1][3], cam[2][3]];
        let f = self.path.forward;
        let dot = |u: V3, w: V3| u[0] * w[0] + u[1] * w[1] + u[2] * w[2];
        if 0.0 < TRAVERSAL_VIEW_LERP_IN_DURATION {
            let k = 1.0 - (now - self.path.start_time) / TRAVERSAL_VIEW_LERP_IN_DURATION;
            let k = if 0.0 >= k { 0.0 } else if 1.0 <= k { 1.0 } else { k };
            let mut d = [eye[0] - p[0], eye[1] - p[1], eye[2] - p[2]];
            let lead = dot(d, f);
            if 0.0 < lead {
                d = [d[0] - f[0] * lead, d[1] - f[1] * lead, d[2] - f[2] * lead];
            }
            p = [d[0] * k + p[0], d[1] * k + p[1], d[2] * k + p[2]];
        }
        let (mid, end) = (self.path.mid, self.path.end);
        let along = dot([p[0] - mid[0], p[1] - mid[1], p[2] - mid[2]], f);
        let limit = (dot([end[0] - mid[0], end[1] - mid[1], end[2] - mid[2]], f) + half_hull) - VIEW_END_MARGIN;
        if along > limit {
            let over = along - limit;
            p = [p[0] - over * f[0], p[1] - over * f[1], p[2] - over * f[2]];
        }
        self.view = ViewOffset {
            origin: [p[0] - eye[0], p[1] - eye[1], p[2] - eye[2]],
            angles: [a[0] - angles[0], a[1] - angles[1], a[2] - angles[2]],
            end: now + VIEW_LERP_OUT,
            last: now,
        };
        (p, a)
    }

    /// The client's frame while the proxy shows: transitions recorded, events crossed, and
    /// what to blend.
    pub fn frame(&mut self, set: &ModelAnims, now: f32) -> (Blends, Vec<(u16, String)>) {
        let mut blends = Blends::default();
        let Some(seq) = self.main.seq.filter(|_| self.visible) else { return (blends, Vec::new()) };
        let cycle = self.main.cycle(set, now);
        self.transitions.check_for_sequence_change(&set.seqs, seq.0 as i32, self.main.parity);
        self.transitions.update_current(now, seq.0 as i32, cycle, self.main.rate, self.main.parity);
        let events = self.main.events(set, cycle, NEW_MAIN_CYCLE);
        blends.main = Some((seq.0 as usize, cycle));
        blends.transitions = self.transitions.maintain(&set.seqs, now);
        (blends, events)
    }
}

use super::bones::concat;

/// The grapple aim's spring rate.
const AIM_RATE: f32 = 5.0;

/// The critically damped spring the grapple's aim eases by: `value` and its `rate` toward
/// `target` over `dt`, by the exponential's cubic approximation.
fn smooth(value: &mut f32, rate: &mut f32, target: f32, dt: f32) {
    let x = dt * AIM_RATE;
    let k = 1.0 / ((x * 0.48 * x + (x + 1.0)) + x * 0.235 * x * x);
    let change = *value - target;
    let t = (change * AIM_RATE + *rate) * dt;
    *rate = (*rate - t * AIM_RATE) * k;
    *value = (t + change) * k + target;
}

/// The game's SIMD sine and cosine of an angle in degrees (inside AngleIMatrix, `0x42d130`):
/// reduced to quarter turns, then two polynomials.
fn sin_cos_quarter(deg: f32) -> (f32, f32) {
    let t = deg * f32::from_bits(0x3b36_0b61) * 4.0;
    let q = t.round_ties_even() as i32;
    let r = t - q as f32;
    let r2 = r * r;
    let sin = (((f32::from_bits(0xbb96_bda7) * r2 + f32::from_bits(0x3da3_2ebc)) * r2 + f32::from_bits(0xbf25_5dd9)) * r2 + f32::from_bits(0x3f12_1fb5)) * r + r;
    let c = (((f32::from_bits(0x3a70_1c84) * r2 + f32::from_bits(0xbcaa_e9dd)) * r2 + f32::from_bits(0x3e81_e0f6)) * r2 + f32::from_bits(0xbe6f_4f32)) * r2;
    let cos = 1.0 - (r2 - c);
    let even = q & 1 == 0;
    let s = if even { sin } else { cos };
    let c = if even { cos } else { sin };
    (f32::from_bits(s.to_bits() ^ (((q & 2) as u32) << 30)), f32::from_bits(c.to_bits() ^ ((((q + 1) & 2) as u32) << 30)))
}

/// AngleIMatrix (`0x42d130`): the inverse of AngleMatrix, its rows forward, left and up.
fn angle_imatrix(a: V3) -> Mat {
    let (sp, cp) = sin_cos_quarter(a[0]);
    let (sy, cy) = sin_cos_quarter(a[1]);
    let (sr, cr) = sin_cos_quarter(a[2]);
    let (s0, c0) = sin_cos_quarter(0.0);
    let b = [sp * cy, sp * sy, c0 * cp, c0 * s0];
    let l = [-sy, cy, s0, s0];
    [
        [cy * cp, cp * sy, -(c0 * sp), c0 * s0],
        [0, 1, 2, 3].map(|i| sr * b[i] + cr * l[i]),
        [0, 1, 2, 3].map(|i| cr * b[i] - sr * l[i]),
    ]
}

/// The game's atan2 in turns (`0x2fe780`): from the angle's offset off the diagonal, by a
/// double-precision polynomial.
fn atan2_turns(y: f32, x: f32) -> f32 {
    let sign = (x.to_bits() ^ y.to_bits()) & 0x8000_0000;
    let xs = f32::from_bits(x.to_bits() ^ sign);
    let t = ((y - xs) / (xs + y)) as f64;
    let t2 = t * t;
    let a = f64::from_bits(0xbf83_3c30_7e0c_a391) * t2 + f64::from_bits(0x3f90_1b3c_9f3f_1548);
    let c = f64::from_bits(0xbf47_ee4b_bed2_80c6) * t2 + f64::from_bits(0x3f6f_285b_6a88_47a0);
    let e = f64::from_bits(0xbf96_d496_d3af_2bee) * t2 + f64::from_bits(0x3fa0_4573_85af_af03);
    let t4 = t2 * t2;
    let c = (c * t4 + a) * (t4 * t4);
    let g = f64::from_bits(0xbfab_2948_a000_0000) * t2 + f64::from_bits(0x3fc4_5f30_6dc9_c883);
    let p = (c + (e * t4 + g)) * t;
    let r = f32::from_bits(((p as f32) - 0.125).to_bits() ^ sign);
    r + f32::from_bits(0.25f32.to_bits() ^ (y.to_bits() & 0x8000_0000))
}

/// VectorAngles (`0x42c5d0`): pitch and yaw in degrees, each in [0, 360).
fn vector_angles(v: V3) -> (f32, f32) {
    if v[1] == 0.0 && v[0] == 0.0 {
        return (if v[2] > 0.0 { 270.0 } else { 90.0 }, 0.0);
    }
    let deg = |t: f32| {
        let d = ((t * f32::from_bits(0x40c9_0fdb)) * 180.0) / f32::from_bits(0x4049_0fdb);
        if d < 0.0 { d + 360.0 } else { d }
    };
    let yaw = deg(atan2_turns(v[1], v[0]));
    let xy = (v[0] * v[0] + v[1] * v[1]).sqrt();
    (deg(atan2_turns(-v[2], xy)), yaw)
}

fn rotation(m: &Mat) -> Mat {
    [[m[0][0], m[0][1], m[0][2], 0.0], [m[1][0], m[1][1], m[1][2], 0.0], [m[2][0], m[2][1], m[2][2], 0.0]]
}

fn transpose(m: &Mat) -> Mat {
    [[m[0][0], m[1][0], m[2][0], 0.0], [m[0][1], m[1][1], m[2][1], 0.0], [m[0][2], m[1][2], m[2][2], 0.0]]
}

/// AngleMatrix: columns forward, left and up for (pitch, yaw, roll) in degrees.
fn angle_matrix(a: V3) -> Mat {
    let (f, r, u) = angle_vectors(a);
    [[f[0], -r[0], u[0], 0.0], [f[1], -r[1], u[1], 0.0], [f[2], -r[2], u[2], 0.0]]
}

/// MatrixAngles: (pitch, yaw, roll) in degrees.
fn matrix_angles(m: &Mat) -> V3 {
    let xy = (m[0][0] * m[0][0] + m[1][0] * m[1][0]).sqrt();
    if xy > 0.001 {
        [(-m[2][0]).atan2(xy).to_degrees(), m[1][0].atan2(m[0][0]).to_degrees(), m[2][1].atan2(m[2][2]).to_degrees()]
    } else {
        [(-m[2][0]).atan2(xy).to_degrees(), (-m[0][1]).atan2(m[1][1]).to_degrees(), 0.0]
    }
}

fn matrix_quaternion(m: &Mat) -> [f32; 4] {
    let t = m[0][0] + m[1][1] + m[2][2];
    if t > 0.0 {
        let s = (t + 1.0).sqrt() * 2.0;
        [(m[2][1] - m[1][2]) / s, (m[0][2] - m[2][0]) / s, (m[1][0] - m[0][1]) / s, 0.25 * s]
    } else if m[0][0] > m[1][1] && m[0][0] > m[2][2] {
        let s = (1.0 + m[0][0] - m[1][1] - m[2][2]).sqrt() * 2.0;
        [0.25 * s, (m[0][1] + m[1][0]) / s, (m[0][2] + m[2][0]) / s, (m[2][1] - m[1][2]) / s]
    } else if m[1][1] > m[2][2] {
        let s = (1.0 + m[1][1] - m[0][0] - m[2][2]).sqrt() * 2.0;
        [(m[0][1] + m[1][0]) / s, 0.25 * s, (m[1][2] + m[2][1]) / s, (m[0][2] - m[2][0]) / s]
    } else {
        let s = (1.0 + m[2][2] - m[0][0] - m[1][1]).sqrt() * 2.0;
        [(m[0][2] + m[2][0]) / s, (m[1][2] + m[2][1]) / s, 0.25 * s, (m[1][0] - m[0][1]) / s]
    }
}

/// QuaternionScale: the same rotation axis, `t` of the angle.
fn quaternion_scale(q: [f32; 4], t: f32) -> [f32; 4] {
    let sinom = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2]).sqrt();
    let sinom = if sinom < 1.0 { sinom } else { 1.0 };
    let sinsom = (sinom.asin() * t).sin();
    let k = sinsom / (sinom + f32::EPSILON);
    let r = 1.0 - sinsom * sinsom;
    let r = if r < 0.0 { 0.0 } else { r }.sqrt();
    [q[0] * k, q[1] * k, q[2] * k, if q[3] < 0.0 { -r } else { r }]
}

/// QuaternionSlerp: aligned to `p`, then spherically between, or linearly when nearly equal.
fn quaternion_slerp(p: [f32; 4], q: [f32; 4], t: f32) -> [f32; 4] {
    let d = p[0] * q[0] + p[1] * q[1] + p[2] * q[2] + p[3] * q[3];
    let q = if d < 0.0 { q.map(|x| -x) } else { q };
    let cosom = p[0] * q[0] + p[1] * q[1] + p[2] * q[2] + p[3] * q[3];
    if 1.0 + cosom > 0.000001 {
        let (sclp, sclq) = if 1.0 - cosom > 0.000001 {
            let omega = cosom.acos();
            let sinom = omega.sin();
            (((1.0 - t) * omega).sin() / sinom, (t * omega).sin() / sinom)
        } else {
            (1.0 - t, t)
        };
        [0, 1, 2, 3].map(|i| sclp * p[i] + sclq * q[i])
    } else {
        let qt = [-q[1], q[0], -q[3], q[2]];
        let (sclp, sclq) = (((1.0 - t) * std::f32::consts::FRAC_PI_2).sin(), (t * std::f32::consts::FRAC_PI_2).sin());
        [sclp * p[0] + sclq * qt[0], sclp * p[1] + sclq * qt[1], sclp * p[2] + sclq * qt[2], qt[3]]
    }
}

fn quaternion_matrix(q: [f32; 4]) -> Mat {
    super::bones::quat_matrix(q, [0.0; 3])
}

/// The ledge frame: AngleMatrix(VectorAngles(forward, up)) at the ledge point, its columns
/// forward, left and up.
fn ledge_frame(at: V3, f: V3) -> Mat {
    let l = [-f[1], f[0], 0.0];
    let n = (l[0] * l[0] + l[1] * l[1]).sqrt();
    let l = if n > 0.0 { [l[0] / n, l[1] / n, 0.0] } else { [0.0, 1.0, 0.0] };
    let u = [f[1] * l[2] - f[2] * l[1], f[2] * l[0] - f[0] * l[2], f[0] * l[1] - f[1] * l[0]];
    [[f[0], l[0], u[0], at[0]], [f[1], l[1], u[1], at[1]], [f[2], l[2], u[2], at[2]]]
}

/// MatrixInvert for a rotation and translation: the transpose, and the translation brought
/// into the other space.
fn invert(m: &Mat) -> Mat {
    let mut o = [[0.0; 4]; 3];
    for (r, row) in o.iter_mut().enumerate() {
        for (c, x) in row[..3].iter_mut().enumerate() {
            *x = m[c][r];
        }
    }
    let t = [m[0][3], m[1][3], m[2][3]];
    for row in &mut o {
        row[3] = -(t[0] * row[0] + t[1] * row[1] + t[2] * row[2]);
    }
    o
}

#[cfg(test)]
mod tests {
    use super::super::bones::IDENTITY;
    use super::super::model::SeqInfo;
    use super::super::AE_CL_VSCRIPT_CALLBACK;
    use super::*;

    fn seq(act: Activity, duration: f32, events: &[(f32, &str)]) -> SeqInfo {
        SeqInfo {
            activity: Some(act),
            weight: 1,
            fade_in: 0.2,
            fade_out: 0.2,
            rate: 1.0 / duration,
            duration,
            named_events: events.iter().map(|(c, o)| (*c, "AE_CL_VSCRIPT_CALLBACK".to_string(), o.to_string())).collect(),
            ..SeqInfo::default()
        }
    }

    fn arms() -> ModelAnims {
        ModelAnims::from_seqs(vec![
            SeqInfo::default(),
            seq(Activity::VM_MANTLE_BELOW, 1.333, &[(0.0, "mantle_extralowmantle")]),
            seq(Activity::VM_MANTLE_LEVEL, 1.067, &[(0.0, "mantle_lowmantle")]),
            seq(Activity::VM_MANTLE_JUMPOFF, 0.2, &[]),
        ])
    }

    /// Pathfinder's arms: fire (nodes 0 to 1), its transition (1 to 2), the looping idle (2),
    /// the lower's transition (2 to 3) and the lower (3 to 0).
    fn grapple_arms() -> ModelAnims {
        let node = |mut s: SeqInfo, entry, exit| {
            (s.entry_node, s.exit_node) = (entry, exit);
            s
        };
        let trans = |d: f32| SeqInfo { activity: None, rate: 1.0 / d, duration: d, ..SeqInfo::default() };
        let mut idle = seq(Activity::VM_GRAPPLE_IDLE, 1.9, &[]);
        idle.flags |= STUDIO_LOOPING;
        ModelAnims::from_seqs(vec![
            node(seq(Activity::VM_GRAPPLE_START, 0.267, &[]), 0, 1),
            node(trans(0.167), 1, 2),
            node(idle, 2, 2),
            node(trans(0.167), 2, 3),
            node(seq(Activity::VM_GRAPPLE_STOP, 0.4, &[]), 3, 0),
        ])
    }

    /// A rope up and to the left of the view draws the aim's pitch up (negative) and its yaw
    /// left (positive) a little each frame, and the animation's end lets go of it.
    #[test]
    fn the_grapple_aims_the_arm_up_the_rope() {
        use crate::assets::model::PoseParam;
        let mut set = grapple_arms();
        let pp = |name: &str, lo, hi| PoseParam { name: name.into(), flags: 0, start: lo, end: hi, looping: 0.0 };
        set.sk.pose_params = vec![pp("traversalYaw", -90.0, 90.0), pp("traversalPitch", -45.0, 45.0)];
        let mut p = Proxy::default();
        let (yaw, pitch) = (0, 1);
        let mut last = [0.5, 0.5];
        for _ in 0..50 {
            p.grapple_aim(&set, [100.0, 100.0, 100.0], [0.0; 3], [0.0; 3], 0.004);
            assert!(p.poses[yaw] > last[0] && p.poses[pitch] < last[1], "{:?}", p.poses);
            last = [p.poses[yaw], p.poses[pitch]];
        }
        let (aim_yaw, aim_pitch) = (p.aim[2], p.aim[0]);
        assert!((0.0..45.0).contains(&aim_yaw) && (-35.3..0.0).contains(&aim_pitch), "{aim_yaw} {aim_pitch}");
        p.grapple(&set, GrappleAnim::End, 1.0);
        assert_eq!(p.aim, [0.0; 4]);
    }

    #[test]
    fn the_grapple_fires_idles_through_its_transitions_and_lowers() {
        let set = grapple_arms();
        let mut p = Proxy::default();
        let mut now = 0.0;
        let mut played = Vec::new();
        let run = |p: &mut Proxy, until: f32, now: &mut f32, played: &mut Vec<u32>| {
            while *now < until {
                p.grapple_think(&set, *now);
                if played.last() != p.main.seq.map(|q| q.0).as_ref() {
                    played.extend(p.main.seq.map(|q| q.0));
                }
                *now += 0.01;
            }
        };
        p.grapple(&set, GrappleAnim::Start, now);
        assert!(p.grappling());
        run(&mut p, 1.0, &mut now, &mut played);
        p.grapple(&set, GrappleAnim::Stop, now);
        run(&mut p, 2.0, &mut now, &mut played);
        assert_eq!(played, [0, 1, 2, 3, 4], "fire, its transition, idle, the lower's transition, lower");
        assert!(!p.grappling() && !p.visible, "the lower ends it");
    }

    #[test]
    fn the_mantle_shows_the_arms_and_follows_the_traversal_cycle() {
        let set = arms();
        let mut p = Proxy::default();
        p.command(&set, Some(Traversal { ty: 4, cycle: 0.1, gun_up: true, ..Traversal::default() }), true, 0.1, 1.0);
        assert!(p.visible);
        let (b, ev) = p.frame(&set, 1.0);
        assert_eq!(b.main, Some((2, 0.1)), "level, at the traversal's cycle");
        assert_eq!(ev, [(AE_CL_VSCRIPT_CALLBACK, "mantle_lowmantle".to_string())]);
        p.command(&set, Some(Traversal { ty: 4, cycle: 0.4, gun_up: false, ..Traversal::default() }), true, 0.1, 1.1);
        let (b, ev) = p.frame(&set, 1.2);
        assert_eq!(b.main, Some((2, 0.4)), "rate 0: held where the traversal put it");
        assert!(ev.is_empty());
        assert_eq!(p.hand_time, 1.1, "a second after the start, until the gun goes down");
        assert!(!p.hands_in(1.05) && p.hands_in(1.1));
        p.command(&set, Some(Traversal { ty: JUMP_OFF, cycle: 0.4, ..Traversal::default() }), true, 0.1, 1.3);
        assert_eq!((p.main.seq, p.main.rate), (Some(super::super::Sequence(3)), 2.0), "the jump-off fits its 0.2 s into the 0.1 s jump");
        let (b, _) = p.frame(&set, 1.35);
        assert!((b.main.unwrap().1 - 0.5).abs() < 1e-5, "from cycle 0");
        p.command(&set, None, true, 0.1, 1.4);
        assert!(!p.hands_in(1.4));
        assert!(!p.visible && p.frame(&set, 1.4).0.main.is_none());
    }

    #[test]
    fn a_climb_shows_the_arms_climbing_while_the_player_rises() {
        let mut climb = seq(Activity(0), 0.5, &[]);
        (climb.label, climb.activity, climb.flags) = (WALL_CLIMB.to_string(), None, STUDIO_LOOPING);
        climb.named_events = vec![(0.0, "AE_CL_PLAYSOUND_FOR_TYPE".to_string(), "effort_climb_1p".to_string())];
        let set = ModelAnims::from_seqs(vec![SeqInfo::default(), seq(Activity::VM_MANTLE_BELOW, 1.333, &[]), climb]);
        let mut p = Proxy::default();
        let wall = Climb { normal: [-1.0, 0.0, 0.0], rising: false };
        p.climb(&set, Some(wall), 1.0);
        assert!(p.climbing() && !p.hands_in(1.0), "drawn as the climb's arms, not a mantle's hands");
        let (b, ev) = p.frame(&set, 1.25);
        assert_eq!(b.main, Some((2, 0.5)), "the first call plays the climb from its start");
        assert_eq!(ev, [(super::super::AE_CL_PLAYSOUND_FOR_TYPE, "effort_climb_1p".to_string())]);
        p.climb(&set, Some(wall), 1.25);
        assert_eq!(p.frame(&set, 1.5).0.main, Some((2, 0.5)), "held while not rising");
        p.climb(&set, Some(Climb { rising: true, ..wall }), 1.5);
        assert_eq!(p.frame(&set, 1.625).0.main, Some((2, 0.75)), "on from where it was held");
        p.command(&set, Some(Traversal { ty: 3, ..Traversal::default() }), false, 0.1, 1.8);
        p.climb(&set, None, 1.8);
        assert!(p.visible && !p.climbing(), "a mantle out of the climb keeps the arms shown");
        p.command(&set, None, false, 0.1, 2.0);
        assert!(!p.visible);
        p.climb(&set, Some(wall), 3.0);
        p.climb(&set, None, 3.1);
        assert!(!p.visible, "a climb's end hides them");
    }

    #[test]
    fn the_climbing_arms_slide_in_and_carry_the_view() {
        let mut p = Proxy { wall: Some([-1.0, 0.0, 0.0]), ..Proxy::default() };
        let t = p.climb_place([0.0, 0.0, 10.0], [0.0; 3], 60.0);
        assert_eq!(t, [[1.0, -0.0, 0.0, -50.0], [0.0, 1.0, 0.0, 0.0], [-0.0, 0.0, 1.0, -50.0]], "facing the wall, all the way back and down");
        p.slide_frame(true, 0.5);
        p.slide_frame(true, 0.25);
        assert_eq!(p.slide, 0.5, "only ever further in while the view holds them");
        p.slide_frame(false, 0.5);
        assert_eq!(p.slide, 0.0);
        let mut camera = IDENTITY;
        camera[2][3] = 4.0;
        let eye = [0.0, 0.0, 60.0];
        assert_eq!(p.climb_view(&camera, &IDENTITY, true, 0.0, eye, [0.0; 3]), (eye, [0.0; 3]), "not blended in yet");
        let (o, a) = p.climb_view(&camera, &IDENTITY, true, 0.5, eye, [0.0; 3]);
        assert_eq!((o, a), ([0.0, 0.0, 62.0], [0.0; 3]), "half way to the camera's lift");
    }

    #[test]
    fn the_arms_sit_on_the_ledge_once_the_hands_are_in() {
        let path = Traversal { ledge: [100.0, 0.0, 50.0], forward: [0.0, 1.0, 0.0], ..Traversal::default() };
        let p = Proxy { visible: true, hand_time: 2.0, path, ..Proxy::default() };
        let t = p.place(&IDENTITY, 1.0, [1.0, 0.0, 0.0], 3.0);
        assert_eq!(t, [[0.0, -1.0, 0.0, 100.0], [1.0, 0.0, 0.0, 0.0], [0.0, 0.0, 1.0, 50.0]], "ref on the ledge, facing along it");
        let held = p.place(&IDENTITY, 1.0, [1.0, 0.0, 0.0], 2.0);
        assert_eq!([held[0][3], held[1][3], held[2][3]], [84.0, 8.0, 34.0], "16 back at the hand time");
    }

    #[test]
    fn the_view_moves_onto_the_camera_and_eases_back_out() {
        let set = arms();
        let mut p = Proxy::default();
        let t = Traversal { ty: 4, ledge: [0.0, 0.0, 50.0], forward: [1.0, 0.0, 0.0], start_time: 1.0, mid: [0.0, 0.0, 50.0], end: [100.0, 0.0, 50.0], ..Traversal::default() };
        p.command(&set, Some(t), true, 0.1, 1.0);
        let pinned = p.pinned(&IDENTITY, 1.0);
        let mut camera = IDENTITY;
        camera[2][3] = 20.0;
        let eye = [-20.0, 0.0, 40.0];
        let (o, a) = p.view(&set, &pinned, &camera, &IDENTITY, 36.0, eye, [10.0, 0.0, 0.0], 1.0);
        assert_eq!((o, a.map(|x| (x * 1000.0).round() / 1000.0)), (eye, [10.0, 0.0, 0.0]), "from the eye, looking where the player looks");
        let (o, _) = p.view(&set, &pinned, &camera, &IDENTITY, 36.0, eye, [10.0, 0.0, 0.0], 1.2);
        assert_eq!(o, [0.0, 0.0, 70.0], "on the camera once lerped in");
        p.command(&set, None, true, 0.1, 1.3);
        let (o, _) = p.view(&set, &pinned, &camera, &IDENTITY, 36.0, eye, [0.0; 3], 1.3);
        assert!((o[0] - -10.0).abs() < 1e-3 && (o[2] - 55.0).abs() < 1e-3, "half eased out: {o:?}");
    }
}
