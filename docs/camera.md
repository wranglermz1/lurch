# Camera: build 5dba2a2f-02ae8471

How the oracle's camera answers movement, recovered from the image as behaviour. The first
part runs inside the movement call, so the recordings hold its ground truth and
`src/sim/view.rs` is gated on it like the movement fields. The second part runs in the
client's view setup once a frame, between movement calls. Its state lives on the player
too, so it is gated the same way: from the oracle's state after one call, our frame must
reach the oracle's state before the next. What it renders (roll, FOV, eye height) is not
recorded and so not checked.

## Kept by the movement call (ported, gated)

| RVA | What | Notes |
|---|---|---|
| `0x826c90` | sprint tilt | `PlayerMove`, after max speed. Target `0x826a60`: on world-up ground at above 1.05 × walk speed, (turn rate / `sprinttilt_turnrange` 120, clamped ±1) × ((speed − walk) / (sprint − walk), clamped 0..1). The turn is `AngleDiff` (`0x42ff30`) from the yaw kept at `+0x2828`. The tilt `+0x1eb8` moves toward it at a velocity `+0x282c` that brakes to stop on it: `sign · √(2 · accel · slack)`, `sprinttilt_accel` 35, `sprinttilt_maxvel` 2 |
| `0x8067d0` | view punch springs | First thing in `PlayerMove`. Base punch (`+0x20a4` angle, `+0x20b0` velocity) and weapon punch (`+0x20bc`, `+0x20c8`) each step one frametime of a damped spring, solved exactly per axis (`0x8060d0`, underdamped case `0x433bc0`): `viewpunch_base_springConstant*` 65, `springDamping*` 9. A third pair at `+0x2fac` stays zero. The `viewPunchSpring` setting picks a spring table entry at runtime; the ConVars are what the recordings show |
| `0x12014f0` | `expf` | The CRT's, on its FMA path (the guest CPU has FMA): 64/ln 2 reduction, a table of 2^(j/64), a fused quadratic. glibc's `expf` differs by an ulp often enough to break the springs |
| `0x8702d0`, `0x8221b0`, `0x818320` | fall speed `+0x1a68` | Airborne: −vz before the call and again after categorize; zero on any ground move |
| `0x820790` | landing | At the end of `full_walk_move`: on ground with a fall speed of at least 80 (400 for titans), measured along the up vector when it is a wall's. Clears the fall speed |
| `0x807f70` | landing kick | Scale from the fall speed against `viewkickFallDistMin`/`Max` (feet, ×12) and `viewkickFallDistMaxScale`: below the minimum `fall / min`, then 1 to the max scale. Pitch `scale · (2 + 0.75 r)`, roll `scale · r` |
| `0x807080` | jump kick | In `check_jump`, after `jump_enabled`. Pitch `2 + r`, yaw `r`, roll `0.9 + 0.5 r` |
| `0x865510` | sprint transition | When the wish to sprint begins `+0x2618` = now; when sprinting turns on or off, the time (`+0x2608` / `+0x2610`) and the view's sprint fraction then, in 1/1023 steps (`+0x260c` / `+0x2614`) |
| `0x807520` | wall start kick | In `wall_contact` when more than 0.25 s since the last wall. `wallrun_viewpunch_onStart_*`: yaw and roll lean by the view's right · wall normal. No roll on the weapon |

A kick adds 20 × its angles to the base punch's angular velocity and 4 × to the weapon's.
Unported kicks: double jump (`0x8072d0`), wall-run timeout (`0x8078c0`) and wall hang
(`0x807c60`); none fires in the recordings.

Each draw `r` is uniform in [−0.5, 0.5): the command's random seed and a per-kick salt
mixed by Jenkins' one-at-a-time hash (`0xc40fc0`), which seeds a Park–Miller minimal
standard generator (`0x59ffd0`), scaled by 2⁻³¹ and capped below 1 (`0x5a0060`). Salts
are `0x7670_4c58` + axis (landing), `0x7670_4a58` (jump), `0x7657_5258` (wall start).

The seed is `usercmd+0x184`, set in `CreateMove` (`0x703a70` → `0x8ac6d0`) as the first word
of MD5 over the command's `+0x260`, `+0x8` (gpGlobals `+0x24`), view angles and `+0x264`,
top bit cleared (`net_old_seed_generation` 0). The recordings do not hold it, so the replay
cannot draw a kick: it checks that the oracle kicked on the same call and takes its punch
velocities. The model is confirmed by search instead: one seed reproduces jump.ltr's jump
kick (seq `0x316d`) and one its landing kick (seq `0x3223`), bit for bit.

