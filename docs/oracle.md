# Oracle: build 5dba2a2f-02ae8471

Recon for milestone 1 (see `docs/superpowers/specs/2026-10-07-oracle-harness-design.md`).
Every address is an RVA into `dumps/current.bin`. The base when the dump was taken was
`0x7ff7b6a90000`. Unless stated otherwise, each entry was read statically from the dump
with `tools/dec.sh` (Ghidra decompile), `tools/rtti.py` and `tools/settings.py`.
"Live" marks an entry that the capture DLL must still confirm on a running guest.

## Call chain

| RVA | Role | Evidence |
|---|---|---|
| `0x718460` | `CPrediction::RunCommand(this, player, ucmd, movehelper)` | Stores `ucmd` at `player+0x2f50`, sets `gpGlobals->frametime`, calls SetupMove, ProcessMovement and FinishMove through the objects below |
| `0x2331230` | prediction object, vtable `0x142aac8` | `+0xc8` SetupMove, `+0xd0` FinishMove |
| `0x815400` | `SetupMove(player, ucmd, mv)`, reached through the `0x717a80` thunk | Copies ucmd into `CMoveData` (layout below) |
| `0x22e93f0` | `C_GameMovement` instance | `.data` qword equal to vtable `0x144a2d8` |
| `0x805ee0` | **`C_GameMovement::ProcessMovement(this, player, mv)`**, vtable slot 2 | Called from `0x718995` with `mv = 0x2331100`. It saves `frametime`, multiplies it by `player+0x358c` (lagged movement), sets `mv+0xd4` = max speed, calls PlayerMove, then restores `frametime` |
| `0x2331100` | the static `CMoveData` passed to every call | |
| `0x717aa0` | `FinishMove(player, ucmd, mv)` | Writes `mv` origin and velocity back to the player, and `mv+0x24` to `player+0x1a3c` (old buttons) |
| `0x8221b0` | `PlayerMove` | Switches on movetype `player+0x452` |
| `0x81a080` | walk-movetype move (movetype 2 or 7, the `FullWalkMove` slot) | Half gravity, ground check, then ground or air, then half gravity |
| `0x8089e0` | half-gravity step | `v += gravdir * gravityScale * sv_gravity * 0.5 * dt * scale` |
| `0x818320` | ground / wall move, taken when `player+0x3dc` (ground entity) is valid | It branches on `player+0x266c` (the surface normal). A normal with `z >= 0.999` takes the floor path; any other normal takes the wallrun/climb path, which reads the `wallrun*` and `climb*` settings |
| `0x816c60` | `AirMove` | Air drag (`airDrag`, field `0x7e4`), gated by `airStrafeEnabled`; accel with `player+0x3108` (air speed) and `player+0x310c` (air acceleration); keyboard-grace redirect from the `jump_keyboardgrace_*` convars, gated on newly pressed `0x618` buttons |
| `0x804af0` | max speed = `0x8049e0(player)` × stance `speed` (or `sprintspeed` when `player+0x2604` is set) × `player+0x2cb8` | Helpers `0x869bd0` and `0x869c40` |

The detour target is `0x805ee0`. On entry, `ProcessMovement` copies the player's
abs origin and abs velocity into `mv`. On exit, `mv` holds the result, and FinishMove
then writes it back. So the capture reads `in` from the player before the call and
`out` from `mv` after it.

## Assumptions

1. **Client-side oracle.** Confirmed statically: of the seven code sites that reference to the
   `C_GameMovement` instance, only `0x718995` (in `RunCommand`) calls slot 2. `CGameMovement` (server, vtable `0x14db2f0`) has a constructed
   instance at `0x2371d90`, since static init runs regardless of role. HAVM's
   `docs/remote-server-call.md` records that this process plays against a remote dedicated server
   with no server VM. Live: the DLL counts calls on the server vtable slot 2 (`0xb8abf0`)
   and expects zero.
