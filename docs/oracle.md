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
| `0x084` | view wish: fwd·forward + side·right, normalised only if longer than 1; the mantle reads it |
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
| `0x1a24`, `0x19c8` | footstep and landing probe job handles (`-1` = none) |
| `0x1a38` | `m_nStepside` |
| `0x1a68` | `m_flFallVelocity` |
| `0x19cc` | last landing's speed if hard, else 0 |
| `0x25c0` | last landing's speed, for the land animation |
| `0x25f0` | `m_flStepSoundTime` (ms) |
| `0x2608`, `0x260c` | `m_sprintStartedTime`, `m_sprintStartedFrac` |
| `0x2610`, `0x2614` | `m_sprintEndedTime`, `m_sprintEndedFrac` |
| `0x2618` | `m_stickySprintStartTime` (when the intent to sprint began) |
| `0x3a58` | surface data of the last ground stood on (set with surface friction, `0x805710`) |

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
| `0x2385e90`, `0x23863d0`, `0x2388050` | stance `0x30`, `0x34`, `0x40` | `footstepInterval`, `footstepIntervalSprint` (ints, ms), `footstepMinSpeed` | footsteps |
| `0x2387da8` | `0x328` | `footstep_type` | footstep, landing and jump sound names |
| `0x2387ac8` | `0x7ec` | `impactSpeed` | wall impact |
| `0x2386c98` | `0xdb8` | `hardFallDist` | landing |
| `0x2387398`, `0x2386f00`, `0x23868d8` | `0xdbc`, `0xdc0`, `0xdc4` | `landDistLow/Medium/High` | land animation |
| `0x2385b98`, `0x2385a30`, `0x23861c0`, `0x2386460` | `0xd70`–`0xd7c` | `sprintStartDelay`, `sprintStartDuration`, `sprintStartFastDuration`, `sprintEndDuration` | sprint blend |

The capture writes these values to the trace `settings` line under these names, read
the way the oracle reads them, along with `sv_gravity` (the ConVar pointed to by
`0x231ed78`, float at `+0x68`).

The `sound_*` strings (`0x3c8`–`0x778`), `classActivityModifier` (`0x320`), `quadMoveAnims`
(`0xe1e`), `sprintAnimOrientToMoveDir` (`0xe1f`) and `aimAngle*` (`0xcb4`–`0xcc0`) are read by
the sound and animation code; `docs/feedback.md` has what each does.

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
| `0x859840` | `update_step_sound` | Footstep timer and probe; called after categorize, before duck |
| `0x81aad0` | `wall_impact` | Speed lost against a wall; restarts the footstep timer |
| `0x820fe0`, `0x820790` | `land_check` | End of the walk move: landing probe, speed, animation event |
| `0x865240` | `sprint_view_frac` | The sprint blend, from the sprint timestamps |

Sound and animation around these, and the client code that turns cues into them, are in
`docs/feedback.md`.

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
- Categorize refreshes the last-ground time (`+0x3084`) only when its probe hit something and
  the player is on the ground or left it under 0.1 s ago. Walking off a ledge keeps the time
  of the last grounded call, which the mantle's 0.25 s coyote window reads.

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
- Mantle: ported (below), but for the animation's root motion between start and exit.
- Zipline: ported (below), not yet recorded.

## Mantle

The traversal movetype (12). Ported in `src/sim/mantle.rs`, verified by
`traces/mantle_sprint.ltr` (a sprint into a chest-high wall) and `mantle_climb.ltr` (a wall
climb that ends in a mantle): every oracle trace is matched, and every field is exact call
by call and chained.

