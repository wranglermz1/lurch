# Game shell plan (Bevy)

This plan is for the sessions that build the playable game shell, roadmap item 2. It also leaves room for the viewmodel (item 4) and for the two later features the user named, multiplayer and our own models (section 8). A fresh session can follow it from top to bottom.

Every fact carries one of three labels:

- **Verified:** read in the pinned 0.19.1 sources or the lurch sources, or measured in a cloud container.
- **Likely:** inferred but not run.
- **Unverified:** needs the workstation, a recording or a decompile. The step that settles it is named.

A probe compiled the snippets marked **(type-checked)** against bevy 0.19.1 and lurch. The other snippets have not been compiled. When one does not build, read the pinned sources under `~/.cargo/registry/src/*/bevy*-0.19.1/`, including the examples and `_release-content/migration-guides/`. Online Bevy docs default to newer versions.

## 0. Working rules for the executing session

- **Where this plan lives.** `docs/game-shell-plan.md`, reached through roadmap item 2 and the CLAUDE.md Layout section. M0 is done.
- **Branch and commits.** Follow the session's own git instructions (its designated branch). Commit at the end of each milestone, and push when the session's instructions say to.
- **Authority.** The user approved Bevy ("Bevy rust engine is good"). They named multiplayer and 3D models as features for later ("if I want to add more features in the future like multiplayer and models").
  - CLAUDE.md's Non-goals already list both as deferred, not refused.
  - Nothing in M0 to M10 depends on them. Ask the user before starting section 8's work. The frame-loop choices in section 1 stand on their own merits.
- **Before every commit:**
  - Root `cargo test` is green.
  - `git diff --exit-code Cargo.toml Cargo.lock` prints nothing.
  - If CLAUDE.md changed, `test $(wc -l < CLAUDE.md) -le 200` passes.
- **Crates.** No crates besides Bevy. Ask the user before adding anything else, including `bevy_framepace`, `serde` or `clap`.
- **Comments.** CLAUDE.md's no-comments rule applies to `game/`.
- **Notation.** `$S` is the session's scratchpad directory.
- **Pace.** Each milestone is one session step and one commit. A milestone that needs the workstation says so.

## 1. Goal and scope

The goal is a playable first-person practice shell on the test map (`src/testmap.rs`), with movement and camera taken from lurch unchanged:

- **One command per rendered frame.** One `Pilot::step` per command. The command's frametime is the frame's duration.
- **Edges from raw input.** Commands are built from raw input events with per-command button edges, so a scroll-wheel tap strafe works as it does in the game.
- **View once per frame.** `Player::view_frame` runs once per rendered frame, after the frame's commands.
- **Camera.** It comes from lurch's `eye_position`, `camera_angles` and `view_fov`.
- **Map and HUD.** The map is drawn flat-shaded with a debug HUD.

Deferred:

- **Mantle.** Not ported (roadmap item 1, which needs the workstation). On the test-map walls between 96 and 192 units, the climb slows under the ledge and drops back. That is expected and is not a shell bug.
- **Sprint shake and view bob.** Not ported yet (roadmap item 3). They will reach the shell through `Pilot::eye` once lurch has them.
- **Viewmodel and models.** A stand-in viewmodel comes with roadmap item 4. Our own glTF models come later (section 8). Apex models and animations stay out for legal reasons.
- **Multiplayer.** Later (section 8).

**Why the frame loop is shaped this way.** Each of these choices costs a few lines now and pays off offline:

- **Commands are the only input to the simulation.** Scripts, the headless binary and the replay tests then drive the same code that real input drives.
- **Commands are numbered by +1.** The sim needs this: climb probes read `command_number % 4` (`climb.rs:434`) and ground edge probes read `& 1` (`ground.rs:92`).
- **The kick seed is a pure function of the command number.** Scripted runs are then reproducible.
- **The clock advances only by command frametimes.** Pausing is then exact, and a respawn rebases f32 precision.
- **A snapshot is one `clone()`.** That gives the history replay check, and checkpoints later.
- **The step path has no side effects, and there is no fixed tick.** Fidelity needs both.

These are also exactly what a Source-style networked protocol (numbered commands, server state, client replay) needs. When multiplayer comes, the loop does not have to be rewritten.

## 2. Decisions

| Question | Decision |
|---|---|
| Engine | Bevy, pinned exactly: `bevy = "=0.19.1"`. It is the newest stable release (verified). 0.20.0-rc.2 exists; do not move to it |
| Rust | Bevy 0.19.1 needs 1.95 (verified). The game crate declares `rust-version = "1.95"`. The root keeps 1.88. The cloud has 1.97 |
| Layout | A separate package in `game/`, which is its own workspace root (empty `[workspace]` table). It has its own `Cargo.lock` and `game/target`, and depends on `lurch = { path = ".." }`. The root `Cargo.toml` and `Cargo.lock` are not touched (verified: root `cargo test --offline` passes with an empty Cargo cache, and the root lock stays byte-identical) |
| Rejected layout | A root workspace with `default-members = ["."]`. It adds about 380 packages to the root lock, lets the root's rust-version 1.88 steer Bevy's dependency resolution, and makes a fresh-clone `cargo test` need the crates.io index (verified) |
| Names | Directory `game/`, package `lurch-game`, library `lurch_game`, binary `lurch-game` |
| Units | lurch stays in inches on Source axes. The shell converts only at the render boundary: positions are multiplied by 0.0254 into metres and the axes are rotated into Bevy's. Bevy's light units and glTF assume metres |
| Time | One command per `app.update()`. dt comes from `Time<Real>`, clamped. Never use `FixedUpdate`, `Time<Fixed>` or `Time<Virtual>`; Virtual clamps at 250 ms |
| Audio, gamepad, glTF | Left out of the feature list. glTF and its companions arrive together in one later change (section 8) |
| Tests | One integration-test binary, `game/tests/shell/main.rs`. Every Bevy-linked test binary is about 90 MB and relinks on each change |

`game/Cargo.toml`, complete. The probe checked this feature list: all 27 names exist, it resolves to 474 packages, it builds, and it renders under xvfb (verified).

```toml
[package]
name = "lurch-game"
version = "0.1.0"
edition = "2024"
rust-version = "1.95"
publish = false

[workspace]

[lib]
test = false
doctest = false

[[bin]]
name = "lurch-game"
path = "src/main.rs"
test = false

[dependencies]
lurch = { path = ".." }
bevy = { version = "=0.19.1", default-features = false, features = [
  "std", "multi_threaded", "async_executor", "bevy_log", "bevy_asset",
  "bevy_window", "bevy_winit", "x11", "wayland",
  "bevy_render", "bevy_core_pipeline", "bevy_pbr",
  "bevy_camera", "bevy_light", "bevy_mesh", "bevy_color", "bevy_image",
  "png", "tonemapping_luts", "ktx2", "zstd_rust",
  "bevy_ui", "bevy_ui_render", "bevy_text", "default_font",
  "bevy_gizmos", "bevy_gizmos_render",
] }

[profile.dev]
opt-level = 1
debug = "line-tables-only"

[profile.dev.package."*"]
opt-level = 3
debug = false
```

Notes on the feature list:

- **Input.** `bevy_window` already brings keyboard, mouse, touch and gesture input (verified).
- **Screenshots.** `png` is needed to save them.
- **Tonemapping.** `tonemapping_luts` (with `ktx2` and `zstd_rust`) prevents pink output under the default tonemapper.
- **Settle features once.** Changing features later rebuilds nearly everything (about 8 minutes). That is why UI, text and gizmos are in from the start.
- **Wayland.** `wayland` is always on, so the workstation build needs no feature switch. It needs `libwayland-dev` and `libxkbcommon-dev` at build time (section 9).
- **No `dynamic_linking`.** A relink after a one-line change takes 1.8 to 2.1 s with this profile (verified).
- **Integration test.** `tests/shell/main.rs` is discovered automatically as the integration test `shell`. `test = false` on the library and the binary stops Cargo from building two more Bevy-linked test binaries.

**Profiles.** Profiles are read only from a workspace root, so these apply to `game/` alone. lurch is a path dependency outside the game workspace, so `package."*"` builds it at opt-level 3 inside the game. Results do not change with the optimisation level: rustc does not contract FMAs, and an earlier probe got bit-identical results in debug, release and target-cpu=native builds after 10,000 frames (verified). The replay gates are still the root crate's `cargo test`.

**Root edits.** Add `/game/target` to `.gitignore`. Commit `game/Cargo.lock`, since the package builds a binary.

**Where the code lives:**

```
game/
  Cargo.toml  Cargo.lock
  src/main.rs      args into Config, App assembly (windowed or --headless)
  src/lib.rs       Config, FrameIndex, Frame sets, shared messages, common(), plugin exports
  src/args.rs      hand-parsed flags -> Config
  src/binds.rs     Action, bind table, held-input set, wheel modes
  src/look.rs      mouse counts -> view angles (plain Rust)
  src/cmd.rs       CmdPlugin: Paused, pause_input, read_input, make_cmd, CmdQueue, CmdLog, frametime clamp
  src/sim.rs       SimPlugin: Sim, Map, Spots, History/Snapshot, Respawn, run_cmds, pose, without_view
  src/script.rs    ScriptPlugin: parser, inject_script, ScriptFrametime, final_line, script_end
  src/convert.rs   Source -> Bevy positions, rotation, FOV (glam only)
  src/view.rs      ViewPlugin: map mesh, light, sky, camera, body, sync, apply_cursor, shots
  src/hud.rs       HudPlugin: HUD text, crosshair
  src/limiter.rs   Limiter (plain), LimiterPlugin
  scripts/         tour.txt, tapstrafe.txt
  tests/shell/     main.rs: mod input; mod sim; mod script; mod convert; mod play; mod render;
tools/bevy-deps.sh system packages for a cloud session, run on demand
```

**Headless use.** `CmdPlugin`, `SimPlugin`, `ScriptPlugin` and `LimiterPlugin` (without a RenderApp) touch no window or render resources. A test can therefore run `MinimalPlugins + bevy::input::InputPlugin + CmdPlugin + SimPlugin` without a GPU.

## 3. Client input model and lurch changes

The lurch changes are M2 and depend on M1's decompile. They are engine-free, add no dependencies, and are additive. They leave `Pilot::step`, `setup_move` and everything a replay gate runs untouched. Root `cargo test` must stay green and about as fast as now (17 s in debug, measured).

### 3.1 Why M1 comes first

The previous evidence for "a fresh press moves at 1, a tap moves at 0" came from 62,612 matching recorded calls. That evidence is an artefact of how the recordings were made:

- `capture/drive.cpp` stamps kbutton state `0x1` (held, no impulse bits) and `0x4` on release (verified, lines 24-31). So no recorded command ever carried an impulse-down bit, and every recorded forward and side value is -1, 0 or 1.
- The driver's comment says that state `0x5` (down plus impulse-up) makes `KeyState` return 0. That matches Source's `KeyState` rule *impulse-up without impulse-down returns 0*. It makes it likely that R5 keeps Source's whole table.

Source's `KeyState` table (Source 2007, unverified for R5):

| kbutton state | Meaning | Value |
|---|---|---|
| down only (`1`) | held the whole frame | 1.0 |
| down + impulse down (`3`) | pressed this frame, still held | 0.5 |
| impulse down + impulse up (`6`) | pressed and released this frame (a tap) | 0.25 |
| down + impulse down + impulse up (`7`) | released and pressed again | 0.75 |
| impulse up, with or without down (`4`, `5`) | released this frame | 0 |
| nothing (`0`) | up | 0 |

**Button bits.** Source's `CalcButtonBits` sets the command's bit when `state & 3` (down or impulse down). Whether R5 builds the bits before or after `KeyState` clears the impulse bits decides whether a wheel tap sets `IN_FORWARD`. That order is unverified.

M1 settles this table and the bit order by decompiling. M2 then writes `Keys` and `key_state` from the result. If M1 runs out of time, M2 uses Source's table, takes the bits before the impulse clear, and marks the move-value tests provisional. Every test asserts through `key_state(..)` rather than literals, so a later answer changes one function.

### 3.2 `src/sim/buttons.rs` (new; add `pub mod buttons;` in `src/sim/mod.rs`)

These are every bit the sim reads or the client sets (verified against the sim and 62,621 recorded calls):

```rust
pub const IN_JUMP: u32 = 0x2;
pub const IN_DUCK: u32 = 0x4;
pub const IN_FORWARD: u32 = 0x8;
pub const IN_BACK: u32 = 0x10;
pub const IN_LEFT: u32 = 0x80;
pub const IN_RIGHT: u32 = 0x100;
pub const IN_MOVELEFT: u32 = 0x200;
pub const IN_MOVERIGHT: u32 = 0x400;
pub const IN_WALK: u32 = 0x800;
pub const IN_SPEED: u32 = 0x8000;
pub const IN_TOGGLE_DUCK: u32 = 0x400_0000;
pub const IN_DODGE: u32 = 0x1000_0000;
/// Our name. The client sets it on the command where IN_SPEED goes down; the sim never reads it.
pub const IN_SPEED_PRESSED: u32 = 0x2000_0000;
```

`jump.rs:9-14`, `duck.rs:9-10` and `sprint.rs:9-10` lose their own definitions:

- **Public re-exports.** jump.rs gets `pub use super::buttons::{IN_FORWARD, IN_JUMP};`, duck.rs gets `{IN_DUCK, IN_TOGGLE_DUCK}`, sprint.rs gets `IN_SPEED`.
- **Private imports.** A private `use` brings in the rest.
- **Paths kept.** `tests/testmap.rs` imports `lurch::sim::jump::IN_JUMP` and the other public paths. They keep resolving, and the values do not change.

### 3.3 `src/sim/input.rs` (new): the client's kbutton model

```rust
pub const DOWN: u8 = 1;
pub const IMPULSE_DOWN: u8 = 2;
pub const IMPULSE_UP: u8 = 4;

/// What a key adds to a move value this command; the table M1 recorded in docs/oracle.md.
pub fn key_state(state: u8) -> f32;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Keys { down: u32, impulse_down: u32, impulse_up: u32, last: u32 }

impl Keys {
    pub fn press(&mut self, bits: u32);    // new = bits & !down; down |= new; impulse_down |= new
    pub fn release(&mut self, bits: u32);  // gone = bits & down; down &= !gone; impulse_up |= gone
    pub fn release_all(&mut self);         // release(down)
    pub fn state(&self, bit: u32) -> u8;
    pub fn fill(&mut self, cmd: &mut UserCmd);
}
```

`press` on a bit that is already down does nothing, as in Source's `KeyDown`. `release` only affects bits that are down. Several physical inputs bound to one bit are reconciled in the shell (section 4.3), not here.

`fill` writes the command:

- `buttons = down | impulse_down`. This is `CalcButtonBits`, taken before the impulse clear unless M1 finds the other order.
- `IN_SPEED_PRESSED` is added when `IN_SPEED` is in `buttons` and was not in `last`.
- `forward = key_state(FWD) - key_state(BACK)`.
- `side = key_state(MOVERIGHT) - key_state(MOVELEFT)`.
- Then `impulse_down = impulse_up = 0` and `last = buttons`.

`up` and roll stay 0, as in every recording.

Several notches in one frame make one tap (state 6). Taps in consecutive frames set the bit on both commands, but the sim's edge (`buttons & !oldbuttons`) fires only on the first. That is what caps the tap rate by frame rate. Any speed scaling or normalisation M1 finds in `CreateMove` also goes in `fill`.

### 3.4 `src/sim/setup.rs`

```rust
#[derive(Clone, Copy, Debug, Default, PartialEq)]   // PartialEq added
pub struct UserCmd { .. }

/// Ours: the game's MD5 seed is not reproducible, and its kick draws are uniform, so a well-mixed seed behaves the same.
pub fn command_seed(command_number: u32) -> u32;   // view::mix([n, 0, 0]) & 0x7fff_ffff
```

`setup_move` always passes `Some(cmd.random_seed)` (`setup.rs:96`). A seed left at 0 draws the same kick on every jump and landing. The first 10,000 seeds are distinct and have the top bit clear (verified numerically).

The frametime clamp does **not** go into lurch. Its retail constants are not in the oracle image (section 4.4). It moves into lurch only if M1 finds a clamp in the oracle.

### 3.5 `src/sim/pilot.rs`

```rust
#[derive(Clone, Debug)]                     // added
pub struct Pilot { pub player, pub origin, pub velocity, oldbuttons }

impl Pilot {
    pub fn oldbuttons(&self) -> u32;
    pub fn with_oldbuttons(self, oldbuttons: u32) -> Self;
    pub fn eye(&self, s: &Settings, cl_fov_scale: f32, now: f32) -> Eye;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Eye { pub origin: V3, pub angles: V3, pub fov: f32 }

/// Our offline clock, matching the test drive (tests/testmap.rs:31-36), not the game's clock arithmetic.
/// Spawned event times are zero; the longest window measured from them is slide_boost_cooldown (2 s).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Clock(f64);
impl Clock {
    pub const START: f64 = 100.0;
    pub fn now(&self) -> f32;                    // self.0 as f32
    pub fn advance(&mut self, frametime: f32);   // self.0 += frametime as f64
    pub fn secs(&self) -> f64;
    pub fn from_secs(secs: f64) -> Self;
}
impl Default for Clock { /* Clock(START) */ }
```

What each piece is for:

- **`eye`.** It composes `eye_position(s, origin, player.view_angles, now)`, `camera_angles(s, player.view_angles)` and `view_fov(s, cl_fov_scale, now)`. `step` sets `view_angles` to the pitch-clamped command angles (`pilot.rs:257`).
- **`with_oldbuttons`.** It lets a respawn carry the held keys over, so a key held through a respawn makes no new edge. `from_commands` keeps using `Pilot::new`.
- **No `Pilot::frame`.** The shell's loop runs `step` per command and `view_frame` once per frame (section 4.6), and the tests write that order out explicitly.
- **`Clock`.** It is ours. The game's curtime follows neither rule exactly. f32 addition of each frametime matches 895 of 1317 calls in bhop and 41 of 563 in jump, and an f64 accumulator matches 79 and 10 (verified). `secs` and `from_secs` let a snapshot be stored and restored without a later API change.

**Spawn FOV fix (done).** `spawn()` sets `fov_from: 1.0, fov_to: 1.0`. Before, both defaulted to 0, so `view_fov` returned 1 degree at spawn and eased to 70 over 0.25 s. `tests/testmap.rs` `a_spawned_pilot_sees_the_class_fov` covers it. No replay gate reads a spawned pilot's FOV.

### 3.6 `Player` helpers

Add `Player::ducked()` (`flags & 2 != 0`) and `Player::on_ground()` (`ground.is_some()`) in `impl Player` (`mod.rs:333`). They sit alongside the `Move` versions (`mod.rs:399-405`), which stay as they are.

### 3.7 `tests/frame.rs` (new)

It uses no recordings. libtest must report `finished in` under 0.1 s.

- **`spawned_pilot_sees_the_class_fov`.** `Pilot::new(o).eye(&s, 1.0, Clock::default().now()).fov == 70.0`. It must still hold after one `step` and one `view_frame`.
- **`a_cloned_pilot_replays_bit_for_bit`.** Clone after 100 commands on the test map, run both pilots on the same next 200 commands, and compare their `format!("{:?}")` strings.
- **`held_jump_through_with_oldbuttons_does_not_jump`.** On the test map at `wall 48`:
  - `Pilot::new(o).with_oldbuttons(IN_JUMP)` stepping a command with `IN_JUMP` gives `velocity[2] == 0`.
  - Without the seed, the same command jumps (vz > 0; a probe measured 285.33).
- **Keys tests.** Values are asserted through `key_state`:
  - `a_tap_sets_its_bit_for_one_command`. The bit is set and forward is `key_state(IMPULSE_DOWN | IMPULSE_UP)`. On the next command the bit is clear and forward is 0.
  - `a_held_key_reads_press_then_held`. The first command's forward is `key_state(DOWN | IMPULSE_DOWN)`, later ones `key_state(DOWN)`. After a release it is `key_state(IMPULSE_UP)`, then 0. W and S held together give 0.
  - `release_and_repress_in_one_command` gives `key_state(7)`.
  - `taps_in_consecutive_commands_edge_once`. The bit is set on both commands, and `buttons & !previous` has it only on the first.
  - `sprint_press_is_flagged_on_its_first_command_only`.
  - `release_all_releases_every_held_bit`.
- **`command_seeds_are_distinct_with_the_top_bit_clear`.** The first 10,000 numbers.
- **`clock_starts_at_100_and_round_trips`.** Also checks that `advance` matches the f64 accumulator of `tests/testmap.rs:31-36`.

`tests/testmap.rs` compiling unchanged covers the re-exported paths.

## 4. The frame

### 4.1 Shared pieces in `lib.rs`