2. **dt per usercmd.** Confirmed: `RunCommand` sets `gpGlobals->frametime = ucmd+0x270`.
   The value `0x7184fc` writes first is overwritten. `ProcessMovement` then multiplies
   it by `player+0x358c`. `gpGlobals` is the pointer at `0x171f448`: `+0x10` curtime,
   `+0x30` frametime, `+0x44` interval_per_tick. There is no fixed movement tick.
3. **Speed and acceleration come from player settings.** Confirmed. Ground values are
   per-stance fields of the settings blob at `player+0x2240` (block below). Air speed and
   air acceleration are player fields (`+0x3108`, `+0x310c`), not settings and not convars.
   Gravity is `sv_gravity` × settings `gravityScale`.
4. **Usercmd offsets re-derived.** They differ from retail. Table below.

## Usercmd (oracle)

| Off | Field | Evidence |
|---|---|---|
| `0x000` | `command_number` | `RunCommand` passes `*ucmd` to `GetUserCmd` |
| `0x00c` | viewangles (pitch, yaw, roll) | SetupMove normalises them, clamps pitch to ±89 and copies them to `mv+0xc` |
| `0x018` | f32[3], copied to `mv+0x18` | |
| `0x024` | forwardmove, sidemove, upmove | SetupMove clamps each to [-1, 1] into `mv+0x30` |
| `0x030` | buttons | ORed with forced buttons `player+0x2f48`, copied to `mv+0x24` |
| `0x034` | impulse (u8) | copied to `mv+0x8` |
| `0x184` | random_seed | stored to the prediction seed global `0x171fb20` |
| `0x270` | frametime | written to `gpGlobals->frametime` |

The button bits are as given in HAVM `docs/usercmd-layout.md`. That doc covers the
retail layout; only the bit meanings carry over to this build.

## CMoveData (oracle)

| Off | Field |
|---|---|
| `0x008` | impulse |
| `0x00c` | viewangles |
| `0x024` | buttons |
| `0x028` | old buttons (`player+0x1a3c`) |
| `0x02c` | pressed = buttons & ~old |
| `0x030` | forward, side, up move, each in [-1, 1] |
| `0x03c`, `0x048`, `0x054` | `AngleVectors` forward, right, up |
| `0x060`, `0x06c` | forward and right flattened to z = 0, normalised |
| `0x078` | flat wish direction: fwd·flatForward + side·flatRight, normalised only if longer than 1 |
| `0x090`, `0x09c` | forward and right with their component along `0x0a8` removed, normalised |
| `0x0a8` | ground plane normal, initialised to (0, 0, 1) |
| `0x0b4` | ground wish: fwd·`0x090` + side·`0x09c`, normalised only if longer than 1 |
| `0x0c8`, `0x0d0` | standing hull height, and half its width |
| `0x0d4` | max speed (`0x804af0`) |
| `0x0f0` | f32[3], accumulated by the ground move |
| `0x118` | origin |
| `0x124` | velocity |

`src/sim/setup.rs` builds all of it from a command (checked against every recorded call's
CMoveData). Old buttons are the previous command's buttons. What a run from commands also
needs from the game's side of RunCommand: the player's view angles (`+0x2188`) are the
command's, `+0x3a92`/`+0x3a93` are set on every recorded player, and so is flag `0x40`.

## Player (oracle)

| Off | Field |
|---|---|
| `0x098` | flags byte; bit 1 is the stance index source (`(b >> 1) & 1`) |
| `0x140` | abs velocity (after `0x620060`) |
| `0x14c` | abs origin (after `0x61f810`) |
| `0x3dc` | ground entity handle (`0xffffffff` = none) |
| `0x452` | movetype (2 = walk) |
| `0x1a3c` | old buttons |
| `0x2240` | settings blob pointer |
| `0x2604` | sprinting |
| `0x266c` | surface normal used by the ground move |
| `0x2cb8` | speed scale |
| `0x3108`, `0x310c` | air speed, air acceleration |
| `0x358c` | lagged movement value |

## Player settings

