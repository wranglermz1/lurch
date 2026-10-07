pub type V3 = [f32; 3];

pub const ZERO: V3 = [0.0; 3];
pub const TINY: f32 = 1.084_202_2e-19;

pub fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub fn scale(a: V3, k: f32) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}

pub fn mad(a: V3, b: V3, k: f32) -> V3 {
    [a[0] + b[0] * k, a[1] + b[1] * k, a[2] + b[2] * k]
}

pub fn neg(a: V3) -> V3 {
    [-a[0], -a[1], -a[2]]
}

pub fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

pub fn len2(a: V3) -> f32 {
    dot(a, a)
}

pub fn len(a: V3) -> f32 {
    dot(a, a).sqrt()
}

pub fn normalize(a: V3) -> V3 {
    scale(a, 1.0 / len(a).max(TINY))
}

pub fn reject(v: V3, n: V3) -> V3 {
    sub(v, scale(n, dot(n, v)))
}

pub fn max_abs(a: V3) -> f32 {
    a[0].abs().max(a[1].abs()).max(a[2].abs())
}

/// The game's "is this up vector the world's up" test: -up.z <= -0.999 in double.
pub fn is_world_up(up: V3) -> bool {
    ((up[0] * 0.0 + up[1] * 0.0) - up[2] * 1.0) as f64 <= -0.999
}

pub fn angle_vectors(angles: V3) -> (V3, V3, V3) {
    let (sp, cp) = angles[0].to_radians().sin_cos();
    let (sy, cy) = angles[1].to_radians().sin_cos();
    let (sr, cr) = angles[2].to_radians().sin_cos();
    let forward = [cp * cy, cp * sy, -sp];
    let right = [-sr * sp * cy + cr * sy, -sr * sp * sy - cr * cy, -sr * cp];
    let up = [cr * sp * cy + sr * sy, cr * sp * sy - sr * cy, cr * cp];
    (forward, right, up)
}