- **`Config`.** A `Resource` with `Default` holding every flag (section 5). `main` or a test inserts it **before** adding the plugins, and plugins read it in `build`. A plugin that finds none uses `Config::default()`. Tests set fields directly (for example `Config { spot: "wall 1024".into(), fov_scale: 1.0, ..default() }`) and never go through argv.
- **`FrameIndex(u64)`.** Starts at 0 and is incremented by `advance_frame`, the last system in `Last`. Frame F is the F-th `app.update()`, counting from 0.
- **`Frame` system sets.** `#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)] pub enum Frame { Pause, Read, Respawn, Make, Run, Pose, View }`.
  - `configure_sets(PreUpdate, (Frame::Pause, Frame::Read).chain().after(InputSystems))`.
  - `configure_sets(Update, (Frame::Respawn, Frame::Make, Frame::Run, Frame::Pose, Frame::View).chain())`.
  - Every ordering between plugins goes through these sets.
- **Shared messages.** `Respawn` (section 4.7) and `LookReset { yaw }`.
- **`common(app)`.** Every plugin calls it, and it is guarded by `contains_resource::<FrameIndex>()` so it runs once. It:
  - inserts `FrameIndex`, `Paused` and `HudVisible(true)`;
  - registers the messages, plus `app.add_message::<WindowFocused>()` so headless tests can send it. `add_message` is idempotent (verified, `bevy_app sub_app.rs:394`);
  - configures the sets.

### 4.2 Schedule

| Schedule and set | System | Plugin | Does |
|---|---|---|---|
| `First` | Bevy `time_system` | Bevy | `Time<Real>` delta = the previous frame's period, render end to render end. RenderPlugin's `TimeReceiver` stamps the time after the Render schedule (verified) |
| `PreUpdate`, `.before(InputSystems)` | `inject_script` | Script, only in script mode | Writes this frame's scripted `KeyboardInput`, `MouseWheel` and `MouseMotion` messages with `window: Entity::PLACEHOLDER`, so scripts go through the real input path |
| `PreUpdate`, `InputSystems` | Bevy input | Bevy | Fills `ButtonInput` and `AccumulatedMouseMotion` |
| `PreUpdate`, `Frame::Pause` | `pause_input` | Cmd | Escape or `WindowFocused { focused: false }` pauses and releases all input. A left click resumes. Not added in script mode |
| `PreUpdate`, `Frame::Read` | `read_input` | Cmd | Messages go through the binds and the held-input set into `Keys`; wheel taps; look; actions (R, `[`, `]`, F1) |
| `Update`, `Frame::Respawn` | `apply_respawn` | Sim | Pending respawn: new pilot, clock, history and view time (section 4.7) |
| `Update`, `Frame::Make` | `apply_look_reset`, then `make_cmd` | Cmd | Skips the command if paused or dt is 0. Otherwise builds one `UserCmd` and pushes it to `CmdQueue` |
| `Update`, `Frame::Run` | `run_cmds` | Sim | Drains the queue: `step` per command, then `view_frame` once (section 4.6) |
| `Update`, `Frame::Pose` | `pose` | Sim | Sets `Pose(sim.pilot.eye(&s, cfg.fov_scale, sim.view_now))` every frame |
| `Update`, `Frame::View` | `sync_camera`, `sync_body`, `apply_cursor`; `hud` | View; Hud | One-way writes from `Pose` and `Sim` |
| `Last` | `take_shots`, `script_end`, then `advance_frame` | View, Script, lib | Screenshots; script exit; frame counter |
| RenderApp `Render`, `RenderSystems::Cleanup` | `limit` | Limiter | fps cap, windowed with pipelined rendering off |
| `Last` (fallback) | `limit` | Limiter | fps cap when there is no RenderApp or with `--pipelined` |

Sim state is written only in `apply_respawn` and `run_cmds`. Rendering reads `Pose` and `Sim` and never writes back.

One `app.update()` is one rendered frame and one command. This is likely, not verified: winit's Continuous mode runs one update per iteration.

### 4.3 Input to `Keys` (`binds.rs`, `cmd.rs`)

`read_input` reads messages, not `ButtonInput`. `ButtonInput` collapses a press-release-press within one frame and never sees the wheel (verified). The signature below is adapted from a type-checked version; the writers were added after the check:

```rust
fn read_input(mut keys: MessageReader<KeyboardInput>, mut buttons: MessageReader<MouseButtonInput>,
              mut wheel: MessageReader<MouseWheel>, mut lost: MessageReader<KeyboardFocusLost>,
              motion: Res<AccumulatedMouseMotion>, cfg: Res<Config>, paused: Res<Paused>,
              mut b: ResMut<CmdBuilder>, mut respawn: MessageWriter<Respawn>, mut hud: ResMut<HudVisible>) { .. }
```

`CmdBuilder { keys: Keys, held: Held, look: Look, wheel: Wheel, next_number: u32 }`. `next_number` starts at 1.

The rules, applied in this order inside one call:

1. **Script mode** (`cfg.scripted`). Drop any message whose `window` is not `Entity::PLACEHOLDER`, and ignore `KeyboardFocusLost`. Only scripted input drives a script run, so a focus change under xvfb cannot release scripted keys.
2. **Repeats.** Skip `KeyboardInput` with `repeat: true`.
3. **Binds.** `Binds` maps a `KeyCode` or `MouseButton` to an `Action`: `Bits(u32)`, `Respawn`, `PrevSpot`, `NextSpot` or `ToggleHud`.
4. **Press.** Ignored while paused. Otherwise `held.press(input, bits)` returns false if that physical input is already in the set, and the press is then ignored. `Bits(b)` calls `keys.press(b)`. The other actions write `Respawn::Spot(0)`, `Respawn::Spot(-1)`, `Respawn::Spot(1)` or flip `HudVisible`.
5. **Release.** `held.release(input)` returns `None` for an input not in the set, and the release is then ignored. Otherwise `keys.release(b & !held.bits())` releases only the bits no other held input still holds.
6. **Wheel.** Ignored while paused. A tap is `keys.press(b)` followed by `keys.release(b & !held.bits())`. `--wheel-mode` picks how wheel messages become taps:
   - `event` (the default) is one tap per non-zero message. That is Source's rule; R5 is unverified.
   - `notch` is ours. It accumulates `MouseScrollUnit::Line` deltas, resets on a direction flip, and makes one tap per whole notch.
   - `MouseScrollUnit::Pixel` (touchpads) always uses a separate accumulator, with one tap per `PIXELS_PER_NOTCH = 120.0`. That threshold is ours.
   - Normal wheels deliver 1.0 per click, so the two modes agree. They differ only for high-resolution or free-spinning wheels.
7. **Focus loss.** `KeyboardFocusLost` is handled after every key and button message of the frame. `b.release_all()` clears the held set, calls `keys.release_all()` and resets the wheel accumulators.
   - Bevy also sends a synthetic `KeyboardInput` release for every held key (verified). Those inputs are no longer in the set, so the releases are ignored. A set cannot underflow the way a counter can.
   - Bevy sends no mouse-button releases (verified), which `release_all` covers.
8. **Look.** If not paused, `b.look.apply(motion.delta)`.

`Held` is a small `Vec<(Input, u32)>`, where `Input` is `Key(KeyCode)` or `Mouse(MouseButton)`. It has `press`, `release`, `bits` and `clear`.

`pause_input` reads `Res<ButtonInput<KeyCode>>`, `Res<ButtonInput<MouseButton>>` and `MessageReader<WindowFocused>`:

- Escape `just_pressed`, or `focused: false`, sets `Paused(true)` and calls `b.release_all()`.
- Left `just_pressed` while paused sets `Paused(false)`.

`Paused` defaults to false. The windowed binary inserts `Paused(true)` when there is no `--script`, so the game starts on "click to play".

### 4.4 Commands (`make_cmd`)

This is adapted from a type-checked version:

```rust
fn make_cmd(time: Res<Time<Real>>, script: Option<Res<ScriptFrametime>>, paused: Res<Paused>,
            frame: Res<FrameIndex>, mut b: ResMut<CmdBuilder>, mut queue: ResMut<CmdQueue>,
            log: Option<ResMut<CmdLog>>) {
    if paused.0 || time.delta().is_zero() { return; }
    let frametime = match script { Some(f) => f.0, None => clamp_frametime(time.delta_secs_f64()) };
    let n = b.next_number;
    b.next_number += 1;
    let mut cmd = UserCmd { command_number: n, angles: b.look.angles(), frametime, random_seed: command_seed(n), ..default() };
    b.keys.fill(&mut cmd);
    if let Some(mut log) = log { log.0.push((frame.0, cmd)); }
    queue.0.push_back(cmd);
}
```

- **First update.** It has delta 0 in both windowed and headless runs, so frame 0 makes no command. Command n is made on frame n (verified with a probe).
- **Clamp.** In `cmd.rs`: `RETAIL_USERCMD_FRAMETIME_MIN: f32 = 0.002857` and `RETAIL_USERCMD_FRAMETIME_MAX: f32 = 0.1`. Their doc line names them as the retail `usercmd_frametime_*` ConVars, absent from the oracle image (verified). `clamp_frametime(dt: f64) -> f32` returns `(dt as f32).max(MIN).min(MAX)`, so it is never 0 and never NaN.
  - Frametime 0 is unsafe: `sprint_tilt` divides by it (`view.rs:161`), and the NaN becomes a phantom full tilt.
  - A stall becomes one 0.1 s command instead of a huge one.
- **Script frametime.** In script mode, `ScriptFrametime(f32)` holds the exact f32 that `--frametime` asked for (default `SCRIPT_FRAMETIME: f32 = 0.004`, the `DT` of `tests/testmap.rs`). It is parsed by `script::parse_frametime(&str) -> Result<f32, String>`, which the tests also call. `TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(ft as f64))` only keeps Bevy's clocks moving.
  - A Duration has nanosecond resolution and would round the f32 off by a few ulp. That is why the command does not take its frametime from `Time`.
- **`CmdLog`.** It holds `Vec<(u64, UserCmd)>`. It is inserted only by tests and in script or headless mode, so a long play session does not grow it without bound.
- **`CmdQueue(VecDeque<UserCmd>)`.** This is the seam: a script, a file replay or a network source can push commands, and `run_cmds` does not change.

### 4.5 Mouse look (`look.rs`, plain Rust)

`Look { yaw, pitch, sens }` with `apply(delta)`, `angles() -> V3` and `reset(yaw)`.

The convention is Source's (likely), until M1 recovers R5's:

- Degrees per count = 0.022 × `mouse_sensitivity`, retail default 5.
- `yaw -= k·dx` and `pitch += k·dy`, where dx and dy are raw counts from `AccumulatedMouseMotion`.
- Clamp pitch to ±89 in the accumulator. `setup_move` clamps only its own copy, so motion past the clamp would otherwise be lost.
- Wrap yaw to (-180, 180] every frame. Recordings stay in that range, and f32 range reduction degrades for huge yaw.

Never use `CursorMoved`. `MouseMotion` is winit's raw `DeviceEvent::MouseMotion` (verified).