| RVA | Ours | Behaviour |
|---|---|---|
| `0x8221b0` | `player_move` | Movetypes 2 and 7: with `traversal_enable` and not within `traversal_cooldown` (0.5 s) of the last traversal's end (`+0x273c`), the automantle check and then the window check (`0x810960`, which needs a traversal-window entity, so never here); either starting one runs the traversal move instead of the walk move. Movetype 12 runs the traversal move, and skips categorize |
| `0x811650` | `automantle` | Refuses with `automantle_enable` or the mantle setting (`0xdf5`) off, in a view mode or touching slip entities; standing on world-up ground, or within 0.25 s of leaving it, unless jump is held; within `automantle_cooldown` (0.25 s) of the last start. Wants a mantle with forward input ≥ 0.3 (or within `grapple_autoMantle` of a grapple); without that, needs to be moving along the view's yaw. Sweeps the hull along the view's yaw by max(`automantle_searchdist` − 0.5, speed · dt), lifting the sweep along a walkable slope; a wall it hits goes to the ledge finder |
| `0x80f360` | `find_ledge` | Needs the input into the wall and the view facing it (`0x80ed90`, shared with the climb). Probes from a height above the origin that cycles with the command number mod 7: h + 1/32 (0 and 4), and 66.5, 26.5, 16.5, 34.5 × h/80 (h is `mantleHeight`, 80); on 6 it asks for a traversal hint instead (`0x9cc140`, entities, none here) and finds nothing that call. A ducked hull swept into the wall by max(`automantle_forwarddist` 26, `automantle_mindist` 18) must get 18 past the face, then dropped by h + 1/32 land on a walkable top at least 1/32 above the origin, and a ducked hull must fit straight up from the origin to the probe height. A ledge under step height + 1/32 that a standing hull fits on is left to the step; a low obstacle a ducked hull clears at half the rise is left alone. A line 55 below the top and two hull widths into the wall finds whether there is room to hang below |
| `0x811650` | `mantle_onto` | Mid: the height at which a ducked hull swept back from the ledge reaches the origin's xy (1/64 above the top's plane, else rising 4 at a time). Begin: h + 1/32 below mid. End: the ledge top a hull width past the wall, as far as a standing hull fits (a ducked one, forcing a duck, `0x8212e0`, if standing does not), at most reach − mindist. The player starts at the higher of its origin and begin; `mid_frac` = h / (forward + half width + h), progress starts at the origin's height share of it. `+0x2768` is how far below begin a hull can hang |
| `0x812770` | `place_hands` | The ledge point: an 8-tall box probe down at either side; if one side misses, a box sweep along the edge finds `traversal_hand_required_width` of room and the point slides there; then line traces fanned over the edge bisect where wall meets top. Marks the hang flag when the hands fit |
| `0x8100f0` | `start_traversal` | The path stored in the ledge entity's frame, the ledge point floored to 1/32, the blend offset set, ground set to the ledge's entity, velocity zero, slide and jump flags cleared, the climb ended, movetype 12 |
| `0x8139e0` | `traversal_move` | Type, at the first call, by the eye against the ledge point: 3 below (eye more than 10 under it, and not behind it), 4 level (−10 to 10), 5 above (10 to 30), 6 high (30 up); 4 more ducked, 11 hanging at rest, 12 jumping off. Progress moves toward a target at (1 − ((band + h)/h) · `mid_frac`) / duration per second, the band being the type's eye height bound less the eye height (−h for below), the duration `mantleDuration*` by type (below 0.7, level 0.6, above 0.5, high 0.35). Target: 1 while pushing along the ledge's forward (view wish · forward > 0.1), or once past mid; with room to hang and no input it stops at `automantle_rest_frac_below` (0.3) or `_rest_frac` (0.4), type 11. Pulling back (< −0.3) lets go past `mid_frac` or `automantle_backoff_anim_maxfrac`. Jump (once past `mid_frac` if pushing) leaves after `automantle_jumpoff_duration` (0.1 s) or past `automantle_jumpoff_anim_maxfrac` (0.5): below mid as a wall jump (`wallrunJumpOutwardSpeed` out, `wallrunJumpUpSpeed` up), above it along the ground wish at run or sprint speed, then the jump check. At progress 1 the player stands at end, with run or sprint speed along the wish if pushing |
| `0x815210` | `end_traversal` | Every exit: the player onto the procedural path at the stored progress, movetype 2, categorize. The first-person model takes the jump as a view offset (`+0x1380`) |
| `0x866eb0` | `traversal_pos` | The procedural path: begin to mid up to `mid_frac`, then mid to end |

