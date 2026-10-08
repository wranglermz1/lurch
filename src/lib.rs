// Float expressions copy the oracle's own: `(a - a) + k * a` keeps its sign of zero,
// `!(a < b)` keeps its answer for NaN, and `max(0).min(1)` is maxss/minss, which a NaN
// passes through differently from clamp. Rewriting any of them changes results.
#![allow(clippy::eq_op, clippy::neg_cmp_op_on_partial_ord, clippy::manual_clamp, clippy::neg_multiply)]

pub mod anim;
pub mod ltr;
pub mod oracle;
pub mod replay;
pub mod sim;
pub mod sound;
pub mod testmap;
pub mod world;