### 4.6 Step, clock, view frame (`run_cmds`, `pose`)

The state is one resource, `Sim { pilot: Pilot, clock: Clock, settings: Settings, view_now: f32 }`. Around it:

- **`Map(CollisionWorld)`.** `lurch::world::World` imported as `CollisionWorld`, to avoid the clash with Bevy's `World`. Built once. It needs `ResMut` only because `Tracer::hull` takes `&mut self`.
- **`Spots { list, current }`.** From `testmap::build().spots`.
- **`History`.** A ring of 1024 `Snapshot { cmd: UserCmd, pilot: Pilot, clock: Clock }`, about 1 KB each.
- **When they exist.** All are inserted in `Plugin::build`, so tests have them without a Startup run. `view_now` starts at `Clock::START as f32`.

The loop:

```rust
let Sim { pilot, clock, settings, view_now } = &mut *sim;
let mut last = None;
while let Some(cmd) = queue.0.pop_front() {
    pilot.step(settings, &cmd, clock.now(), &mut map.0);
    clock.advance(cmd.frametime);
    history.push(Snapshot { cmd, pilot: pilot.clone(), clock: *clock });
    if pilot.origin[2] < KILL_Z { respawn.write(Respawn::Spot(0)); }   // KILL_Z = -2048.0
    last = Some(cmd);
}
if let Some(cmd) = last {
    *view_now = clock.now();
    pilot.player.view_frame(settings, pilot.velocity, *view_now, cmd.frametime);
}
```

- **`view_frame` once per rendered frame.** The game runs the view setup once per frame (docs/camera.md), not once per command. The chained replay gate pairs one `view_frame` with each call only because each recorded call is one frame (`replay.rs:170,201`). With one command per frame the two orders are identical. With several queued commands, `view_frame` must not run N times. It only writes `slide_tilt` and `fov_*`, which movement never reads (verified).
- **View clock.** `view_now` is the clock *after* the advance. The oracle's view clock (gpGlobals+0x24) sits a constant amount behind the post-command clock within a session. Measured over all 43 recordings, that lag ranges from about +0.001 to -0.075 s between sessions. Lag 0 is constant and inside that range. Using the pre-step clock would make the lag equal each frame's dt. The view clock is ours, and roadmap item 3's view-setup hook can add a lag later.
- **Snapshots.** They are taken after `step` and before the frame's `view_frame`, so they hold movement state. `without_view(&Pilot) -> Pilot` clones and resets the five view-only fields (`slide_tilt`, `fov_start`, `fov_duration`, `fov_from`, `fov_to`). Every replay comparison uses it.
- **Precision.** f32 time resolution is 2 ms after about 4.5 h of play. Every respawn rebases it.

### 4.7 Respawn

```rust
#[derive(Message)]
pub enum Respawn { Spot(isize), Named(String), At { origin: V3, yaw: f32 } }
```

| Variant | Effect | Sent by |
|---|---|---|
| `Spot(k)` | Moves `Spots.current` by k, wrapping | R (0), `[` (-1), `]` (1), the kill plane (0) |
| `Named(n)` | Selects spot n | Script `spot NAME` |
| `At { origin, yaw }` | Teleports to that origin; the current spot stays as it was | Tests |

`apply_respawn` applies the last pending message:

```rust
let old = sim.pilot.oldbuttons();
sim.pilot = Pilot::new(origin).with_oldbuttons(old);
sim.pilot.player.view_angles = [0.0, yaw, 0.0];
sim.clock = Clock::default();
sim.view_now = sim.clock.now();
history.clear();
look_reset.write(LookReset { yaw });
```

- **Held keys.** They make no new edge after a respawn, because `oldbuttons` carries over. Without this, holding Space while pressing R would jump.
- **Command numbers.** They keep counting.
- **Start of the run.** The initial pilot at startup goes through the same helper, at the `Config.spot` spot with oldbuttons 0.
- **Floor spots only.** A spot in mid-air holds the origin for the first command (verified). Every test-map spot stands on a floor.

### 4.8 Camera: coordinates, rotation, FOV (`convert.rs`)

Source is X forward, Y left, Z up, right-handed, in inches. Bevy is X right, Y up, -Z forward.

The mapping is the proper rotation `M = [[0,-1,0],[0,0,1],[-1,0,0]]`. Its determinant is +1, so winding is kept. It sends Source forward to -Z, left to -X and up to +Y. Source zero angles are therefore Bevy's identity rotation (verified numerically over 10,000 angles, worst error 2.2e-16).

The functions (type-checked):

```rust
pub const INCH: f32 = 0.0254;
pub fn dir(v: V3) -> Vec3 { Vec3::new(-v[1], v[2], -v[0]) }
pub fn pos(v: V3) -> Vec3 { dir(v) * INCH }
pub fn rot(a: V3) -> Quat {   // a = [pitch, yaw, roll] in degrees, positive pitch looks down
    Quat::from_euler(EulerRot::YXZ, a[1].to_radians(), (-a[0]).to_radians(), (-a[2]).to_radians())
}
pub fn vfov(hfov43_deg: f32) -> f32 { 2.0 * (0.75 * (hfov43_deg.to_radians() * 0.5).tan()).atan() }
```

`sync_camera` sets:

```rust
*t = Transform::from_translation(pos(pose.0.origin)).with_rotation(rot(pose.0.angles));
if let Projection::Perspective(p) = &mut *proj { p.fov = vfov(pose.0.fov); }
```

**Projection.** Spawn the camera with `PerspectiveProjection { near: 0.05, far: 2000.0, .. }`. The near plane is about 2 in, because the eye can sit about 7 in from a wall (16 hull minus the 9.2 pitch offset). Bevy keeps `aspect_ratio` up to date itself. Use this one mapping everywhere; `(x, z, -y)` is a different rotation.

**FOV.** `view_fov` returns degrees, read as a horizontal FOV defined at 4:3 and widened with aspect (Hor+). Bevy's `fov` is vertical, so the vertical FOV never depends on window aspect: 70 gives 55.41°, 90 gives 73.74° and 110 gives 93.93°. That R5 keeps Source's 4:3 definition is likely, not recovered (Q2).

## 5. World, HUD, binds, flags, window and timing

### Map mesh (`view.rs`)

It is built from `testmap::build().tris`. The ViewPlugin calls `build()` itself, since it is deterministic and cheap. The mesh is one non-indexed `TriangleList` (type-checked):

```rust
Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)   // pos(v).to_array(), 3 per tri
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)         // Color::srgb(..).to_linear().to_f32_array()
    .with_computed_flat_normals()
```

**Material.** `StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 1.0, cull_mode: None, double_sided: true, .. }`. Both culling settings are required:

- The test map's winding is inconsistent: in `cuboid`, the y1 and x0 faces point inward.
- Collision does not care about winding.
- With `double_sided`, the inward faces light from the correct side (verified).
- Do not rewind the map in lurch, because reordering vertices could shift float results.

**Vertex colours** carry the readability. The shader reads them as linear and multiplies them into `base_color` (verified, `pbr_fragment.wgsl`), so convert from sRGB as shown above:

- **Floor checker.** The first quad `build()` pushes is the ±16384 floor (the first two triangles). For rendering only, replace it with 256 in tiles in two greys, srgb 0.42 and 0.50. Collision keeps the original two triangles. A uniform floor gives no sense of speed.
- **Tops.** Light grey (srgb 0.75).
- **Walls by axis.** Walls facing ±X are warm (srgb 0.80, 0.64, 0.50). Walls facing ±Y are cool (srgb 0.50, 0.62, 0.80). Tint by the axis of the Source normal, not its sign.

### Light and sky

- **Sun.** `DirectionalLight { shadow_maps_enabled: false, ..default() }` with `Transform::default().looking_to(Vec3::new(-0.4, -1.0, -0.3), Vec3::Y)`. It is on all render layers the cameras use (section 8).
- **Ambient.** `GlobalAmbientLight { brightness: 2000.0, ..default() }`. With the default of 80, faces turned away from the sun render at about (32,32,32) (measured). Tune within 1500 to 3000 from the M7 screenshots.
- **Sky.** `ClearColor(Color::srgb(0.55, 0.70, 0.85))`. It renders as exactly (140,178,217) on lavapipe (measured).
- **Shadows.** Off. If they are added, tune `CascadeShadowConfigBuilder` for a 400 m floor.

### Body

A `Body` entity: a `Cuboid::default()` mesh with its own grey material, on `RenderLayers::layer(2)`, which the first-person camera does not render. `sync_body` writes its Transform from `Sim` on the same one-way path as `sync_camera`:

- Translation: `pos` of the hull centre.
- Scale: `dir(hull_max - hull_min).abs() * INCH`.
- Hull: the duck hull when `player.ducked()`.

Later a third-person debug camera, our own glTF pilot (as a child of `Body`) and remote players all reuse this entity.

### Spawn and reset

- **Spots.** `TestMap::spots` in build order: the eight walls 48 to 1024, the six steps 8 to 48, the six ramps 15 to 60, `hill`, `crates`. All face yaw 0.
- **Start.** `--spot NAME`, default `wall 48`.
- **Keys.** R respawns at the current spot, and `[` and `]` cycle spots.
- **Kill plane.** z < -2048 respawns at the current spot.

### HUD (`hud.rs`, `HudPlugin`)

A separate plugin, so the render test leaves it out. A `Text` at the top left (type-checked) uses `TextFont::from_font_size(16.0)` and `Node { position_type: PositionType::Absolute, top: px(8), left: px(8), .. }`. It is updated every frame and hidden when `HudVisible` is false (F1):

```
speed 299 u/s  27.3 km/h   vz -120
ground | air | wall   slide  sprint  duck
fov 89  sens 5.0  fov-scale 1.272  wheel event
fps 240  ft 4.17 ms  cmd 12345  spot wall 128
present AutoNoVsync  adapter <name>
click to play            (only while paused)
```

- Speed is horizontal, `hypot(vx, vy)`, and km/h = u/s × 0.09144.
- `wall` is `ground.is_some() && !is_world_up(up)`, the rule `tests/testmap.rs` uses.
- fps is averaged over 0.5 s.
- A 4 px crosshair dot `Node` sits at the centre.

### Default binds

These are the Apex PC defaults as commonly documented. They are unverified against R5Reloaded's default config (Q5).

| Input | Action | Source |
|---|---|---|
| W / S / A / D | IN_FORWARD / IN_BACK / IN_MOVELEFT / IN_MOVERIGHT | Apex default |
| Space | IN_JUMP | Apex default |
| Left Ctrl | IN_DUCK (hold) | Apex default |
| C | IN_TOGGLE_DUCK (the sim toggles on the press edge) | Apex default |
| Left Shift | IN_SPEED (a tap is enough; the sim keeps sprint intent) | Apex default |
| Wheel down | tap IN_FORWARD (tap-strafe bind) | Ours; Apex's default is weapon cycling, which is out of scope |
| Wheel up | tap IN_JUMP (scroll jump) | Ours |
| Left click | unpause and grab (in `pause_input`) | Ours |
| Escape | pause and release (in `pause_input`) | Ours |
| R, `[`, `]`, F1 | respawn, previous spot, next spot, HUD | Ours |

