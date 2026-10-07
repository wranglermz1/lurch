use crate::trace::{Cmd, FLAG_SPRINT, Settings, State};

type V3 = [f32; 3];

const TINY: f32 = 1.084_202_2e-19;
const SPRINT_INPUT_SCALE: f32 = 1.25;
const MIN_GROUND_SPEED: f32 = 1.0;
const FLOOR_NORMAL_Z: f64 = 0.999;
const FRICTION_DECEL_FALLBACK: f32 = 0.6;

fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale(a: V3, k: f32) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}

fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn len(a: V3) -> f32 {
    dot(a, a).sqrt()
}

fn normalize(a: V3) -> V3 {
    scale(a, 1.0 / len(a).max(TINY))
}

fn reject(v: V3, n: V3) -> V3 {
    sub(v, scale(n, dot(n, v)))
}

fn forward_right(angles: V3) -> (V3, V3) {
    let (sp, cp) = angles[0].to_radians().sin_cos();
    let (sy, cy) = angles[1].to_radians().sin_cos();
    let (sr, cr) = angles[2].to_radians().sin_cos();
    let forward = [cp * cy, cp * sy, -sp];
    let right = [-sr * sp * cy + cr * sy, -sr * sp * sy - cr * cy, -sr * cp];
    (forward, right)
}

pub fn max_speed(s: &Settings, flags: u32) -> f32 {
    let stance = if flags & FLAG_SPRINT != 0 { s.sprint_speed } else { s.speed };
    s.speed_scale * stance * s.weapon_speed_scale
}

fn ramp(s: &Settings, curtime: f32) -> f32 {
    if curtime >= s.friction_ramp_end {
        return 1.0;
    }
    let start = s.friction_ramp_end - 0.5;
    if curtime <= start { 0.0 } else { (curtime - start) * 2.0 }
}

fn half_gravity(s: &Settings, v: &mut V3, t: f32) {
    let g = if s.gravity == 0.0 { 1.0 } else { s.gravity };
    let k = if s.gravity_scale * g != 0.0 { s.gravity_scale * g } else { 1.0 };
    let dv = ((k * s.sv_gravity) as f64 * 0.5 * t as f64) as f32;
    v[2] += -dv;
}

fn ground_accel(s: &Settings, v: V3) -> f32 {
    let speed2 = v[0] * v[0] + v[1] * v[1];
    let fast = s.speed_scale * s.speed - 1.0;
    let a = if s.sprint_acceleration >= 0.0 && fast * fast < speed2 {
        s.sprint_acceleration
    } else {
        let low = s.speed_scale * s.low_speed;
        if low * low <= speed2 || s.low_acceleration < 0.0 { s.acceleration } else { s.low_acceleration }
    };
    a * s.accel_scale * s.speed_scale
}

fn friction(s: &Settings, cmd: &Cmd, n: V3, v: V3, wishdir: V3, wishspeed: f32, dt: f32) -> V3 {
    let mut keep = scale(n, dot(n, v));
    let mut vt = sub(v, keep);
    let along = dot(vt, wishdir);
    if along > 0.0 {
        let f = (along * wishspeed / dot(vt, vt)).min(1.0) * along;
        let protect = scale(wishdir, f);
        keep = add(keep, protect);
        vt = sub(vt, protect);
    }
    let speed = len(vt);
    let stance_speed = s.speed_scale * s.speed;
    let mut decel = -1.0;
    if stance_speed * stance_speed < dot(v, v) {
        if s.sprint_deceleration >= 0.0 {
            decel = s.sprint_deceleration;
        } else if s.sprint_acceleration >= 0.0 {
            decel = s.sprint_acceleration;
        }
    }
    if decel < 0.0 {
        decel = if s.deceleration >= 0.0 { s.deceleration } else { s.acceleration * FRICTION_DECEL_FALLBACK };
    }
    let drop = ramp(s, cmd.curtime) * decel * s.accel_scale * s.time_scale * s.friction_scale * dt;
    let newspeed = (speed - drop).max(0.0);
    if newspeed < speed { add(keep, scale(vt, newspeed / speed)) } else { v }
}

