// Float expressions copy the oracle's own: `(a - a) + k * a` keeps its sign of zero, and
// `!(a < b)` keeps its answer for NaN. Rewriting either changes results.
#![allow(clippy::eq_op, clippy::neg_cmp_op_on_partial_ord)]

pub mod anim;
pub mod ltr;
pub mod oracle;
pub mod replay;
pub mod sim;
pub mod sound;
pub mod world;