IN_WALK, IN_LEFT and IN_RIGHT are unbound. IN_WALK has never been recorded, and keyboard turning is client-side only.

### Flags (`args.rs`)

`parse(impl Iterator<Item = String>) -> Result<Config, String>`. An unknown flag prints usage and exits with code 2.

| Flag | Config field, default | Added in |
|---|---|---|
| `--spot NAME` | `spot`, `wall 48` | M3 |
| `--shot N:PATH` | `shot`, none. Screenshot at frame N. Without `--script` the app exits once it is saved; with `--script` it exits at the script's `exit` once every shot is saved. Ignored with a warning under `--headless` | M3 |
| `--script PATH` | `scripted` plus the parsed script | M6 |
| `--frametime SECS` | f32, `SCRIPT_FRAMETIME` (0.004). Script mode only | M6 |
| `--headless` | Requires `--script`. No window or renderer | M6 |
| `--sens F` | `mouse_sensitivity`, 5 | M8 |
| `--fov-scale F` | `cl_fovScale`, 1.27216 (the retail default; copy yours from Apex) | M8 |
| `--fps-max N` | 240, at most 350 so no frame is shorter than the retail minimum | M8 |
| `--vsync` | `PresentMode::Fifo` instead of `AutoNoVsync` | M8 |
| `--size WxH` | 1600x900 | M8 |
| `--wheel-down A`, `--wheel-up A` | `forward`, `jump`, `duck` or `none` | M8 |
| `--wheel-mode event\|notch` | `event` | M8 |
| `--frame-latency N` | `Window.desired_maximum_frame_latency = NonZeroU32::new(N)`, unset by default | M8 |
| `--pipelined` | Keeps `PipelinedRenderingPlugin` (off by default) | M8 |

### Window and timing

- **Window.** `Window { title: "lurch", resolution: cfg.size.into(), present_mode, desired_maximum_frame_latency, .. }`.
- **Winit.** `WinitSettings::continuous()` (type-checked), so an unfocused or xvfb window is not throttled to 60 Hz.
- **Pipelined rendering.** `PipelinedRenderingPlugin` is disabled unless `--pipelined` (type-checked: `DefaultPlugins.build().disable::<bevy::render::pipelined_rendering::PipelinedRenderingPlugin>()`). That drops one frame of input latency. M10 measures the cost.
- **Pipeline compilation.** With `--shot` or `--script`, `RenderPlugin { synchronous_pipeline_compilation: true, ..default() }` is set (the field is at `bevy_render-0.19.1/src/lib.rs:135`). Otherwise a capture can catch frames whose mesh pipelines are still compiling. It stays off for interactive play.
- **Limiter (`limiter.rs`).**
  - `Limiter { period: Duration, next: Instant }` with `wait()` is plain Rust: sleep until 0.5 ms before `next`, spin to `next`, then set `next = max(next + period, now)`.
  - `LimiterPlugin` puts it in the render sub-app when one exists and pipelined rendering is off: `app.sub_app_mut(RenderApp).insert_resource(..).add_systems(Render, limit.in_set(RenderSystems::Cleanup))`, with types from `bevy::render::{Render, RenderApp, RenderSystems}`. That runs after present and before `send_time`, so `Time<Real>` includes the sleep and no sleep sits between input and present.
  - The probe measured a 100 ms cap as dt 100.0 ± 0.3 ms under xvfb (verified).
  - Otherwise (headless, or `--pipelined`) it runs in `Last`. It is off in script mode.
- **Startup log.** Log the requested present mode and the adapter name (`RenderAdapterInfo`; likely available in the main world, check in M8), and show both on the HUD.
  - On Wayland, `Immediate` does not exist, so `AutoNoVsync` falls back to Mailbox or Fifo. Under Fifo the fps locks to the refresh rate and `--fps-max` has no effect.
  - Whether the resolved mode can be read is unverified. M10's check is behavioural.
- **Cursor.** `apply_cursor` mirrors `Paused` into `Single<&mut CursorOptions, With<PrimaryWindow>>` when `Paused` changes:
  - paused: `visible: true, grab_mode: CursorGrabMode::None`;
  - playing: `visible: false, grab_mode: CursorGrabMode::Locked`. X11 has no Locked mode and falls back to Confined (verified in bevy_winit).
  - Focus loss pauses. The OS drops the grab, and bevy_winit only re-applies on a change, so the click that resumes re-grabs.
- **Pausing.** While paused, no commands are made. The clock advances only by commands, so pausing is exact.

## 6. Milestones

`$S` below is the session's scratchpad directory. Every milestone also meets the commit checks of section 0.

| | Delivers | Where | Status |
|---|---|---|---|
| M0 | Plan in the repo, pointers, CLAUDE.md non-goals | cloud | done |
| M1 | Client input path recovered from the oracle | cloud | not started |
| M2 | lurch API for a shell (section 3) | cloud | not started |
| M3 | `game/` scaffold, environment, CLAUDE.md rules | cloud (+ workstation smoke) | not started |
| M4 | Input to commands (`CmdPlugin`) | cloud | not started |
| M5 | Simulation loop (`SimPlugin`) | cloud | not started |
| M6 | Scripts and the headless binary | cloud | not started |
| M7 | Map, camera and body rendering | cloud | not started |
| M8 | Playable core: flags, window, timing, cursor | cloud (+ workstation first play) | not started |
| M9 | HUD, spots, script run under xvfb | cloud | not started |
| M10 | Workstation verification and docs | workstation | not started |

### M0. Plan in the repo (done)

- This file is committed as `docs/game-shell-plan.md`.
- `docs/roadmap.md` item 2 points to it and records that Bevy 0.19.1 is approved for a separate `game/` crate.
- The CLAUDE.md Layout section lists it, and CLAUDE.md's Non-goals mark multiplayer and our own models as deferred, not refused.
- The spawn FOV fix from section 3.5 is in.

### M1. Client input path from the oracle

- **Steps** (cloud, decompile, time-boxed to one session). Use `tools/decomp.sh` or `tools/dec.sh`, and `tools/annotate.py` to name ConVars. dec.sh runs without auto-analysis, so there is no xref database. Find references to an RVA with a scratch scan of `.text` for RIP-relative disp32 operands that resolve to it; keep that script in `$S`, not the repo.
  1. **kbutton.**
     - Find the `+forward` and `-forward` ConCommands: their name strings are in `.rdata`, each ConCommand points at its callback, and `in_forward`'s kbutton is at RVA `0xd4151a8` (`capture/drive.cpp`).
     - Decompile the callbacks to reach KeyDown and KeyUp, then KeyState (it reads state bits 1, 2 and 4 and returns a float) and the button-bit builder.
     - Record the KeyState table, whether the command bits are `state & 3`, and whether the bits are built before or after KeyState clears the impulses.
  2. **CreateMove `0x703a70`.**
     - How forward and side are formed from KeyState: any speed scale, any normalisation, any IN_WALK scaling.
     - Whether the frametime is clamped.
     - Where the angles come from.
  3. **RunCommand `0x718460`.** Any frametime clamp or negative-frametime check before `gpGlobals->frametime = ucmd+0x270` (`sv_clampPlayerFrameTime`, `player_disallow_negative_frametime`).
  4. **Mouse.** Through xrefs to the `mouse_sensitivity` ConVar: degrees per count, yaw and pitch signs, the pitch clamp, `m_invert_pitch`. The oracle image has no `m_yaw` or `m_pitch` ConVar (verified), so any factor is a literal in the mouse code.
  5. **Optional: projection.** Where `view_fov`'s value becomes a frustum, to settle whether it is 4:3 Hor+.
- **Output.** A "Client input" section in `docs/oracle.md`, written as behaviour and constants with RVAs. It must contain no decompiler output (CLAUDE.md, Legal). Under each of Q1 to Q4 in this file, write "settled (rva …)" or "fallback".
- **Fallbacks**, if a task does not resolve:
  - Source's KeyState table, with the bits taken before the impulse clear.
  - No frametime clamp in lurch; the clamp stays in the game crate.
  - Source's mouse convention.
  - M2 marks the dependent tests provisional.
- **Acceptance.** The section exists and Q1 to Q4 each say settled or fallback.

### M2. lurch API