fn accelerate(s: &Settings, v: V3, wishdir: V3, wishspeed: f32, accel: f32, dt: f32) -> V3 {
    let cap = dot(v, v).max(wishspeed * wishspeed);
    let addspeed = wishspeed - dot(wishdir, v);
    if addspeed <= 0.0 {
        return v;
    }
    let out = add(v, scale(wishdir, addspeed.min(accel * dt * s.time_scale)));
    let l2 = dot(out, out);
    if cap < l2 { scale(out, (cap / l2).sqrt()) } else { out }
}

fn air_accelerate(s: &Settings, v: V3, wishdir: V3, wishspeed: f32, accel: f32, dt: f32) -> V3 {
    let addspeed = wishspeed - dot(wishdir, v);
    if addspeed > 0.0 {
        return add(v, scale(wishdir, addspeed.min(accel * dt)));
    }
    let cap = dot(v, v).max(wishspeed * wishspeed);
    let out = add(v, scale(wishdir, dt * s.extra_air_accel));
    let l2 = dot(out, out);
    if cap < l2 { scale(out, (cap / l2).sqrt()) } else { out }
}

fn ground_wish(s: &Settings, cmd: &Cmd, flags: u32, n: V3) -> V3 {
    let (f, _) = forward_right(cmd.angles);
    let r = normalize(cross(f, n));
    let fwd_dir = cross(n, r);
    let k = if flags & FLAG_SPRINT != 0 { SPRINT_INPUT_SCALE } else { 1.0 };
    let (mut fwd, mut side) = (cmd.forward * k, cmd.side * k);
    let m2 = side * side + fwd * fwd;
    if m2 > 0.0 {
        let m = m2.sqrt();
        let shaped = if m2 <= 1.0 { (2.0 - m) * m } else { 1.0 };
        fwd *= shaped / m;
        side *= shaped / m;
    }
    side *= s.speed_scale_side;
    if fwd < 0.0 {
        fwd *= s.speed_scale_back;
    }
    reject(add(scale(fwd_dir, fwd), scale(r, side)), n)
}

fn air_wish(cmd: &Cmd) -> V3 {
    let (f, r) = forward_right(cmd.angles);
    let flat_f = normalize([f[0], f[1], 0.0]);
    let flat_r = normalize([r[0], r[1], 0.0]);
    let wish = add(scale(flat_f, cmd.forward), scale(flat_r, cmd.side));
    if dot(wish, wish) > 1.0 { normalize(wish) } else { wish }
}

fn walk(s: &Settings, cmd: &Cmd, st: &mut State, n: V3, dt: f32) {
    let mut v = reject(st.velocity, n);
    let wish = ground_wish(s, cmd, st.flags, n);
    let l = len(wish);
    let wishdir = scale(wish, 1.0 / l.max(TINY));
    let wishspeed = l.min(1.0) * max_speed(s, st.flags);
    let accel = ground_accel(s, v);
    v = friction(s, cmd, n, v, wishdir, wishspeed, dt);
    v = accelerate(s, v, wishdir, wishspeed, accel, dt);
    if len(v) < MIN_GROUND_SPEED {
        st.velocity = [0.0; 3];
        return;
    }
    st.velocity = v;
    st.origin = add(st.origin, scale(v, dt));
}

fn air(s: &Settings, cmd: &Cmd, st: &mut State, dt: f32) {
    let wish = air_wish(cmd);
    let l = len(wish);
    let wishdir = scale(wish, 1.0 / l.max(TINY));
    let air_speed = if s.air_strafe_enabled == 0.0 { s.air_speed } else { 0.0 };
    let mut v = st.velocity;
    let speed = (v[1] * v[1] + v[0] * v[0]).sqrt();
    if speed > 0.0 {
        let drop = (speed * s.air_drag + s.air_drag_constant) * ramp(s, cmd.curtime) * speed * dt;
        let k = (speed - drop).max(0.0) / speed;
        v = [v[0] * k, v[1] * k, v[2]];
    }
    let wishspeed = s.weapon_speed_scale * s.speed_scale * (l.min(1.0) * air_speed);
    v = air_accelerate(s, v, wishdir, wishspeed, s.air_acceleration * s.accel_scale, dt);
    st.velocity = v;
    st.origin = add(st.origin, scale(v, dt));
}