Between start and exit the position is not procedural. With `traversal_anim 1` each call
places the player by the first-person model's sequence for the type (activity `0x2fe` +
type) at the animation cycle (`+0x1eb4`, which chases (progress − p0) / (1 − p0) at 1.5×
the rate, p0 the type's start share): its root motion (`0x1d6c80`) placed at the ledge
point facing the ledge's forward, plus the offset from where the player stood when the
sequence started, blended out over 0.2 s (`+0x2740`, `+0x2744`). The root motion is in the
model files, not the image. Every exit puts the player back on the procedural path, so the
outcome does not depend on it; only the path the camera takes does. `src/bin/rootmotion.rs`
recovers the curve from recordings (`src/sim/rootmotion.rs`: types 3 and 5 so far); other
types follow the procedural path, as `traversal_anim 0` would. In replay the recording
supplies it.

Player fields: type `+0x26e8`, kind `+0x26ec` (0 mantle, 1 window), begin `+0x26f0`, mid
`+0x26fc`, end `+0x2708`, `mid_frac` `+0x2714`, forward `+0x2718`, ledge point `+0x2724`,
progress `+0x2730`, start `+0x2734`, hand time `+0x2738`, end time `+0x273c`, blend start
`+0x2740` and offset `+0x2744`, jump time `+0x275c`, hang `+0x2760`, back off `+0x2761`,
push latch `+0x2762`, gun lowered `+0x2764`, rest `+0x2768`, cycle `+0x1eb4`. Retail's
`m_traversal*` names fit these with a 0x488 shift. The hand time and the gun-lowered count
only drive the weapon and hands, and are not modelled.

## Zipline

Ported in `src/sim/zipline.rs` (movement) and `src/sim/view.rs` (camera). No recording
covers a zipline, so none of it is checked against the game yet; the capture records what a
replay needs (`docs/capture.md`).

A zipline is a `C_Zipline` entity whose rope runs to the entity at `+0xd50` (`m_nextZipline`):
another zipline, making a chain, or a `C_ZiplineEnd`. The rope is the polyline the entity
holds this frame, `m_currentFrameZipline`: up to 16 points at `+0xe44` (count `+0xe40`), each
span's length at `+0xfc4` (negative: measure it). The map's rest shape (`+0xd68`, count
`+0xe28`) stands in for the use search while its ends sit on the two entities. Other fields:
physics type `+0xa0c` (0 a line, 1 a vine whose end hangs loose, 2 vertical, from the
`ZiplineVertical` key), previous `+0xd4c`, `m_detachEndOnUse` `+0xd54`, `m_dropToBottom`
`+0xd55`, auto-detach distance `+0xd58`, push off `+0xd5c`, preserve velocity `+0xd5d`,
enabled `+0xd64`, speed scale `+0xe30`. A zipline_end has its auto-detach distance at
`+0xa00`, push off `+0xa04` and its zipline at `+0xa08`. Retail's datamap names fit with a
0x4c8 shift.

| RVA | Ours | Behaviour |
|---|---|---|
| `0x85e9f0` | `player_use` | PreThink, before the movement call. Use (`0x8000_2020`, of which `IN_USE` 0x20) newly pressed, or held while a press found nothing (`+0x2ca8`), or held 0.7 s after the last use (`+0x30d0`), looks for a use target; none in traversal. A zipline target with use down grabs it. A press that finds nothing sets the retry; letting go clears it |
| `0x85e8c0`, `0x85d900` | `use_target` | Not in another context action (`+0x1764`, unless on a vine), not grappling. Generic usables within 210 come first; the zipline the search finds wins unless something scores better, and nothing scores below −1 |
| `0xa10780` | `find_zipline` | From 72 above the origin: enabled ropes whose floor plan passes within 120 and whose polyline does (`0xa0f850`), not the one ridden, nor the last one within `useZiplineCooldown` (0.4 s) of letting go. Each rope's fixed ends are pulled in to where a standing hull (0.1 skin) hanging from it, swept back from 200 along it, first touches something (`0xa14210`). Of each span, the part within 130 is checked against the view ray from 7 ahead of that point (`0xc42f50`): the span is clipped to the half-space in front, and the point of it nearest the ray wins. The grab needs a clear standing hull from a step above the origin to under that point |
| `0xa0cc10` | `ride_reverse` | Which way to ride: toward the end the view faces along the rope (a vertical one toward its end unless the view is more than 120° off it when the rope runs up, or 60° off it when it runs down), but never toward an end within its auto-detach distance of the grab |
| `0xa13ab0` | `try_zipline` | State 1, the grab: from a step above the origin and the velocity then, the use point, the direction; sliding stops |
| `0x8221b0` | `player_move` | Movetypes 2 and 7 on a zipline with somewhere to go (`0x869d50`) run the zipline branch instead of the walk move: by state, mount start, mounting or riding, then the velocity check (`0x81e770`) and categorize. A no-zipline trigger (`0x80dc70`) lets go; none here |
| `0xa14aa0` | `mount_start` | Ends a climb, forces standing (`+0x1a7c` 1), context action 9. The mount time is `mountZiplineTime` (0.5 s) unless the viewmodel has a mount sequence (`ACT_VM_ZIPLINE_MOUNT`), whose length then rules: model data, as in the mantle. Target: under the use point by the standing hull's height (72), the rider hanging below the rope |
| `0xa15360` | `mounting` | Alpha runs to 1 over the mount time. The target rides along the rope as a rider would, starting at the larger of alpha · ½ `ziplineSpeed` and the grab velocity along the rope; a target the hull cannot reach keeps the last. The player closes alpha of the remaining way each call. At 1, state 3, and touching ground's resets (`0x869420`) |
| `0xa15a80` | `ride` | Speed along the rope: what the velocity carries along the current span (at least 0.01), plus `ziplineAcceleration` (400) · dt, at most `ziplineSpeed` (600) × the zipline's speed scale (`0xa0d980`). The hull hangs its height below the point reached; a blocked sweep lets go. Reaching the end of a chained rope moves onto the next (reverse: the previous) |
| `0xa0df20` | `jump_off` | Jump, crouch or crouch toggle newly pressed. Crouch: velocity eased toward straight down at `ziplineJumpOffSpeed` (400) by `ziplineJumpOffDampScale` (1, so kept), capped at 400. Jump: the same toward the view's forward at 400, then a jump's √(2 g `jumpHeight`) along up. A vertical rope pushes off along the view, flattened, at 400 |
| `0xa14eb0` | `end_of_line` | At the last rope's far end, or within the end entity's auto-detach distance of it (from the eye or the origin, the nearer), let go. Off the top of a vertical rope, push off at `ziplineJumpOffSpeed` along the end entity's facing if it pushes off, else the view's, flattened; added to the velocity if the rope preserves it |
| `0xa16060` | `detach_zipline` | Remembers the zipline and when; a view kick (salt `0x76704a58`: pitch 10 + 3r, yaw and roll 10r) unless still mounting; standing allowed again, movetype 2, context action cleared, the floor height reset to the origin's |
| `0xa0d5e0` | `rope_param` | Where along the rope, by length, the nearest point lies |

Player fields: allowed `+0x2994`, zipline `+0x2998`, last zipline `+0x299c`, detach time
`+0x29a0`, state `+0x29a8` (1 grabbed, 2 mounting, 3 riding), mount time `+0x29bc`, alpha
`+0x29c0`, start time `+0x29c4`, end time `+0x29c8`, grab position `+0x29cc` and velocity
`+0x29d8`, mount target `+0x29e4`, use point `+0x29fc`, rope parameter `+0x2a08`, reverse
`+0x2a18`, camera sway `+0x2a20` and its rate `+0x2a2c`; context action `+0x1764`, use time
`+0x30d0`, use retry `+0x2ca8`. Retail's `m_zipline*` fields fit `+0x29b0` on with a 0x5b8
shift and 4 more past `+0xc`: the oracle keeps the mount time there. Settings: `mountZiplineTime`
`0xc38`, `useZiplineCooldown` `0xc3c`, `ziplineSpeed` `0xc40`, `ziplineAcceleration` `0xc44`,
`ziplineJumpOnAcceleration` `0xc48` (rope physics, server side), `ziplineJumpOffSpeed`
`0xc4c`, `ziplineJumpOffDampScale` `0xc50`, `ziplineRoll*` `0xc54` to `0xc60`,
`ziplineViewOffset` `0xc64`, `context_action_can_use` `0xe1c`.

Not ported: vines (type 1: a detached end, which the client simulates as a rope in PreThink,
`0xa69140`), rope ziplines on keyframe ropes (`C_RopeKeyframe`, with sag), the server's rope
physics and anything it does to the points a map zipline networks, no-zipline triggers,
weapons and hands, script callbacks that can veto a use. Ziplines carry a use mask
(`+0x9e8`) the player's must meet; map ziplines are usable by all.