A field is read as `*(settings + *(u32 *)global)`. Names are resolved by `tools/settings.py`.
The stance block starts at `*(u32 *)0x2387f6c` (0x78). Its stride is `*(u32 *)0x2387f74`
(0x68), and its index is `(player+0x98 >> 1) & 1` (0 = stand, 1 = crouch). It is 2 when
`player+0x3e0 < 1`, a field not yet identified.
Stance fields are offsets inside that block.

| Global | Offset | Name | Read by |
|---|---|---|---|
| `0x2385c68` | stance `0x48` | `speed` | max speed |
| `0x2386220` | stance `0x4c` | `sprintspeed` | max speed while sprinting |
| `0x2387678` | stance `0x50` | `acceleration` | ground move |
| `0x2387d60`, `0x2387e38` | stance `0x00`, `0x0c` | `hull_min`, `hull_max` | SetupMove |
| `0x2387320` | `0x7b8` | `speedScaleSide` | ground wish |
| `0x23879d8` | `0x7bc` | `speedScaleBack` | ground wish, backward only |
| `0x2385f98` | `0xdd8` | `gravityScale` | gravity |
| `0x23878e8` | `0x7e8` | `airDrag` | AirMove |
| `0x2387098` | `0x7e4` | unnamed | AirMove, constant drag term |
| `0x2387a38` | `0xe07` | `airStrafeEnabled` | AirMove |

The capture writes these values to the trace `settings` line under these names, read
the way the oracle reads them, along with `sv_gravity` (the ConVar pointed to by
`0x231ed78`, float at `+0x68`).

## Ground wish (floor path of `0x818320`)

This is the reading so far. The code is the authority, and the diff decides.

- Raw input is `mv+0x30` (forward) and `mv+0x34` (side), each scaled by 1.25 while sprinting.
- If the input's length is above 1, it is normalised. If the length `m` is in (0, 1], it
  is rescaled to `m(2 - m)`.
- Side input is multiplied by `speedScaleSide`. Forward input below zero is multiplied by `speedScaleBack`.
- The wish vector is then projected onto the ground plane. Friction and acceleration
  follow in `0x808d10`, `0x817c90`, `0x817ad0` and `0x826540`. Those have not been read yet.

This is not Source's `WalkMove`. The spec's "start from published Source behaviour" holds for
the base trace-and-clip layer only. The ground and air math is recovered from these functions.

## Movement call tree

Recovered by reading the decompiles (`tools/dec.sh`, `tools/annotate.py`), and checked by
replaying recordings through `src/sim`. Names are ours.

| RVA | Ours | Notes |
|---|---|---|
| `0x8702d0` | `pre_think` | Called by RunCommand before ProcessMovement; stamps the jump-grace time and up vector while standing |
| `0x8221b0` | `player_move` | With `sv_optimizedmovement` and an unmoved origin, skips categorize unless moving up faster than 200 |
| `0x865510` | `update_sprint` | `+speed` hold window, can-keep (`0x865160`) and allowed (`0x864d90`) tests; allowed needs last tick's velocity (`+0x41c`) in front of the eye yaw (`+0x1ebc`) |
| `0x81a080` | `full_walk_move` | |
| `0x8089e0` | `half_gravity` | While sliding on the ground, also pushes downhill: `v.xy += dv · plane.xy · plane.z` with the ground plane `+0x3e60` |
| `0x809cb0` | `check_jump` | Grace period, skip penalty, anti multi-jump, slide jump, wall push |
| `0x818320` | `ground_move` | Velocity re-projected along the ground plane (`+0x3e60`) for the move only |
| `0x808d10` | `ground_accel` | sprintAcceleration above speed−1, lowAcceleration below lowSpeed, else acceleration |
| `0x817c90` | `ground_friction` | The part of velocity along the wish direction (clipped by the last wall) is spared |
| `0x817ad0` | `ground_accelerate` | Caps at max(|v|, wishspeed) |
| `0x816c60` | `air_move` | Air drag, air accelerate (`0x816a40`), keyboard redirect (lurch), then the move |
| `0x8162a0` | `along_slope` | Continues a blocked move along a walkable plane, else steps |
| `0x815e70` | `step_move` | |
| `0x828d10` | `step_slide` | Raise by step+1/32, slide, drop, keep if it got further |
| `0x8272b0` | `slide` | 8 bumps, 1/128 overclip, two-plane crease |
| `0x828000` | `settle` | The slide used to drop back down; stops on a plane with n·up > 0.1 |
| `0x806f20` | `stay_on_ground` | Probe down by step height, snap if walkable |
| `0x81f070` | `categorize` | Probe by step height (walking) or 2; walkable at n·up ≥ 0.7; re-probes past a steep lip, alternating by command parity |
| `0x81ebb0` | `set_ground` | Landing slowdown (`0x869a80`); leaving the ground clears `+0x30ed` (jumped recently), which a jump then sets again |
| `0x805710` | (surface) | Surface friction ×1.25, capped at 1, by the trace's surface index |