- **Steps.** Section 3 in order: buttons and re-exports, `input.rs` (from M1's table), the setup.rs additions, the Pilot derives and `oldbuttons`/`with_oldbuttons`/`eye`/`Eye`/`Clock`, the `Player` helpers, then `tests/frame.rs`. The spawn FOV fix is already in.
- **Files.** `src/sim/{buttons.rs (new), input.rs (new), mod.rs, jump.rs, duck.rs, sprint.rs, setup.rs, pilot.rs}`, `tests/frame.rs` (new).
- **Acceptance (cloud).**
  - `cargo test` is green, with a time within noise of before (about 17 s).
  - For `cargo test --test frame`, libtest's `finished in` reads under 0.1 s.
  - `git diff --exit-code Cargo.toml Cargo.lock` prints nothing.
  - Before committing, in bash, `diff <(git show HEAD:src/sim/pilot.rs | sed -n '/pub fn step/,/^    }/p') <(sed -n '/pub fn step/,/^    }/p' src/sim/pilot.rs)` prints nothing.

### M3. Scaffold, environment, CLAUDE.md rules

- **Steps.**
  - Write `tools/bevy-deps.sh` (section 9) and run it.
  - Create `game/Cargo.toml` exactly as in section 2.
  - `src/lib.rs`: `Config`, `FrameIndex`, `Frame`, the shared messages, `common()`, and empty `CmdPlugin`, `SimPlugin`, `ViewPlugin`, `HudPlugin` and `LimiterPlugin`.
  - `src/args.rs` with `--spot` and `--shot` only.
  - `src/main.rs`: `DefaultPlugins` with the window settings, `Camera3d`, the clear colour, and synchronous pipeline compilation when `--shot` is given. For `--shot`, at frame N run `commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path)).observe(|_: On<ScreenshotCaptured>, mut e: MessageWriter<AppExit>| { e.write(AppExit::Success); })` (type-checked).
  - `game/tests/shell/main.rs` with `mod play;` and a smoke test: `MinimalPlugins + InputPlugin + CmdPlugin + SimPlugin`, three `update()`s.
  - Add `/game/target` to `.gitignore`.
  - CLAUDE.md edits (section 10): the dependency rule, the Approach line, the Layout entries for `game/` and `tools/bevy-deps.sh`, and the cloud-session line.
- **Acceptance (cloud).**
  - `cargo check --manifest-path game/Cargo.toml --tests` (about 3.5 min cold), then `cargo build --manifest-path game/Cargo.toml` (about 8 min cold).
  - `cargo test --manifest-path game/Cargo.toml` passes.
  - `xvfb-run -a -s "-screen 0 1280x720x24" game/target/debug/lurch-game --shot 30:$S/m3.png` exits 0, and the PNG, opened with Read, is the sky colour.
  - `CARGO_HOME=$S/empty-cargo cargo test --offline` passes at the root, which shows a fresh clone needs no crates.
  - The root `Cargo.lock` is unchanged, and `test $(wc -l < CLAUDE.md) -le 200` passes.
- **Workstation.** `vulkaninfo --summary`, and `cargo run --manifest-path game/Cargo.toml` once to see a window.

### M4. Input to commands

- **Steps.**
  - `binds.rs`: `Action`, the bind table in `Config`, `Held`, the wheel modes and accumulators.
  - `look.rs`.
  - `cmd.rs`: `CmdBuilder`, `CmdQueue`, `CmdLog`, `Paused`, `pause_input`, `read_input`, `apply_look_reset`, `make_cmd`, `clamp_frametime`. All of them go in the `Frame` sets.
- **Files.** `game/src/{binds.rs, look.rs, cmd.rs, lib.rs}`, `game/tests/shell/input.rs`.
- **Acceptance (cloud).** `cargo test --manifest-path game/Cargo.toml --test shell input::`. The tests are headless, with `TimeUpdateStrategy::ManualDuration`, `CmdLog` inserted, and messages written by `app.world_mut().write_message(..)` using `window: Entity::PLACEHOLDER` and `logical_key: Key::Unidentified(NativeKey::Unidentified)`. Move values are asserted through `key_state`. Cases:
  - **Numbering.**
    - The first update makes no command.
    - After that, exactly one command per update, numbered 1, 2, 3.
  - **Keys.**
    - Space pressed and released before one update gives IN_JUMP on exactly that command and not on the next.
    - W held: the first command's forward is `key_state(DOWN|IMPULSE_DOWN)`, later ones `key_state(DOWN)`. W and S together give `key_state(DOWN) - key_state(DOWN)`.
    - Repeat messages are ignored.
  - **Wheel, event mode.**
    - One `y: -1.0` Line message gives IN_FORWARD with forward `key_state(IMPULSE_DOWN|IMPULSE_UP)` on one command.
    - Two messages in one frame do the same.
    - Messages on two consecutive frames set the bit on both commands.
    - A wheel tap while W is held changes nothing.
  - **Wheel, other units.** `wheel_notch_mode_accumulates_fractional_lines_ours`: four `y: -0.25` messages on four frames give one tap, on the fourth. A direction flip resets the accumulator. 120 Pixel units give one tap in either mode.
  - **Focus.**
    - W held, then `KeyboardFocusLost` plus a synthetic `KeyboardInput { key_code: KeyW, state: Released }` before one update: no panic, and the next command has no IN_FORWARD.
    - With `Config` binding `MouseButton::Back` to IN_FORWARD: hold W and Back, lose focus (no mouse release arrives), then press and release W. The bit is clear on the command after.
  - **Pause.**
    - Escape pauses, and no commands are made.
    - A left click resumes.
    - `WindowFocused { window: Entity::PLACEHOLDER, focused: false }` pauses.
    - A key pressed while paused is not held after resuming.
  - **Look.**
    - `MouseMotion { delta: (100, 0) }` at sensitivity 5 gives yaw -11.0 (±1e-4).
    - Pitch stops at 89, and yaw wraps into (-180, 180].
  - **Command fields.** frametime has exactly the bits of `clamp_frametime(manual dt)`, and `random_seed == command_seed(n)`.
  - **Actions.** R, `[` and `]` write `Respawn::Spot(0)`, `Spot(-1)` and `Spot(1)`. Check through `SimPlugin` in M5 or by reading `Messages<Respawn>`.

### M5. Simulation loop

- **Steps.** `sim.rs`: `Sim`, `Map`, `Spots`, `History`, `Snapshot`, `Respawn`, `without_view`, and the `apply_respawn`, `run_cmds` and `pose` systems (sections 4.6, 4.7). All resources are inserted in `build`.
- **Files.** `game/src/{sim.rs, lib.rs}`, `game/tests/shell/sim.rs`.
- **Acceptance (cloud).** `cargo test --manifest-path game/Cargo.toml --test shell sim::`. Each test is driven by input messages written per frame (W, Shift, Space, Ctrl, motion), with `ManualDuration` at `SCRIPT_FRAMETIME`:
  - **`shell_equals_pilot`.** About 600 frames of sprint, jump, slide and turning from `hill`.
    - Group `CmdLog` by frame index.
    - Reference loop: a fresh `Pilot::new(spot)`, `Clock::default()` and `World::from_tris(testmap::build().tris)`. For each frame, `step` each command and `advance`, then `view_frame(s, velocity, clock.now(), last.frametime)` once.
    - After every frame that ran a command, `format!("{:?}")` of the reference pilot must equal the shell's.
  - **`two_commands_in_one_frame_run_the_view_once`.** With `Paused(true)`, which stops `make_cmd` but not `run_cmds`, push two commands per update into `CmdQueue` during a slide.
    - The result must equal a reference that runs `step, step, view_frame`.
    - It must differ from one that runs `view_frame` per command; that second check proves the test is sensitive.
  - **`history_replays_bit_for_bit`.** Restore the oldest snapshot's pilot and clock, replay the remaining snapshots' commands with `step` only, and require `format!("{:?}", without_view(..))` equal to the live pilot's.
  - **`respawn_resets_clock_not_command_numbers`.**
  - **`held_jump_through_respawn_does_not_jump`.** Space held, then `Respawn::Spot(0)`: `velocity[2] == 0` after the first command following the respawn.
  - **`kill_plane_respawns`.**
    - Write `Respawn::At { origin: [20000.0, 0.0, 0.0], yaw: 0.0 }`, which is off the ±16384 floor, and run 700 updates. The fall to -2048 takes about 2.34 s, about 585 commands.
    - The pilot ends within 0.1 of the current spot's origin.
    - `history.len()` is the number of commands k since that respawn.
    - The clock equals `Clock::default()` advanced k times.
    - The last command number equals the number of commands made.
  - **`pose_before_the_first_command`.** `Pose` fov is 70 at `fov_scale` 1.0, and `view_now == Clock::START as f32`.
  - **Grep rule.** `grep -nE 'FixedUpdate|Time<(Fixed|Virtual)>|Res<Time>' game/src/{cmd,sim}.rs` prints nothing.

### M6. Scripts and the headless binary

- **Steps.**
  - `script.rs` parses lines of the form `FRAME ACTION ARGS`.
    - `#` starts a comment. Frames are non-decreasing. Frame F is the F-th update from 0. Frame 0 makes no command, so input on frame 0 first shows in command 1.
    - Actions: `down KEY`, `up KEY`, `tap KEY` (press and release in the same frame), `wheel N` (one `MouseWheel { unit: Line, x: 0.0, y: N }`; negative is down), `look DX DY` (one `MouseMotion`), `spot NAME` (`Respawn::Named`), and `exit`.
    - KEY is one of `KeyW KeyA KeyS KeyD Space ControlLeft ShiftLeft KeyC KeyR BracketLeft BracketRight F1`.
  - `ScriptPlugin(Script)`: `inject_script`, `ScriptFrametime`, `script_end`.
  - `script_end`, at the `exit` frame:
    - prints `script::final_line(&Sim, &CmdBuilder)`, which is `final cmd=N origin=[..] velocity=[..] clock=..` with Debug formatting (exact);
    - sets `Paused(true)`, so no further commands change the result;
    - writes `AppExit::Success` once no screenshot is pending.
  - `args.rs` gains `--script`, `--frametime` and `--headless`.
  - `--headless` runs `MinimalPlugins + InputPlugin + CmdPlugin + SimPlugin + ScriptPlugin` with `ManualDuration`. MinimalPlugins' runner loops without a window (likely; check the default `ScheduleRunnerPlugin` mode).
  - `scripts/tour.txt` sprints, slides down the hill and jumps. `scripts/tapstrafe.txt` sprints, jumps, then wheel-taps forward with the view turned 90° over several frames.
- **Files.** `game/src/{script.rs, main.rs, args.rs}`, `game/scripts/*.txt`, `game/tests/shell/script.rs`.
- **Acceptance (cloud).** `cargo test --manifest-path game/Cargo.toml --test shell script::`:
  - **`parser_rejects_bad_lines`.** Unknown key, unknown action, frames going backwards.
  - **`commands_match_the_script`.** A 12-frame script with a hand-written table of `(command_number, buttons, forward, side)`, using `key_state` constants for the move values. It covers a press, a hold, a tap, a release, two wheel taps in a row, and a sprint press with `IN_SPEED_PRESSED`. No second model of `Keys`.
  - **`same_script_same_result`.** Two runs of each script give identical final lines.
  - **`shell_equals_pilot_for_each_script`.** The M5 reference loop over `tour.txt` and `tapstrafe.txt`.
  - **`script_frametime_bits_are_exact`.** Every `CmdLog` frametime has `to_bits()` equal to the requested f32, for 0.004 and 0.0041666665.
  - **Binary.** `game/target/debug/lurch-game --headless --script game/scripts/tour.txt | grep '^final '` prints one line, the same on two runs.

### M7. Map, camera and body rendering

- **Steps.**
  - `convert.rs` (section 4.8).
  - `view.rs`: the map mesh with the floor checker and tints, the material, the light, the ambient, the clear colour, `MainCamera`, `sync_camera`, `Body` and `sync_body`, and `take_shots`.
  - `ViewPlugin` takes a target, `Window` or `Image { w, h }`, so a test can render offscreen. In 0.19 `RenderTarget` is its own component on the camera (verified).
  - The render test:
    - builds `DefaultPlugins` with `primary_window: None`, `exit_condition: ExitCondition::DontExit`, `WinitPlugin` disabled and `RenderPlugin { synchronous_pipeline_compilation: true, .. }`;
    - loops `tick_global_task_pools_on_main_thread()` while `plugins_state() == PluginsState::Adding`, then calls `finish()` and `cleanup()` (type-checked and run);
    - renders to `Image::new_target_texture(640, 360, TextureFormat::Rgba8UnormSrgb, None)` with `COPY_SRC` usage, and captures with `Screenshot::image`;
    - runs `CmdPlugin + SimPlugin + ViewPlugin`, without `HudPlugin`, using `Config { spot: "wall 1024", fov_scale: 1.0, .. }`.
- **Files.** `game/src/{convert.rs, view.rs}`, `game/tests/shell/{convert.rs, render.rs}`.
- **Acceptance (cloud).**
  - `cargo test --manifest-path game/Cargo.toml --test shell convert::`:
    - For 10,000 angles from a small LCG in the test (no rand crate), `rot(a) * Vec3::NEG_Z`, `* Vec3::X` and `* Vec3::Y` equal `dir(f)`, `dir(r)` and `dir(u)` from `lurch::sim::math::angle_vectors(a)` within 1e-5.
    - `(rot([0,0,10]) * Vec3::X).y < 0`: a positive roll lowers the right side.
    - `vfov(70.0)` and `vfov(90.0)` match 55.413° and 73.740° within 1e-4 rad.
  - `cargo test --manifest-path game/Cargo.toml --test shell -- --ignored render::` passes on lavapipe. The test prints the sampled values. If a relation fails while the PNG looks right, change the tint, not the check.
    - The top-left pixel is within 1 of (140,178,217), the sky.
    - The centre pixel is not sky and has r > b + 10 (the warm ±X face of wall 1024).
    - The bottom-centre pixel is not sky, with |r−g| ≤ 6 and |g−b| ≤ 6 (floor grey).
    - The geometry at 640x360 puts wall at the centre, sky at the top-left and floor at the bottom-centre (checked by a critic).
  - Run `xvfb-run -a -s "-screen 0 1280x720x24" game/target/debug/lurch-game --spot "wall 128" --shot 60:$S/wall.png`, and the same for `ramp 30` and `hill`. Read each PNG:
    - the horizon is level;
    - every wall face is drawn, none missing from culling, and faces away from the sun are not black;
    - the checker is visible;
    - the sky is above.

### M8. Playable core

- **Steps.**
  - The remaining flags in `args.rs`.
  - Window settings, `WinitSettings::continuous()`, `PipelinedRenderingPlugin` off unless `--pipelined`, and `--frame-latency`.
  - `limiter.rs`: `Limiter` and `LimiterPlugin` (section 5), off in script mode.
  - `apply_cursor`, and `Paused(true)` at windowed start without `--script`.
  - The startup log of present mode and adapter.
- **Files.** `game/src/{main.rs, args.rs, limiter.rs, view.rs}`, `game/tests/shell/play.rs`.
- **Acceptance (cloud).**
  - `--test shell play::`:
    - `args_parse` covers every flag, defaults and errors.
    - `limiter_holds_the_period`: a plain `Limiter` at 200 fps takes at least 0.245 s for 50 `wait()`s.
    - `limiter_plugin_headless`: `MinimalPlugins + LimiterPlugin` at `--fps-max 200` takes at least 0.24 s for 50 updates.
  - The xvfb `--shot 30` run still exits 0, and its log shows the adapter and the present mode.
- **Workstation.** First real play: `cargo run --manifest-path game/Cargo.toml -- --sens <yours> --fov-scale <yours>`.

### M9. HUD, spots, script run under xvfb

- **Steps.** `hud.rs` (text, crosshair, `HudVisible`), the spot keys through `Respawn`, and `take_shots` with pending-shot counting for script mode.
- **Files.** `game/src/{hud.rs, view.rs, cmd.rs, sim.rs}`, `game/tests/shell/play.rs`.
- **Acceptance (cloud).**
  - `--test shell play::`:
    - R respawns at the current spot.
    - `[` and `]` move to the previous and next spot, wrapping.
    - F1 flips `HudVisible`.
  - In bash: `diff <(game/target/debug/lurch-game --headless --script game/scripts/tapstrafe.txt | grep '^final ') <(xvfb-run -a -s "-screen 0 1280x720x24" game/target/debug/lurch-game --script game/scripts/tapstrafe.txt --shot 400:$S/hud.png | grep '^final ')` prints nothing.
    - xvfb-run merges stderr into stdout (measured), which is why both sides go through grep.
    - `tapstrafe.txt` must `exit` after frame 400.
  - The PNG shows the HUD text and the crosshair.
  - Root `cargo test` is still green.
  - Update roadmap item 2 to a status line pointing here, and add a note to item 4 (section 10).

### M10. Workstation verification and docs

- **Steps (the user plays; the session reads the results).** The checklist below, the Q1 recording, then the remaining doc edits in section 10. Set this plan's status column.
- **Play-test checklist:**
  - `vulkaninfo --summary`. If only llvmpipe answers, the GPU belongs to the guest, and frame times will not reflect play (Q7).
  - The cursor stays captured and turning never stops at the window edge (X11 Confined with raw motion, Q6). After alt-tab and a click, the grab comes back.
  - With vsync off, the HUD fps follows `--fps-max`. If it is locked to the refresh rate on Wayland, run under XWayland with `env -u WAYLAND_DISPLAY` and record which works (Q7).
  - Latency and fps: the default against `--pipelined`, and `--frame-latency 1` against unset. Keep the best default.
  - One mousepad 360 matches Apex at the same `mouse_sensitivity`, unless M1 settled Q3.
  - The FOV looks the same as Apex at the same `cl_fovScale` and aspect (Q2).
  - Wheel mode: which default feels like Apex on this mouse (Q8).
  - Each of these behaves as in the game: sprint, slide, slide hop, bunny hop, a wheel tap strafe, a climb on wall 256.
- **Q1 recording.**
  1. In the guest console, run `bind mwheeldown +forward`.
  2. On the host, run `MAP=mp_rr_arena_empty tools/record.sh pending/realkeys -:20`. An idle drive step stamps nothing.
  3. During those 20 s in the guest, press and hold W, tap W quickly, and scroll the wheel.
  4. Run `cargo run --bin inspect traces/pending/realkeys.ltr`, which prints `btn` and `fwd` per command. Read the first command of each press and of each tap.
  5. Move the recording to `traces/` only if `cargo test -- --ignored` passes on it.
- **Acceptance.** The results are written under section 10's questions. Root and game suites are green. `test $(wc -l < CLAUDE.md) -le 200` passes.

## 7. Verification strategy

- **The gates stay the proof.** The shell never computes movement or camera itself:
  - every command goes through `Pilot::step`;
  - every view update goes through `Player::view_frame`;
  - every camera value goes through `Pilot::eye`.
  
  Root `cargo test` (the replay gates) is unchanged and runs without Bevy.
- **Shell equals Pilot.** `shell_equals_pilot` and `shell_equals_pilot_for_each_script` replay the shell's own logged commands through a bare `Pilot` loop: step per command, then view_frame once per frame. They require identical Debug strings after every frame. Any frame coupling, double step, extra view_frame, stale clock or stray state write fails them.
- **Input equals intent.** `input::` and `commands_match_the_script` pin, from raw Bevy messages, the per-command buttons, edges, move values (through `key_state`), numbering, seeds and frametimes. This is where a tap-strafe regression would show.
- **Determinism.**
  - A script run with a fixed f32 frametime prints the same `final` line headless and under xvfb (M9).
  - The history ring replays bit for bit with `step` alone.
  - The library has no statics, RNG or clock reads (verified).
  - libm-backed `pow` and the `expf` fallback may differ across operating systems in the last bit. That matters only for cross-OS lockstep, which section 8 rules out.
- **No frame-coupled side effects.**
  - Sim state is written only in `apply_respawn` and `run_cmds`.
  - Frame time enters the sim only as `cmd.frametime`.
  - Nothing renders, plays or spawns from inside a step.
  - The M5 grep rule.
- **Rendering.**
  - The conversion tests pin the axes, the roll sign and the FOV formula.
  - The ignored render test checks the sky exactly and mesh pixels by relation. Lit mesh pixels are tonemapped and dithered, so they never equal a tint.
  - The xvfb screenshots catch missing faces and a wrong camera.
  - Lavapipe images were byte-identical across runs in one container (verified), but they are valid only for that Mesa version, so there are no reference-image diffs.
- **What tests cannot settle.** Feel, the mouse scale, the FOV convention and the wheel mode on a real mouse are workstation questions (section 10). Later, a view-setup capture hook (roadmap item 3) can gate the camera the way movement is gated.

## 8. Later phases

The user named multiplayer and models as later features (section 0). Ask before starting them. The viewmodel is already roadmap item 4.

### Viewmodel (roadmap item 4)

First built from Bevy primitives:

- A second `Camera3d` with `Camera { order: 1, .. }`, and `RenderLayers::layer(1)` for the viewmodel.
- `RenderLayers::from_layers(&[0, 1])` on the DirectionalLight, or the viewmodel renders unlit. This follows `examples/camera/first_person_view_model.rs:186`. Add layer 2 if a third-person camera renders the `Body`.
- `IsDefaultUiCamera` on the main camera, so the HUD stays put.
- Drive it with the recovered sway and bob as they are ported.

### Own models

- **Features.** Add `bevy_gltf`, `gltf_animation` and `jpeg` in one change. Each feature change costs a near-full rebuild of about 8 minutes, rigged models need `gltf_animation`, and most glTF textures are JPEG.
- **Loading.** `WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/pilot.glb")))`, spawned as a child of `Body`. 0.19 renamed `SceneRoot` (verified).
- **Assets.** In `game/assets/`, found through `CARGO_MANIFEST_DIR` under `cargo run`. Only our own or CC0 assets. Never Apex models, animations or textures.

### Multiplayer

The approach is Source-style numbered commands with server authority. It is likely the best fit, because it is what the oracle's ConVars describe: per-frame commands carrying real dt, `cl_cmdbackup`, `cl_predict_cmdlimit` 750, `cl_smoothtime`, and server-side frametime policing.

- **Wire format.** A Bevy-free `proto` crate for the format and a server loop. Transport is renet 2.0 with renet_netcode (UDP), and the client uses bevy_renet 5.0. Both support Bevy 0.19 (verified on crates.io).
- **Client to server.** Every frame, one unreliable packet with the newest command plus the previous two as backups (`cl_cmdbackup` 2).
- **Server.** A queue per client:
  - drop duplicates by command number;
  - reject negative frametimes and clamp the rest;
  - keep a budget of accumulated frametime against real time (playerframetimekick-style);
  - bound how many commands run per server frame;
  - recompute seeds and own the clock, never trusting client values;
  - call `step` only, never `view_frame`;
  - reply with the last command number run and its `Snapshot` (pilot with oldbuttons, plus the clock through `Clock::secs`).
- **Client reconciliation.**
  - Compare the server snapshot with the stored one for that number through `without_view`.
  - Only on a mismatch: restore the server state, keep the client's view-only fields, and replay the unacknowledged commands from `History` with `step` only (at most 750).
  - `view_frame` still runs once per rendered frame.
  - Smooth the visual error over 0.25 s (`cl_smoothtime`), as presentation only.
- **Remote players.** Replicate at about 20 Hz and interpolate two snapshots behind, reusing the `Body` bundle. Add bevy_replicon 0.44 with bevy_replicon_renet 0.20 once there is world state beyond pilots; it shares the renet connection.
- **Not used:**
  - lightyear's input and prediction stack: one input per fixed tick would silently change frame-rate-dependent mechanics such as the tap-strafe rate;
  - ggrs: lockstep needs cross-OS bit-exactness;
  - naia and matchbox: still on Bevy 0.18.
- **Already prepared by M2 to M9.** Commands as the only sim input, `CmdQueue` as the seam, numbered commands, `command_seed`, `Clock` with `secs`/`from_secs`, `Pilot: Clone`, `oldbuttons`/`with_oldbuttons`, `Snapshot`, `History`, `without_view`, respawn as an explicit event, an immutable collision world, and settings only from `pilot::settings()`.
- **lurch changes then** (ask first; each must keep the gates green): `#[cfg(unix)]` around `World::open` and the File source, or a `std::os::windows::fs::FileExt::seek_read` path. Today `src/world.rs:9` stops lurch from compiling for Windows (verified).
- **Cost to know.** About 1 ms per trace against full map meshes (roadmap item 5) makes replaying 24 commands take tens of ms. Replay only on a mismatch, and land the trace speed-up first.
- **Versions.** Re-check crates.io at that time. Bevy 0.20 is at rc.2, the netcode crates followed 0.19 within 1 to 7 days, and lightyear breaks roughly monthly.

### Mantle and the rest of the tech list

These land in lurch (roadmap item 1), and the shell gets them with no change. Once mantle exists, show `mantle` in the HUD state line.

### Real maps

`--map PATH.mesh` uses `World::open`. Rendering needs the triangles too: add a `World::tris()` accessor or read the mesh in the shell. This waits for meshes of the maps the oracle runs (docs/maps.md) and for the trace speed-up.

## 9. Environment

### Cloud container

Ubuntu 24.04, 4 CPUs, about 27 GB free. `tools/bevy-deps.sh` is run on demand, not from the SessionStart hook, so ordinary sessions stay fast:

```sh
#!/bin/sh
# tools/bevy-deps.sh: system packages to build and screenshot game/ in a cloud session.
set -e
test -d /root/.ccr || { echo "bevy-deps.sh: cloud container only; on the workstation see docs/game-shell-plan.md section 9"; exit 1; }
PKGS="g++ pkg-config libx11-dev libwayland-dev libxkbcommon-dev libxkbcommon-x11-0 libxcursor1 libxrandr2 libxi6 libx11-xcb1 libvulkan1 mesa-vulkan-drivers xvfb xauth"
dpkg -s $PKGS >/dev/null 2>&1 && exit 0
apt-get update
DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends $PKGS
```

- **What the packages are for.** `mesa-vulkan-drivers` provides lavapipe. `libwayland-dev` and `libxkbcommon-dev` are needed because the `wayland` feature links through pkg-config. `vulkan-tools` is optional, for `vulkaninfo`.
- **Cost.** `apt-get update` takes about 3.5 minutes through the proxy, and the install about 1 minute (verified). The `dpkg -s` check skips both when everything is present.

Build and test from the repo root:

```sh
cargo check --manifest-path game/Cargo.toml --tests   # about 3.5 min cold; separate artifacts, about 0.6 GB
cargo build --manifest-path game/Cargo.toml           # about 8 min cold; relink 1.8-2.1 s
cargo test  --manifest-path game/Cargo.toml --test shell          # headless groups
cargo test  --manifest-path game/Cargo.toml --test shell -- --ignored render::   # needs lavapipe
export XDG_RUNTIME_DIR=$S/xdg; mkdir -p -m 700 "$XDG_RUNTIME_DIR"    # silences a warning only
xvfb-run -a -s "-screen 0 1280x720x24" game/target/debug/lurch-game --spot hill --shot 60:$S/hill.png
```

- **Screenshots.** Open the PNG with the Read tool.
- **Backend.** wgpu picks Vulkan on llvmpipe by itself. `WGPU_BACKEND=gl` panics with "Unable to find a GPU!", because the GL backend is not compiled in (verified).
- **Display.** Without `DISPLAY`, winit panics. Always use `xvfb-run`, the offscreen render test or `--headless`.

Expected cost, measured with this Cargo.toml plus lurch (±20%; other agents may share the 4 CPUs):

| Step | Cost |
|---|---|
| `cargo check --tests`, cold | 3m18s |
| `cargo test --no-run`, cold | 7m57s |
| Binary | about 165 MB |
| Each integration-test binary | about 90 MB |
| `game/target` after check and build | about 2.5 GB |

Budget about 3 GB of disk.

Lavapipe frame times say nothing about feel. Never judge movement or camera feel from the cloud.

### Workstation

Not probed; the distro and GPU are unknown. `tools/bevy-deps.sh` refuses to run there. Install the build packages by hand:

- **Debian or Ubuntu:** `sudo apt-get install g++ pkg-config libx11-dev libwayland-dev libxkbcommon-dev libvulkan1`, plus the GPU's Vulkan driver.
- **Other distros:** `bevy-0.19.1/docs/linux_dependencies.md` under `~/.cargo/registry/src/`.

Then:

```sh
vulkaninfo --summary          # if only llvmpipe answers, the GPU belongs to the guest
rustup update stable          # >= 1.95
taskset -c 12-17 cargo build -j6 --manifest-path game/Cargo.toml
taskset -c 12-17 cargo run --manifest-path game/Cargo.toml -- --sens 1.9 --fov-scale 1.55
```

Guest vCPUs own host cores 0-11, so builds and the game stay on cores 12-17. The cold build there is unmeasured; scaling from the cloud numbers, 6 to 8 minutes is a guess.

## 10. Open questions, risks, doc edits

### Open questions

Each question names the step that settles it, and the answer is written under it.

- **Q1. KeyState fractions, bit order (unverified).** The table for press, hold, tap, re-press and release, and whether the command's bits are built before KeyState clears the impulses. The old evidence for "tap moves 0, press moves 1" is an artefact of `capture/drive.cpp` stamping 0x1 and 0x4. *Settle:* M1 task 1 (decompile), then the M10 `realkeys` recording.
- **Q2. FOV definition (likely).** That `view_fov` is a horizontal FOV at 4:3, widened Hor+, is a Source convention. *Settle:* M1 task 5 if time allows, M10's screenshot comparison, and later the view-setup hook (roadmap item 3).
- **Q3. Mouse scale (likely).** 0.022°/count × sensitivity is Source's convention, and the oracle has no `m_yaw` ConVar. *Settle:* M1 task 4, then M10's 360 comparison.
- **Q4. Frametime clamp (unverified in the oracle).** `[0.002857, 0.1]` are retail ConVars that the oracle image lacks. The recorded minimum is 0.0028575. Until settled, the clamp lives in the game crate. *Settle:* M1 tasks 2 and 3.
- **Q5. Apex default binds (unverified).** *Settle:* read R5Reloaded's default bind config in its `cfg/` on the workstation (M10).
- **Q6. Raw motion at the window edge on X11 (likely).** X11 falls back to Confined. *Settle:* M10 checklist.
- **Q7. Workstation GPU and present mode.** Whether Wayland locks the fps to the refresh rate. *Settle:* M10 checklist.
- **Q8. Wheel mode (ours).** `event` (Source's rule) against `notch` (our accumulator) on a high-resolution wheel. *Settle:* M10 checklist and the `realkeys` recording.

### Risks

- **Snippets.** Only the marked snippets were type-checked. M3 starts with a `cargo check`. The pinned 0.19.1 sources are the reference, not the online docs.
- **Features.** Changing the feature list costs a near-full rebuild. Settle it in M3.
- **Disk.** One target dir is about 2.5 to 3 GB. A second profile or feature set doubles that.
- **Vulkan only.** A machine without a Vulkan driver panics at startup.
- **Mantle.** It is missing, so walls of 96 to 192 units drop the pilot back. Players may read that as a shell bug; the docs say so.
- **Fixed-tick creep.** A future switch to a fixed-tick netcode crate would change fps-dependent mechanics, and the replay gates would not notice; only the shell tests would.
- **Pipelined rendering off.** It may cost frame rate. Measured in M10, and `--pipelined` restores it.
- **Wayland present modes.** No Immediate mode; Fifo fallback ignores `--fps-max`. Mitigated by XWayland; M10.
- **Provisional input values.** If M1 does not settle Q1, the move values in M2 and M4 are Source's and the tests are marked provisional. A later answer changes `key_state` and the bit order in `fill`, nothing else.
- **Bevy 0.20.** It is at rc.2. Stay on 0.19.1 until the netcode crates move.

### Doc edits

Keep CLAUDE.md under 200 lines. It has 181 now, so replace lines rather than add, and check `test $(wc -l < CLAUDE.md) -le 200` after every edit.

- **M0 (done).** This file, the roadmap item 2 pointer, the CLAUDE.md Layout entry, and the Non-goals edit below.
- **M1.** `docs/oracle.md`: a "Client input" section, written as behaviour and constants with RVAs.
- **M3, CLAUDE.md:**
  - *Meta-Rules.* Replace "Nothing is installed yet. Do not add a toolchain, a crate, or a dependency without being asked for it." with: "The lurch crate has no dependencies and gets none. Bevy 0.19.1, pinned exactly, is approved for the game crate in `game/` only, with the features its Cargo.toml lists. Ask before adding any other crate, feature or toolchain."
  - *Approach.* Replace "Phase 1 is headless. No renderer, no engine, no window. The first deliverable is a simulation that eats a usercmd stream and emits a per-tick trajectory to diff." with: "The lurch crate stays headless: no renderer, no engine, no window. The game shell in `game/` drives it, one command per rendered frame, and makes no fidelity claim of its own."
  - *Layout.* Add `game/   the Bevy game shell; its own workspace, root cargo test never builds it` and `tools/bevy-deps.sh   system packages for building game/ in a cloud session`.
  - *Where a session runs.* Add: "A cloud session can build `game/` after `tools/bevy-deps.sh`, run its headless tests and take xvfb screenshots on lavapipe. How the shell plays is judged on the workstation."
- **M9, docs/roadmap.md.**
  - Item 2 becomes a status line pointing to this plan.
  - Item 4 notes the second-camera and render-layer setup from section 8.
- **M10.** Answers under Q1 to Q8, and the milestone status column.
- **When section 8 starts:** roadmap "Later" gains "Multiplayer: numbered commands over renet (game shell plan section 8)".