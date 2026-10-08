---
paths:
  - "src/**/*.rs"
  - "tests/**/*.rs"
---

# Rust

- `src/sim/` is movement: it runs where the oracle's ProcessMovement runs, and the replay
  gates check it bit for bit. Keep client code (sound, animation, camera) out of it. Where
  the oracle calls out from movement, raise a `Cue` (`src/sim/cue.rs`) instead.
- `src/oracle.rs` holds the oracle build's layout and nothing that moves the player.
  Player, move-data and settings offsets live there and in `docs/oracle.md`, nowhere else.
- A new setting is a field in `sim::Settings`, read in `oracle::settings` at its blob
  offset. A ConVar is a constant named after it in the module that reads it, with the
  value read from the image (`SV_GRAVITY` in `src/oracle.rs`, `MAX_BODY_YAW` in
  `src/anim/body.rs`).
- What changes the player between calls from outside movement (FinishMove, networked
  copies, server updates) belongs in `between_calls` or `network_update` in
  `src/replay.rs`, never in the simulation. RunCommand's pre-think is movement:
  `Move::pre_think`.
- `src/lib.rs` allows `clippy::eq_op` and `clippy::neg_cmp_op_on_partial_ord` for the
  oracle's float forms. For another lint a faithful form trips, add a scoped `#[allow]`
  with a one-line why, as `clamp01` in `src/sim/sprint.rs` does.
- Name tests for what holds: `recordings_replay_exactly`, `cues_fall_where_the_oracle_acted`.
- A doc comment on a module or public item says what it is in the game's terms. Keep
  doc comments to the items that need one.