## View setup

`0x67eff0` is the player's view calculation. It runs after each frame's movement call on
the frame's clock: gpGlobals `+0x24` as the time, and the frame's frametime, which is the
command's. Ported as `Player::view_frame` (state) and `Player::view_roll`; gated on the
slide tilt and the FOV ease.

Roll (`0x85ffc0`) adds, for plain movement:

- sprint tilt: `sprinttiltMaxRoll` (2°) × `+0x1eb8`;
- slide tilt: −`slide_viewTiltSide` (15°) × (view forward · `+0x30fc`). `+0x30fc` moves toward
  the horizontal velocity turned 90° left, scaled 0 at `slideStopSpeed` to 1 at
  `slide_viewTiltPlayerSpeed` (400), times the sliding flag; it rises at
  `slide_viewTiltIncreaseSpeed` (5) and falls at `slide_viewTiltDecreaseSpeed` (2.5) per
  second (`0x85fb70`). No tilt looking along the slide, full tilt looking 90° off it;
- `sv_rollangle` is 0, so no strafe roll. Dodge, wall-run and zipline rolls do not apply.

FOV (`0x8610c0`, `0x860f40`, ported as `Player::view_fov`): `cl_fovScale` clamped to
[1, 1.7] times the `fov` setting (70), rounded, capped at 115, then times the FOV scale and
clamped to [1, 179] (`0x67eff0`). The FOV scale is what sliding sets: while sliding
(`+0x2815` with `+0x2816`) it eases to `slideFOVScale` (1.1) over `slideFOVLerpInTime`
(0.25 s), and back to 1 over `slideFOVLerpOutTime` (0.25 s), linearly in game time
(`SetFOVScale`, `0x683e20` / `0x683d90`). Abilities widen the base through status effects
(`stim_visual_effect`, `timeshift_visual_effect`, `hunt_mode`; handles `0x285a5058`,
`0x2859d658`, `0x285a0094`), which are out of scope. Weapon zoom passes through `0x857900`.

Sprint eye drop (`0x8585d0`, `0x865240`, ported as `Player::sprint_eye_drop`): the eye
drops by `sprintViewOffset` (−6) × sin(f · π/2), f the sprint fraction. It ramps in over
`sprintStartFastDuration` (0.2 s) from the sprint start, and over `sprintStartDuration`
(0.8 s) after `sprintStartDelay` (0.2 s) from when the wish to sprint began, whichever is
behind; and out over `sprintEndDuration` (0.15 s). Each starts from the fraction when
sprinting turned. `update_sprint` stamps those times and fractions (`+0x2608` to `+0x2618`,
fractions in 1/1023 steps), so they are gated with the movement fields.

Eye (`0x858710`, ported as `Player::eye_position`): origin + the view offset `+0xb8` +
the pitch offset + the sprint eye drop on z. The view offset is the stance's (+0x18 in the
stance block: 60 standing, 32 crouched) eased through a duck inside the movement call
(`0x8214c0`, from `0x821610`): smoothstep of the duck timer, and for a duck begun in the air
shifted by half the hull difference, as the hull moves instead. It is gated with the
movement fields. The pitch offset (`0x858170`) moves the eye forward by `pitchOffsetScale`
(9.2) × sin(pitch), halved looking up, × (0.6 + 0.4 sin((1 − duck)·π/2)) (`0x869500`).
Lean, jetpack and prediction-error smoothing terms are left out.

Angles (`0x67eff0`, ported as `Player::camera_angles`): the view angles plus the punch
(`0x8580b0`: base `+0x20a4` + weapon `+0x20bc`, the weapon's pitch and yaw scaled by
1 − ADS fraction), plus a weapon-side offset (`0x857770`, `0x857980`, not recovered), then
the roll above computed on the punched angles.

Not ported yet:



Sprint shake scale (`0x626110`): `sprint_view_shake_style` 1 eases between 1 and 0.25 over
the sprint transition with a half cosine; style 0 between 1 and 0.5.

## To record

A capture hook that also logs the command's random seed (`usercmd+0x184`, or the prediction
seed global `0x171fb20` during the call) would let the replay draw kicks itself. A per-frame
hook on the view setup (eye origin, angles with punch and roll, FOV) is the check for the
second part.
