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

/// powf as the engine's CRT computes it: correctly rounded from double.
pub fn pow(x: f32, y: f32) -> f32 {
    (x as f64).powf(y as f64) as f32
}

pub fn max_abs(a: V3) -> f32 {
    a[0].abs().max(a[1].abs()).max(a[2].abs())
}

/// The game's "is this up vector the world's up" test: -up.z <= -0.999 in double.
pub fn is_world_up(up: V3) -> bool {
    ((up[0] * 0.0 + up[1] * 0.0) - up[2] * 1.0) as f64 <= -0.999
}

/// The engine's own sine and cosine (four lanes at once in the oracle): Cody-Waite
/// reduction by pi/2 in three parts, then short minimax polynomials. Matching it, rather than
/// the platform's sin/cos, is what keeps directions exact to the last bit.
pub fn sin_cos_deg(deg: f32) -> (f32, f32) {
    const DEG2RAD: f32 = f32::from_bits(0x3c8e_fa35);
    const TWO_OVER_PI: f32 = f32::from_bits(0x3f22_f983);
    const P1: f32 = f32::from_bits(0x3fc9_1000);
    const P2: f32 = f32::from_bits(0xb695_7000);
    const P3: f32 = f32::from_bits(0xb06f_4b9e);
    const C4: f32 = f32::from_bits(0x37cf_14c2);
    const C3: f32 = f32::from_bits(0xbab6_0b22);
    const C2: f32 = f32::from_bits(0x3d2a_aaa9);
    const S3: f32 = f32::from_bits(0xb94d_6102);
    const S2: f32 = f32::from_bits(0x3c08_85d2);
    const S1: f32 = f32::from_bits(0xbe2a_aaa8);
    let r = deg * DEG2RAD;
    let q = (r * TWO_OVER_PI).round_ties_even() as i32;
    let qf = q as f32;
    let x = ((r - P1 * qf) - P2 * qf) - P3 * qf;
    let x2 = x * x;
    let cos = (((C4 * x2 + C3) * x2 + C2) * x2 + -0.5) * x2 + 1.0;
    let sin = ((S3 * x2 + S2) * x2 + S1) * (x2 * x) + x;
    let (s, c) = if q & 1 == 0 { (sin, cos) } else { (cos, sin) };
    let s = f32::from_bits(s.to_bits() ^ (((q & 2) as u32) << 30));
    let c = f32::from_bits(c.to_bits() ^ ((((q + 1) & 2) as u32) << 30));
    (s, c)
}

pub fn angle_vectors(angles: V3) -> (V3, V3, V3) {
    let (sp, cp) = sin_cos_deg(angles[0]);
    let (sy, cy) = sin_cos_deg(angles[1]);
    let (sr, cr) = sin_cos_deg(angles[2]);
    let forward = [cy * cp, cp * sy, -sp];
    let srsp = sr * sp;
    let right = [cr * sy - srsp * cy, -cr * cy - srsp * sy, -sr * cp];
    let crsp = cr * sp;
    let up = [sr * sy + crsp * cy, crsp * sy - cy * sr, cr * cp];
    (forward, right, up)
}