pub fn unsupported(s: &Settings, st: &State) -> Option<&'static str> {
    if s.movetype != 2.0 {
        return Some("movetype is not walk");
    }
    if s.stance != 0.0 {
        return Some("stance is not standing");
    }
    if (s.normal_z as f64) < FLOOR_NORMAL_Z {
        return Some("surface normal is not a floor");
    }
    if st.flags & !FLAG_SPRINT != 0 {
        return Some("ducked or sliding");
    }
    None
}

pub fn step(s: &Settings, cmd: &Cmd, input: &State) -> State {
    let mut st = input.clone();
    let dt = cmd.frametime * s.lagged;
    let n = [s.normal_x, s.normal_y, s.normal_z];
    half_gravity(s, &mut st.velocity, dt);
    if st.onground {
        walk(s, cmd, &mut st, n, dt);
    } else {
        air(s, cmd, &mut st, dt);
    }
    half_gravity(s, &mut st.velocity, dt);
    if st.onground {
        st.velocity = reject(st.velocity, n);
    }
    st
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        Settings {
            gravity: 1.0,
            speed_scale: 1.0,
            accel_scale: 1.0,
            time_scale: 1.0,
            friction_scale: 1.0,
            lagged: 1.0,
            air_speed: 60.0,
            air_acceleration: 500.0,
            normal_z: 1.0,
            weapon_speed_scale: 1.0,
            movetype: 2.0,
            low_speed: 90.0,
            speed: 180.0,
            sprint_speed: 300.0,
            acceleration: 1500.0,
            deceleration: 2000.0,
            low_acceleration: 1000.0,
            sprint_acceleration: 1000.0,
            sprint_deceleration: 2500.0,
            speed_scale_side: 0.85,
            speed_scale_back: 0.6,
            gravity_scale: 1.0,
            air_drag: 0.0,
            sv_gravity: 750.0,
            extra_air_accel: 2.0,
            ..Default::default()
        }
    }

    fn ground(v: V3) -> State {
        State { velocity: v, onground: true, ..Default::default() }
    }

    fn cmd(dt: f32, forward: f32, side: f32) -> Cmd {
        Cmd { frametime: dt, forward, side, curtime: 100.0, ..Default::default() }
    }

    fn settle(s: &Settings, c: &Cmd) -> State {
        (0..2000).fold(ground([0.0; 3]), |st, _| step(s, c, &st))
    }

    #[test]
    fn friction_stops_dead_below_the_ground_speed_floor() {
        let out = step(&settings(), &cmd(1.0 / 60.0, 0.0, 0.0), &ground([30.0, 0.0, 0.0]));
        assert_eq!(out.velocity, [0.0; 3]);
    }

    #[test]
    fn extreme_dt_never_flips_velocity() {
        for dt in [0.002857, 0.1] {
            let out = step(&settings(), &cmd(dt, 0.0, 0.0), &ground([250.0, -40.0, 0.0]));
            assert!(out.velocity[0] >= 0.0 && out.velocity[1] <= 0.0, "dt {dt}: {:?}", out.velocity);
        }
    }

    #[test]
    fn diagonal_input_is_clamped_to_max_speed() {
        let s = settings();
        let v = settle(&s, &cmd(1.0 / 60.0, 1.0, 1.0)).velocity;
        assert!(len(v) <= s.speed + 1e-3, "{v:?}");
    }

    #[test]
    fn backward_input_is_scaled_by_speed_scale_back() {
        let s = settings();
        let v = settle(&s, &cmd(1.0 / 60.0, -1.0, 0.0)).velocity;
        assert!((len(v) - s.speed * s.speed_scale_back).abs() < 1e-2, "{v:?}");
    }

    #[test]
    fn one_air_tick_from_rest_falls_by_gravity_times_dt() {
        let dt = 1.0 / 60.0;
        let out = step(&settings(), &cmd(dt, 0.0, 0.0), &State::default());
        assert!((out.velocity[2] + 750.0 * dt).abs() < 1e-4, "{:?}", out.velocity);
    }
}