## Wall climb

The wallrun code with `wallrun` off; `climbEnabled` selects its climb paths. Ported in
`src/sim/climb.rs`, verified by `traces/climb_*.ltr`.

| RVA | Ours | Behaviour |
|---|---|---|
| `0x81ca50` | `wall_contact` | Called with the hit point, normal, trace and pre-clip velocity wherever a move is blocked: the end of the air move, a failed ground step, and (air move, nothing hit) a remembered wall once `wallrun_retry_interval` passes. Classifies the wall; a climb start makes the wall normal the up vector |
| `0x81bb60` | `classify_wall` | Refuses: world up and on (or within 0.5 s of) the ground; not facing the wall within ~45° (`0xbf0a40`); vz above `climbSpeedStart`/2; the hit above `verticalGainCutoff_wallrun + floor − climbFinalJumpUpHeight`; floors (n.z > 0.7) and ceilings; startsolid or contents `0x100000`; the repel wall; velocity not into the wall (`wallrun_minAngle_air`). Then probes the wall at foot and head height, and with world up also at the top when reaching for a ledge (`0x80ed90`) |
| `0x81c750` | `wall_probe` | A cube of half the hull width swept from the probe point into the wall by reach + `wallrunAllowedWallDistance` + 0.2929·half; a wall within the allowed distance counts |
| `0x81ae40` | `climb_think` | Every call, after the jump check. On a wall it lets go on duck, on facing away, on stalling (vz < 10 after 1 s), at the top (`0x81ade0` + `climbHeight`), or on pushing away for `wallrun_pushAwayFallOffTime`; every 4th command it re-probes the wall. Letting go pushes off by `wallrun_fallAwaySpeed`, remembers the wall, ends the climb, and at the top adds a jump of up to 10 + `climbFinalJumpUpHeight` |
| `0x818320` wall branch | `wall_wish`, `wall_accelerate` | Axes from `up × right`; forward input pushes up the wall (`wallrun_upwardAutoPush`). Vertical and horizontal each decelerate beyond and accelerate toward their own top speed: `climbSpeedStart` easing to `climbSpeedEnd` near the top, `climbHorzSpeed`; without upward input the `wallrun*` speeds apply, with slip after `wallrun_slipstarttime` |
| `0x8089e0` | `half_gravity` | On a wall, gravity × `0x806be0`: none while pushing into the wall, else ramping in over `wallrunGravityRampUpTime` by wall strength (`0x81d800`) |
| `0x81d9c0` | `wall_jump` | Out by `wallrunJumpOutwardSpeed`, up by `climbFinalJumpUpHeight` × strength |
| `0x866760` | `end_climb` | Up back to (−0, −0, 1), leave time stamped. Also run before an air move with a wall up |
| `0x870160` | `update_floor_height` | In pre-think: `+0x1d00` follows the abs origin down, and up only on walkable ground; each climb or wall jump lowers it by `wallclimb_vertical_gain_reduction` |

Player fields: up `+0x266c`, previous up `+0x2678`, climb start origin `+0x2684`, climb
count `+0x2690`, same wall `+0x2694`, push-away timer `+0x26a4`, retry `+0x26b0`/`+0x26b4`/
`+0x26c0`, climb start time `+0x307c`, leave time `+0x3080`, repel wall `+0x1bb4`
(count `+0x1bd0`), floor height `+0x1d00`. Landing on world-up ground clears the repel wall
and sets "same wall"; a normal jump clears the count and "same wall".

