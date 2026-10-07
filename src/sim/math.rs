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

/// 2^(j/64), correctly rounded.
const EXP2_64THS: [u64; 64] = [
    0x3ff0000000000000, 0x3ff02c9a3e778061, 0x3ff059b0d3158574, 0x3ff0874518759bc8,
    0x3ff0b5586cf9890f, 0x3ff0e3ec32d3d1a2, 0x3ff11301d0125b51, 0x3ff1429aaea92de0,
    0x3ff172b83c7d517b, 0x3ff1a35beb6fcb75, 0x3ff1d4873168b9aa, 0x3ff2063b88628cd6,
    0x3ff2387a6e756238, 0x3ff26b4565e27cdd, 0x3ff29e9df51fdee1, 0x3ff2d285a6e4030b,
    0x3ff306fe0a31b715, 0x3ff33c08b26416ff, 0x3ff371a7373aa9cb, 0x3ff3a7db34e59ff7,
    0x3ff3dea64c123422, 0x3ff4160a21f72e2a, 0x3ff44e086061892d, 0x3ff486a2b5c13cd0,
    0x3ff4bfdad5362a27, 0x3ff4f9b2769d2ca7, 0x3ff5342b569d4f82, 0x3ff56f4736b527da,
    0x3ff5ab07dd485429, 0x3ff5e76f15ad2148, 0x3ff6247eb03a5585, 0x3ff6623882552225,
    0x3ff6a09e667f3bcd, 0x3ff6dfb23c651a2f, 0x3ff71f75e8ec5f74, 0x3ff75feb564267c9,
    0x3ff7a11473eb0187, 0x3ff7e2f336cf4e62, 0x3ff82589994cce13, 0x3ff868d99b4492ed,
    0x3ff8ace5422aa0db, 0x3ff8f1ae99157736, 0x3ff93737b0cdc5e5, 0x3ff97d829fde4e50,
    0x3ff9c49182a3f090, 0x3ffa0c667b5de565, 0x3ffa5503b23e255d, 0x3ffa9e6b5579fdbf,
    0x3ffae89f995ad3ad, 0x3ffb33a2b84f15fb, 0x3ffb7f76f2fb5e47, 0x3ffbcc1e904bc1d2,
    0x3ffc199bdd85529c, 0x3ffc67f12e57d14b, 0x3ffcb720dcef9069, 0x3ffd072d4a07897c,
    0x3ffd5818dcfba487, 0x3ffda9e603db3285, 0x3ffdfc97337b9b5f, 0x3ffe502ee78b3ff6,
    0x3ffea4afa2a490da, 0x3ffefa1bee615a27, 0x3fff50765b6e4540, 0x3fffa7c1819e90d8,
];

/// expf as the engine's CRT computes it on a CPU with FMA: x = (n/64)·ln 2 + r, then
/// 2^(n/64)·(1 + r + r²/2 + r³/6) in fused double steps. The platform's expf rounds
/// differently often enough to break exact replay. Arguments past the CRT's fast path
/// (|x| > 88) fall back to the platform's.
pub fn crt_expf(x: f32) -> f32 {
    let d = x as f64;
    let t = d * f64::from_bits(0x4057_1547_652b_82fe);
    if !(-9600.0..8192.0).contains(&t) {
        return x.exp();
    }
    let n = t.round_ties_even() as i32;
    let r = (-(n as f64)).mul_add(f64::from_bits(0x3f86_2e42_fefa_39ef), d);
    let poly = r.mul_add(1.0 / 6.0, 0.5);
    let p = (r * r).mul_add(poly, r);
    let j = n & 63;
    let tj = f64::from_bits(EXP2_64THS[j as usize]);
    let scale = f64::from_bits(((((n - j) >> 6) + 1023) as u64) << 52);
    (p.mul_add(tj, tj) * scale) as f32
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
pub const HALF_PI: f32 = f32::from_bits(0x3fc9_0fdb);

fn reduce(r: f32) -> (i32, f32) {
    let q = (r * TWO_OVER_PI).round_ties_even() as i32;
    let qf = q as f32;
    (q, ((r - P1 * qf) - P2 * qf) - P3 * qf)
}

fn sin_poly(x: f32, x2: f32) -> f32 {
    ((S3 * x2 + S2) * x2 + S1) * (x2 * x) + x
}

fn cos_poly(x2: f32) -> f32 {
    (((C4 * x2 + C3) * x2 + C2) * x2 + -0.5) * x2 + 1.0
}

fn flip(v: f32, q: i32) -> f32 {
    f32::from_bits(v.to_bits() ^ (((q & 2) as u32) << 30))
}

/// The engine's own sine and cosine (four lanes at once in the oracle): Cody-Waite
/// reduction by pi/2 in three parts, then short minimax polynomials. Matching it, rather than
/// the platform's sin/cos, is what keeps directions exact to the last bit.
pub fn sin_cos_deg(deg: f32) -> (f32, f32) {
    let (q, x) = reduce(deg * DEG2RAD);
    let x2 = x * x;
    let (sin, cos) = (sin_poly(x, x2), cos_poly(x2));
    let (s, c) = if q & 1 == 0 { (sin, cos) } else { (cos, sin) };
    (flip(s, q), flip(c, q + 1))
}

/// The same sine of radians, as the code that wants one lane computes it. A cosine there
/// is this sine at `x + HALF_PI`, rounded to f32 first.
pub fn engine_sin(r: f32) -> f32 {
    let (q, x) = reduce(r);
    let x2 = x * x;
    flip(if q & 1 == 0 { sin_poly(x, x2) } else { cos_poly(x2) }, q)
}

pub fn engine_cos(r: f32) -> f32 {
    engine_sin(r + HALF_PI)
}

pub fn deg2rad(deg: f32) -> f32 {
    deg * DEG2RAD
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
