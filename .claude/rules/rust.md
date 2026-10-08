---
paths:
  - "src/**/*.rs"
  - "tests/**/*.rs"
---

# Rust

- `src/sim/` is movement: it runs where the oracle's ProcessMovement runs, and the replay
  gates check it bit for bit. Keep sound and animation out of it. Where the oracle calls
  out from movement, raise a `Cue` (`src/sim/cue.rs`) instead. The camera is the exception:
  the view state movement keeps and the view setup's frame update between calls live in
  `src/sim/view.rs`, where the gates check them.
- `src/oracle.rs` holds the oracle build's layout and nothing that moves the player.
  Player, move-data and settings offsets live there and in `docs/oracle.md`, nowhere else.
- A new setting is a field in `sim::Settings`, read in `oracle::settings` at its blob
  offset and given its recorded value in `pilot::settings`. A ConVar is a constant named after it in the module that reads it, with the
  value read from the image (`SV_GRAVITY` in `src/oracle.rs`, `MAX_BODY_YAW` in
  `src/anim/body.rs`).
- What happens to the player between calls belongs in the chain's between-call step in
  `src/replay.rs` (FinishMove, the view frame, pre-think), and what the game changes there
  from outside movement (networked copies, server updates) is taken from the oracle
  (`oracle::adopt`), never modelled in the simulation. RunCommand's pre-think is movement:
  `Player::pre_think`.
- `src/lib.rs` allows `clippy::eq_op`, `clippy::neg_cmp_op_on_partial_ord`,
  `clippy::manual_clamp` and `clippy::neg_multiply` for the oracle's float forms. For
  another lint a faithful form trips, add a scoped `#[allow]` with a one-line why.
- Name tests for what holds: `recordings_replay_exactly`, `cues_fall_where_the_oracle_acted`.
- A doc comment on a module or public item says what it is in the game's terms. Keep
  doc comments to the items that need one.