## Traces

Movement traces go through IEngineTrace (`*0xd413e80`) slots `+0x18`, `+0x20`, `+0x38` and `+0x40`;
capture v3 hooks those. `Ray_t` (0x68 bytes): centre start, delta, start offset, half extents,
up, flags at `+0x5c`/`+0x5d`. `trace_t` (0x180): start `+0x00`, end `+0x10`, normal `+0x20`,
plane dist `+0x2c`, fraction `+0x30`, allsolid `+0x38`, startsolid `+0x39`, a step flag
`+0x3b`, surface index `+0x48`, entity `+0x60`. The player hull is (−16,−16,0)–(16,16,72).

## ConVars (from the dump)

`sv_gravity 750`, `sv_maxvelocity 34000`, `sv_optimizedmovement 1`, `player_extraairaccelleration 2`,
`jump_graceperiod 0.2`, `jump_keyboardgraceperiodmin 0.2`, `…max 0.5`, `jump_keyboardgrace_strength 0.7`,
`jump_keyboardgrace_max 0.7`, `skip_time 1`, `skip_jump_height_speed 450`, `skip_jump_height_fraction 1`,
`skip_speed_retain −1`, `skip_speed_reduce 100`, `slide_step_velocity_reduction 10`.

## Client input

How a frame's input becomes a usercmd, for the game shell (`docs/game-shell-plan.md`).
Read from the image by xref scan and decompile; nothing here is checked against a recording
except the clock rule at the end.

**Key buttons.** Each `+name`/`-name` ConCommand inlines Source's `KeyDown`/`KeyUp` on a
kbutton: two key-code slots `down[2]` at `+0`, `state` at `+8`, and a press latch byte at
`+0xc` that Source does not have. `in_forward` is `0xd4151a8` (`+forward` `0x6fecd0`,
`-forward` `0x6fed50`); the others are listed in `capture/drive.cpp`.

- Press: a key already in a slot is a repeat and ignored; otherwise it takes the first free
  slot (a third key is ignored). If the button was not already down, `state |= 3` and the
  latch is set.
- Release: the key leaves its slot; once neither slot holds a key and the button was down,
  `state = (state & ~1) | 4`. A `-name` with no key code clears both slots and sets
  `state = 4`.

**KeyState**, inlined for forward, back, moveright and moveleft in ComputeMove (`0x702350`):
with impulse-down (`state & 2`) it returns `0.25·impulseUp + 0.5·down`; otherwise 1 if down
without impulse-up, else 0. Then `state &= 1`. This is Source's table exactly:

| state | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 |
|---|---|---|---|---|---|---|---|---|
| value | 0 | 1 | 0 | 0.5 | 0 | 0 | 0.25 | 0.75 |

**Move values.** `forward += KeyState(forward) − KeyState(back)` and
`side += KeyState(moveright) − KeyState(moveleft)`, with no speed ConVar, no IN_WALK scale and
no normalisation (SetupMove clamps each to [−1, 1] and limits the wish to unit length). All
four KeyStates run, clearing impulses, before a gate (`CInput+8`, input enabled) decides
whether they are added. With `+strafe` (`0xd4150d8`) held, `+left`/`+right` add to side.
Up uses `cl_upspeed`; no recording has an up value.

**Button bits.** GetButtonBits (`0x705500`, CInput slot `+0x18`) sets a button's bit when
`state & 3` is non-zero **or** its press latch is set. With reset it clears impulse-down
(and down too for buttons in the clear-input mask) and the latch. CreateMove runs
ComputeMove first and GetButtonBits(reset) after, so a command's bit is "down, or pressed
since the last command": a scroll-wheel tap (press and release before the command) sets its
bit for exactly one command and moves 0.25. That is `down | impulse_down` read before
KeyState's clear.

**CreateMove** (`0x703a70`, CInput slot `+0x20`; CInput at `0x22d0710`, vtable `0x13ffce8`):

- The command slot is `sequence % 750`, reset by `0x8a6910` (frametime 0).
- Angles: the engine's view angles (engine slot `+0x1d0`) after keyboard turning
  (`0x702180`), ComputeMove, and the mouse (`0x7027d0` → `0x706a00`).
- Buttons: GetButtonBits(reset) once per command. When IN_SPEED (`0x8000`) or IN_DODGE
  (`0x1000_0000`) is set and was not in the previous command's buttons (`CInput+0xb4`), the
  command also gets `0x2000_0000`; while zoomed (`+0x1881`) that edge clears IN_SPEED.
- Gamepad only: while sprinting with a controller active (`0x6fd070`), forward repeats the
  previous command's.
- The seed is set last (`0x8ac6d0`, see `docs/camera.md`).

**Frametime.** ClientModeShared::CreateMove (`0x5ccde0`) writes `ucmd+0x270` = the frametime
CL_Move passes, when `cl_move_use_dt` (live 1). CL_Move (`0x2c1a70`) is detoured in this
image: its first instruction jumps out of the image into R5Reloaded's SDK, so the frame loop
that runs is not in the dump. The image's own body (dead here) clamps to 0.1 × `host_timescale`
and makes no command below 0.005 s, which the recordings contradict (most of their frametimes
are 2.86 to 5 ms). Recorded frametimes run from 0.002857 to 0.0675. RunCommand (`0x718460`)
copies `ucmd+0x270` into `gpGlobals->frametime` with no clamp and no sign check
(`sv_clampPlayerFrameTime` live 0).

**ExtraMouseSample** (`0x702990`, CInput slot `+0x28`) also runs ComputeMove and
GetButtonBits(reset), so a frame that runs it between two commands would consume a tap. No
direct caller is in the image (its client DLL slot is `+0xe8`); with one command per frame
it does not run between them. Unverified until a recording with real key presses.

**Mouse** (`0x706a00`): raw counts from the input system (slot `+0x150` of `0xd413e30`),
as integers. Scale (`0x706560`) = `mouse_sensitivity` (live 2.25; retail default 5), times,
only while zoomed, a per-scope scalar and tan(fov/2)/tan(base fov/2) (`0x5d4300`); times
`m_acceleration`'s power curve when set (live 0). Then (`0x7067d0`): yaw −= 0.022 × scale × dx,
pitch += 0.022 × scale × dy (−0.022 with `m_invert_pitch`), with `+strafe` or `lookstrafe`
turning them into side and forward instead (`m_side`, `m_forward`). Pitch is clamped
(`0x702090`) to the class's `pitchMaxUp`/`pitchMaxDown` (settings `0x814`/`0x818`, 89 and 89
for the pilot) in the stored view angles. Yaw is not wrapped there. For the local player,
mouse motion against the weapon punch (`+0x20bc`) absorbs it (`0x806650`) and the absorbed
part goes in `ucmd+0x18`; with no weapon punch the motion passes through unchanged.

**Clock** (end of RunCommand): the player keeps a tick count `+0x1910` and a remainder
`+0x1914`. Each command adds its frametime to the remainder, moves whole
`interval_per_tick` (0.05) into the count (remainder floored at 0), then quantises the
remainder to 0.05/1023 s with `cvtss2si` (round half to even); curtime `+0x1d18` is
count × interval + remainder, all in f32. This reproduces every one of the 62,638
consecutive recorded calls bit for bit, both the player's fields and gpGlobals curtime.
The game shell's `Clock` does not use it yet.

Not recovered: where `view_fov` becomes a frustum (whether it is 4:3 Hor+); the SDK's CL_Move.

## Things that only showed up in replay

- PlayerMove picks the stance block once on entry; unducking mid-call keeps the ducked
  stance's speeds for that call.
- Float evaluation order matters: plane push-off measures against the rounded destination,
  and slide steering removes `v·d·(1/|v|²)`, not `v·(d/|v|²)`. Either changes outcomes by a
  rounding residual that normalises into a full-length wish direction.

- AngleVectors uses the engine's own SIMD sine/cosine (`0x42c3a0`: reduction by pi/2 in three
  parts, then minimax polynomials). The platform's sin/cos differ by an ULP often enough to
  break exact replay.
- powf (`0x11c57c0`) is matched by rounding the double-precision power to float.
- The sprint-allowed test uses the command's view angles (`+0x2188`, via `0x856c10` and
  `0x86b530`) when `+0x3a92` and `+0x3a93` are set, not the quantised eye angles at `+0x1ebc`.
  Found by fuzzed input: the quantisation flips the test when facing is near perpendicular
  to velocity.
- `+0x41c` is the previous call's final velocity (FinishMove).
- A call starts from the player's abs origin and velocity (`+0x14c`, `+0x140`), which
  ProcessMovement copies into CMoveData; the CMoveData read before the call can be stale.
- `along_slope` (`0x8162a0`) lifts its target as `e.z + (along.z·k + 0.01)`; the decompile
  prints `(e.z + along.z·k) + 0.01`. On a failed slope move it traces into the caller's trace.
- Slip (`slipSpeed`, `slipAcceleration`, `0x808e40`) only acts while the player touches
  entities listed at `+0x2ac8`; on plain ground its direction is zero and it does nothing.

## Ghidra caveat

A raw image loaded writable makes Ghidra print some double constants as `0.0`. `dec.sh` marks
memory read-only; a literal `0.0` or `1.4e306` that looks wrong is still checked against the
assembly.

## Open

- World collision. The engine's sweep (`0x2dc940` setup, `0x2db2b0` BVH walk, per-primitive
  handlers in tables at `0x13007f0`/`0x1300660`/`0x13006b0`) is not ported; `src/world.rs` is
  a Source-style stand-in. On the arena floor it gives 99.98% of logged endpos bit-exact and
  76% of fractions; the rest differ in the last bits.

- Friction and acceleration internals: `0x817c90`, `0x817ad0`, `0x826540`, `0x808d10`.
- `0x8049e0`: a speed multiplier. It returns 1.0 unless the entity from `0x7d0870`
  (probably the active weapon) sets one at `+0x1a14`.
- The constant drag term at settings `0x7e4` has no name in the registry.
- Jump, keyboard redirect (lurch, tap strafe), crouch and slide are ported and verified.
- Mantle: traversal, movetype 12. Not ported; the move needs animation data (below).

## Mantle (not ported)

- `player_move` runs, for movetypes 2 and 7 and unless within `traversal_cooldown` of the
  last traversal (`+0x273c`), the automantle check `0x811650` and then `0x810960`; either
  starting a traversal runs `0x8139e0(this, 1)` instead of the walk move. Movetype 12 runs
  `0x8139e0(this, 0)`.
- `0x811650` returns early on the ground with world up unless jump is held, and within 0.25
  s of leaving the ground; otherwise it sweeps the hull forward along the view yaw by
  `automantle_searchdist` or the speed (the two sweeps by `automantle_forwarddist` seen in
  air calls), finds the ledge with `0x80f360`, and sets the traversal up (`0x812770`,
  `0x8100f0`): type `+0x26e8` (3 below, 4 level, 5 above, 6 high; +4 ducked; 11, 12 ends),
  progress `+0x2730`, start time `+0x2734`, ledge point `+0x2724`.
- `0x8139e0` advances progress at `(1 − h·k)/mantleDuration*` per second by type, and with
  `traversal_anim 1` places the origin by the first-person model's bone transform for
  sequence `0x2fe + type` at that cycle (`0x7c2bf0`, `0x1d6c80`, then matrix products).
  With `traversal_anim 0` it would use `0x866eb0` instead. At progress 1 it ends
  (`0x815210`), returns to movetype 2 and, if moving forward, leaves at sprint or run
  speed along the wish. Jumping out early (`automantle_jumpoff_*`) uses `wallrunJumpUpSpeed`
  and `wallrunJumpOutwardSpeed`.
