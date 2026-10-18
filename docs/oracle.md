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
| `0x8653c0` | (`update_sprint`'s stop) | StopSprint: stamina bookkeeping, the sprint end fraction and time, then RaiseWeapon for weapons in the sprint state, raised as `Cue::SprintEnd` (`docs/feedback.md`, Viewmodel). Also called from `player_move` (`0x822ac2`) and `0x8615c0`, not ported |
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
| `0x81ebb0` | `set_ground` | Landing slowdown (`0x869a80`); leaving the ground clears `+0x30ed` (jumped recently), which a jump then sets again; landing leaves it |
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
| `0x866760` | `end_climb` | Up back to (−0, −0, 1), leave time stamped, the climbing arms stopped. Also run before an air move with a wall up |
| `0x866880` | `climb_arms` | From the climb branch of `0x818320` and a climb's start in wall_contact: a ray from the eye (`0x858710`) along −up, half the hull's floor diagonal plus `wallrunAllowedWallDistance`; within the allowed distance the arms start (once) and the proxy becomes the view-offset entity, else they stop (`0x866e00`). The animation side is in `docs/feedback.md` (The climb's arms) |
| `0x870160` | `update_floor_height` | In pre-think: `+0x1d00` follows the abs origin down, and up only on walkable ground; each climb or wall jump lowers it by `wallclimb_vertical_gain_reduction` |

Player fields: up `+0x266c`, previous up `+0x2678`, climb start origin `+0x2684`, climb
count `+0x2690`, same wall `+0x2694`, push-away timer `+0x26a4`, retry `+0x26b0`/`+0x26b4`/
`+0x26c0`, climb start time `+0x307c`, leave time `+0x3080`, repel wall `+0x1bb4`
(count `+0x1bd0`), floor height `+0x1d00`, climbing arms on `+0x276c`, view-offset entity
`+0x2860` (a handle; −1 for none) with lerp in `+0x2864`, lerp out `+0x2868`, a flag
`+0x286c` and the blend `+0x2870` the view setup moves between calls. Landing on world-up ground clears the repel wall
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

**Autosprint.** `player_setting_autosprint` (registered at `0xe06f0`; image default 0, live 1
in the dump) is read near the end of GetButtonBits, after the button bits. When the local
player's `+0x2600` is 0, the mode test (`0x703a00`) decides: false if `player+0x2a70` is
set; mode 1 true; mode 2 true unless the settings `generalClass` (`*0x23880e0`) is 1 or 3;
mode 3 true only for those; anything else false. `+0x2600` = 2 turns autosprint off and any
other non-zero value on (presumably `m_autoSprintForced`; unconfirmed). When on, IN_SPEED is
added if all hold:

- can-keep-sprinting (`0x865160`) passes;
- the last command the player ran (`player+0x2f50`, else the copy at `+0x2cc0`) has
  forward² + side² at least 0.8² while sprinting (`+0x2604`) or 0.94² while not, non-zero,
  and forward / its length at least 0.707;
- `now − t ≥ 0`, where `now` is the float at `*0x171f448 + 0x10` and `t` (`0x281a1268`) its value the last time those failed.
  It never blocks while the clock runs forward;
- traversal state (`+0x262c`) is not 1 or 2;
- neither IN_ATTACK nor `0x1_0000` is in the bits (GetButtonBits adds `0x1_0000` from weapon
  state just before).

CreateMove's IN_SPEED edge comes after, so autosprint's first IN_SPEED is a press. A fresh
key press reads 0.5, so autosprint starts on the second command a key is held. CreateMove
also calls the mode test while firing (`0x7041c0`) to clear IN_SPEED; weapons are not
modelled. Ported as `Player::autosprints` (`src/sim/input.rs`) for mode 1 only. Not checked
against a recording.

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
places the player by the sequence the first-person proxy plays for the type (activity
`0x236` + type, `ACT_VM_MANTLE_*` on the legend's `armsModel`; `docs/feedback.md`,
First-person proxy; `0x2fe` + type, `ACT_MP_MANTLE_*`, only names it in the warning printed
when the proxy has no sequence) at the animation cycle (`+0x1eb4`, which chases (progress − p0) / (1 − p0) at 1.5×
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
| `0xa14aa0` | `mount_start` | Ends a climb, forces standing (`+0x1a7c` 1), context action 9, then rumble `rumbleOnZiplineAttach` and `CodeCallback_ZiplineMount`. The mount time is `mountZiplineTime` (0.5 s) unless the viewmodel has a mount sequence (`ACT_VM_ZIPLINE_MOUNT`), whose length then rules: model data, as in the mantle. Target: under the use point by the standing hull's height (72), the rider hanging below the rope |
| `0xa15360` | `mounting` | Alpha runs to 1 over the mount time. The target rides along the rope as a rider would, starting at the larger of alpha · ½ `ziplineSpeed` and the grab velocity along the rope; a target the hull cannot reach keeps the last. The player closes alpha of the remaining way each call. At 1, state 3, touching ground's resets (`0x869420`), then `CodeCallback_ZiplineStart` |
| `0xa15a80` | `ride` | Speed along the rope: what the velocity carries along the current span (at least 0.01), plus `ziplineAcceleration` (400) · dt, at most `ziplineSpeed` (600) × the zipline's speed scale (`0xa0d980`). The hull hangs its height below the point reached; a blocked sweep lets go. Reaching the end of a chained rope moves onto the next (reverse: the previous) |
| `0xa0df20` | `jump_off` | Jump, crouch or crouch toggle newly pressed. Crouch: velocity eased toward straight down at `ziplineJumpOffSpeed` (400) by `ziplineJumpOffDampScale` (1, so kept), capped at 400. Jump: the same toward the view's forward at 400, then a jump's √(2 g `jumpHeight`) along up. A vertical rope pushes off along the view, flattened, at 400 |
| `0xa14eb0` | `end_of_line` | At the last rope's far end, or within the end entity's auto-detach distance of it (from the eye or the origin, the nearer), let go. Off the top of a vertical rope, push off at `ziplineJumpOffSpeed` along the end entity's facing if it pushes off, else the view's, flattened; added to the velocity if the rope preserves it |
| `0xa16060` | `detach_zipline` | Remembers the zipline and when; a view kick (salt `0x76704a58`: pitch 10 + 3r, yaw and roll 10r) unless still mounting; standing allowed again, movetype 2, context action cleared, the floor height reset to the origin's; then `CodeCallback_ZiplineStop` |
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

## Grapple

Pathfinder's tactical. Ported in `src/sim/grapple.rs`. No recording covers a grapple, so none
of it is checked against the game yet (`docs/capture.md` lists the takes).

Pathfinder plays `settings/player/mp/pilot_survival_grapple.rpak`. Of what movement reads it
differs from the base pilot only in `grapple_attachVerticalBoost` (200; the pilot's 0), so
`pilot::pathfinder()` is `pilot::settings()` with that one value (the install's two assets,
diffed). `grapple_swing` is off in both.

The ability's script (`mp_ability_grapple`) only calls the Grapple native with the aim
(`attackParams.dir`); everything else is player code. The weapon runs after the move in
RunCommand, so a shot lands in the player state the next call starts from. The weapon
entity caches its grapple settings: `grapple_power_required` `+0x27c4` (100),
`grapple_power_use_rate` `+0x27c8` (0), `grapple_maxLength` `+0x27cc` and
`grapple_maxLengthVert` `+0x27d0` (850 each); the found-by-`grapple_weapon` flag is `+0x27c0`
(`0xa5e580` walks the six weapon handles at `+0x16dc`). Without a grapple weapon the lengths
read 10. The capture does not record the weapon; `grapple::WEAPON` holds the weapon file's
values.

The grapple data is a struct at `+0x2770`:

| Off | Field |
|---|---|
| `+0x2778` | hook velocity while it flies |
| `+0x2784` | rope points, four of 12 bytes, from the player's end; the last used is the hook |
| `+0x27b4` | point count (0: no hook) |
| `+0x27b8`, `+0x27b9`, `+0x27ba`, `+0x27bb` | attached, pulling, swinging, retracting |
| `+0x27bc` | retract fast: set on letting go, cleared by a miss |
| `+0x27bd` | the pull has touched the ground (`grapple_touchGroundOnAttach`) |
| `+0x27c0` | power used this pull |
| `+0x27c4`, `+0x27c8`, `+0x27cc`, `+0x27d0` | fired, pull started, caught, let go (curtime) |
| `+0x27d4`, `+0x27d8` | auto-aim and melee target handles (−1) |
| `+0x27dc`, `+0x27e0` | pulling fast enough along the rope, and since when not |
| `+0x27e4`, `+0x27e8` | swing length, swing hold time (−1) |

Elsewhere: grapple active `+0x2800`; window-hint flag `+0x2801` and hint handle `+0x2804`;
hook entity `+0x3140`; suit grapple power `+0x1a64`. CMoveData `+0xe4` keeps the velocity the
anchor checks left (below).

| RVA | Ours | Behaviour |
|---|---|---|
| `0x86d160` | `Player::grapple` | The Grapple native. With the hook out: if `grapple_detachOnGrapple`, let go once `…DebounceTime` (0.5) has passed since firing. Otherwise refuse a zero aim or what `0x86c6b0` refuses (below); then active, count 1, the hook at the eye (`0x858710`) moving at `grapple_shootVel` (2000) along the aim, flags and power used cleared, fire time stamped, fast set, swing state reset; `sound_grapple_fire_1p` (`0x804c10`). `0x86c740` then bends the aim to a window hint the shot would miss. Last, without the climbing arms (`+0x276c`) and in first-person animation state 0 or 5, the proxy plays `ACT_VM_GRAPPLE_START` then `ACT_VM_GRAPPLE_IDLE` (`0x9d5290`, below), picked with the player's activity modifiers. Ours also clears the hook entity's zipline here: nothing in the image clears it but the hook's constructor (`0x9d6409`), and a stale one would grab the last zipline at the eye |
| `0x86c6b0` | `Player::may_grapple` | Refuses a traversal (movetype 12), a blocked traversal (`+0x2c7c`), a first-person animation state other than 0 or 5 (`+0x2a4c`), and suit power below the weapon's `grapple_power_required`. Its other refusals (`+0xa2e`, `0x870860`, `0x870920`, `0x8709e0`) are states ours never enters |
| `0x80c030` | `grapple_think` | PlayerMove, after the duck. First, with the grapple active and its hook entity holding a zipline (hook `+0x1300`), the grab from the grapple (`0xa13ab0`, below). A duck or crouch-toggle press (`mv+0x2c` & `0x4000004`) lets go, as does `+0x2184`. While pulling, the swing check (`0x80b420`; nothing without `grapple_swing`) runs its hold timer. Then: the velocity is copied; unwrap; on the anchor its checks; flight while not attached and not retracting with at most 3 points, else retract. Then CMoveData `+0xe4` takes the velocity, `+0x19d0` clears, active becomes count > 0, the copy is written back (what the anchor checks did to the velocity survives only in `+0xe4`), and a positive vertical velocity leaves the ground |
| `0x80b320` | `grapple_params` | Gathers the weapon's four values with `grapple_detachLowSpeed*Threshold`, `grapple_detachLength{Min,Max}` and the sound names |
| `0x9d3430` | `grapple_unwrap` | For each bend: when the eye (or the bend before) sees the point after it, and from 2.5 off the bend toward the nearest point of that line the line is clear too, the bend is dropped |
| `0x9d3d20` | `grapple_anchored` | Not yet pulling: fast. Pulling: past `grapple_gracePeriod` (0.25) after the pull's start, power is used at the weapon's rate; fast while the speed along the rope tops `grapple_detachLowSpeedThreshold` (250). On losing fast, its time is stamped. If `0x9d2c20` says let go: let go with the speed loss. Else, not yet pulling and `grapple_pullDelay_human` (0.2) after the catch: start the pull |
| `0x9d2c20` | `grapple_should_detach` | In a context action other than 6 or 7 while pulling. The nearer of origin and eye to the rope point (at most the third) farther than the longer max length + `grapple_detachExtraAllowedLength` (256). Closer than `grapple_detachLengthMax` (50) and either `…Min` (33) or within a margin growing with the speed along the rope (over `grapple_speedRampMax_human`, 400): arrived, and a pull gets up to `grapple_impactVerticalBoost`, capped at `…MaxSpeed`. Then, pulling, `0x9d2680` |
| `0x9d2680` | `grapple_pull_over` | Looking away from the rope (and, looking down, also flat away from the point) ends the pull unless the velocity climbs past what `+0xe4` kept; so does moving away faster than `grapple_detachAwaySpeed` (500). Moving away adds `grapple_detachVerticalBoost` (200) up to `…VerticalMaxSpeed` (200). Not fast for longer than `grapple_detachLowSpeedTime` (1.5) in the air, `…GroundTime` (0.7) on world-up ground or `…WallTime` (1.2) on a wall ends it too |
| `0x9d1970` | `grapple_start_pull` | Pulling, not swinging; on a parented hook or one holding a zipline, power used is the weapon's `grapple_power_required`, taken from the suit's power (at least 0); start time stamped; the initial impulse (`0x9d1610` → `0x9d7b40`), which leaves the ground; a zipline ridden is let go |
| `0x9d7b40` | `grapple_impulse` | The velocity across the rope kept (vertical × `grapple_initialSlowFracVert_human`, 0.4); along it raised by `grapple_initialImpulse_human` (350) but only to `grapple_speedRampMin_human` (50); then `0x9d7960` |
| `0x9d7960` | `grapple_off_ground` | Standing: the velocity into the ground undone and `grapple_initialImpulseOffGround_human` (50) added along up |
| `0x9d57d0` | `grapple_fly` | The hook moves by its speed · dt, stopping at the max length (an ellipse of `grapple_maxLength` and `…Vert`). A ray (`0x4640400b`) from the hook: no hit, and at the length, let go (a miss: the fast retract flag clears). A hit within `grapple_detachLengthMin` of the eye, or on contents `0x800`, lets go. Else catch |
| `0x9d1b00` | `grapple_catch` | Attached, not retracting, targets cleared. The first catch stamps its time and raises the velocity to `grapple_attachVerticalBoost` (a negative boost caps it). The repel wall count clears. A catch while retracting starts the pull at once; a new anchor while pulling only pushes off the ground (`0x9d7960`). It moves the hook entity, not the rope's points. On a zipline (the hit entity's vfunc `0x4e0`) the hook moves to the point of the zipline's rope nearest the hit, its fixed ends pulled in as the use search pulls them (`0xa14210`), and the hook entity remembers the zipline (`+0x1300`) instead of parenting to it. The impact effect is `grapple`, or `grapple_disallowed` when a trace with mask `0x30` from 100 back along the shot to the hit is blocked (no surface here carries those contents) |
| `0xa12820` | `zipline_on_ray` | A hook's ray (the flight's, the falling reel-in's, and the crosshair's) catches a zipline: of the enabled ropes within half the ray's length + 30 of its middle (`0xa0f850`), not the one ridden nor the last one within `grapple_lastZiplineCooldown` (0.5) of letting go, each span of this frame's rope (cut, when it reaches within 10 of the eye, to its part inside that sphere, or skipped when it passes through) against the ray (`0x42fb90`): where they pass within 30 and a ray between the two points is clear (mask `0x4640400b`), the ray ends at its point nearest the rope, normal toward the ray, fraction scaled, contents 0, the zipline its entity. Parallel lines use the ray's start against the span. Later spans compare with the shortened ray |
| `0xa13ab0` | `grapple_zipline_grab` | The zipline grab with the grapple flag: the grapple active, its hook entity holding a zipline, the hook within 120 of the eye or the abs origin (the nearer), and a clear standing hull from the abs origin to the hull's height under the hook (`0xa0c930`). Then, unless it is the zipline ridden: the grapple lets go (`0x86c5b0`), any zipline is let go, and the grab is the use's (state 1, from the origin with the abs velocity, the use point at the hook, the ride direction by `0xa0cc10`) |
| `0x9d4760` | `grapple_retract` | The hook reels back through the bends at `grapple_forcedRetractVel` (3000) after letting go or with bends, else `grapple_retractVel` (6000). Passing a bend while wrapped (not after letting go) catches there. At the eye the hook is gone (count 0). Reeling in after a miss it falls by `grapple_fallSpeed` (300) and can still catch (ray `0x4640420b`) |
| `0x86c220` | `grapple_detach` | Let go: speed loss (`0x86bfc0`: above `grapple_detachSpeedLossMin` (460), lose `grapple_detachSpeedLoss` (300) but not below it) when asked and pulling; retracting fast, not attached or pulling or swinging, let-go time stamped; after an attachment the floor height (`+0x1d00`) is reset to the origin's. Then the first-person animation: with the grapple active, no climbing arms, state 0 or 5, no scripted grapple animation (`+0x2a50`) and a proxy, `ACT_VM_GRAPPLE_STOP` (`0x9d5290`); otherwise it ends (`0x9d5650`) |
| `0x9d5290` | `Player::grapple_anim_play` | The grapple's first-person animation on the proxy (`+0x313c`): the climbing arms stop (`0x866e00`); the proxy plays the sequence, through the node graph's transition first when its exit node is not the sequence's entry (`0x5f9d40`), and is shown; bodygroups `glove_default` 0 and `glove_animated` 1 when the model has both (Pathfinder's arms have neither); the view-offset entity becomes the proxy (`+0x2860`) with lerp in 0 and out 0.3, `+0x286c` cleared; state 5 (`+0x2a4c`), `+0x2a50` cleared; the sequence after it queued (`+0x2a54`, and `+0x2a58` behind it when a transition plays first) |
| `0x9d5650` | `Player::grapple_anim_end` | In state 5: state 0, the proxy hidden with its transitions dropped and the glove bodygroups back, the view-offset entity let go (`0x869fc0`), the grapple let go when a scripted animation said to (`+0x2a50`), the queue cleared, and the arm's aim cleared (`+0x3118`–`+0x3124`, docs/feedback.md, Grapple). Also run by start_traversal |
| `0x86ef40` | `Player::grapple_anim_think` | PostThink, state 5: when the view-offset entity is not the proxy, or the grapple is neither active nor reeling in (`+0x27bb`, which stays set after the hook is back until the next shot), or the proxy's sequence has finished (`+0xa2e` clear), the queued sequence plays (`0x9d5290`) or, with none, the animation ends. Ours ends at once on the first two; the proxy's finishing is the animation layer's (`Proxy::grapple_think`), which ends the state through the shell |
| `0x8615c0` | `Player::grapple_power_regen` | PostThink: once `grapple_power_regen_delay` (3 s) has passed since the pull started (`+0x27c8`), the suit's grapple power gains `grapple_power_regen_rate` (10) a second, up to 100 |
| `0x6851f0` | `Player::grapple_roll_target` | The view's roll toward the rope, which the view roll (`0x85ffc0`) takes in place of the zipline lean while the grapple is active (`+0x2800`), on the same spring: from the eye to the rope's last point, weighted 0 to 1 over `grapple_rollDistanceMin`..`Max` (0..150) and, by how far that point is off the view, over `grapple_rollViewAngleMin`..`Max` (0..65°) (RemapValClamped `0x427190`); the lean that brings it overhead as for a zipline (`0x685b20`), at most `grapple_rollMax` (20°), × `grapple_rollStrength` (1) |
| `0x674cb0` | `Move::grapple_in_range` | Client, each frame, for the crosshair's `grapple_in_range` (and the times it last turned in and out, `grapple_in_range_time`, `grapple_out_of_range_time`): with the grapple weapon's `hud_grapple_indicator` (`0x86c610`) and `0x86c6b0`, the ray queued the frame before from the eye along the eye angles, 227023.36 long, mask `0x640400b`, through `0xa12820`, must end on contents in `0x4640400b` (a zipline counts) and not `0x800`, nearer than both the longer of `grapple_maxLength` and `…Vert` and the ellipse of the two along the aim, and not in a no-grapple volume (`0x9d5170`); moving away from it (abs velocity · aim < 0) shrinks the ellipse by that speed over `grapple_shootVel`. The distance is the fraction × 227023.36344968397 in double. A new ray is queued each frame |
| `0x86c5b0` | (`grapple_drop`) | Player vfunc `0x870`: the same on the player's abs velocity, which FinishMove overwrites |
| `0x80caa0` | `grapple_release` | The same on the move's velocity |
| `0x80cb20` | `grapple_pull` | Walk move, after climb_think: while pulling, `0x9d8270`. Standing, a pull not into the ground plane (≥ −0.3, or ≥ −0.001 off world up) leaves it. Pulling into the last plane (or, standing with none, up) by more than half: try stepping around it, by command number & 3 up 0.4 · hull height, either side 0.4 · half width, or down 0.15 · height, at one and two steps; the first clear, a push of `grapple_around_obstacle_accel` · 4 · dt. Its result tells the air move's slope continuation and categorize the player is leaving |
| `0x9d8270` | `grapple_pull_velocity` | Across the rope the velocity loses `grapple_decel` (425) · dt, sparing what falls (`grapple_dontFightGravity`) and what points along the view (`0x858e70`); along the rope it gains `grapple_accel` (1500) · dt up to the ramp; the vertical never ends below where it started |
| `0x9d7f60` | `grapple_speed_ramp` | `grapple_speedRampMin` (50) to `…Max` (800) over `…Time` (1.5) from the later of catch and pull start |
| `0x9d77e0` | `grapple_rope` | The rope: from `step_height` + 1 above the origin to the first point, plus `grapple_lift` (25) up |
| `0x808550` | `grapple_gravity` | Gravity while pulled: `grapple_gravityFracMin` (0.25) up to `…Max` (0.7) as the input pushes under the rope, or full when the rope hangs more than `grapple_letGravityHelpCosAngle` (0.8) down |
| `0x8089e0` | `grapple_gravity_scale` | half_gravity scales by `0x808550` while pulled, and by `grapple_detachGravityScale` easing out over `…Time` after letting go, below `…Speed` up; with the time 0 it never applies |
| `0x80dd50` | `grapple_wrap` | End of PlayerMove. A ray from the eye to the first point blocked both ways (by > 0.001) puts a bend where the two hit planes meet near the hit (`0x42fb90`, then a ray from the eye), up to four points; a fourth makes the hook reel in. Contents `0x800` let go first |

Elsewhere: the air move pulled uses `grapple_airSpeedMax` (420) and `grapple_airAccel` (650)
and no air drag; a jump pulled is × `grapple_jumpFrac` (1); a toggle-duck press does nothing
while attached; a climb start (`0x81bb60`) refuses a wall the rope pulls away from; the
automantle (`0x811650`) refuses while the hook is out; starting a traversal (`0x8100f0`)
lets go.

ConVars (the dump's values): `grapple_shootVel` 2000, `grapple_retractVel` 6000,
`grapple_forcedRetractVel` 3000, `grapple_fallSpeed` 300, `grapple_lift` 25,
`grapple_gracePeriod` 0.25, `grapple_pullDelay_human` 0.2,
`grapple_detachExtraAllowedLength` 256, `grapple_speedRampMax_human` 400,
`grapple_speedRampMin_human` 50, `grapple_initialImpulse_human` 350,
`grapple_initialSpeedMin_human` 0, `grapple_initialSlowFrac_human` 1,
`grapple_initialSlowFracVert_human` 0.4, `grapple_initialImpulseOffGround_human` 50,
`grapple_dontFightGravity` 1, `grapple_letGravityHelpCosAngle` 0.8,
`grapple_gravityPushUnderContribution` 2, `grapple_around_obstacle_accel` 1000,
`grapple_jumpFrac` 1, `grapple_autoMantle` 0.25, `grapple_autoMeleePredict` 0,
`grapple_windowCheckDist` 150. Several are read through a ConVar's parent pointer
(`obj+0x48`, value at `+0x68`).

Settings: `grapple_detachOnGrappleDebounceTime` `0xa88`, `grapple_swingType` `0xa90`,
`…swingGravity` `0xa94`, `…swingHoldTime` `0xa98`, `…swingMinLength` `0xa9c`,
`…attachVerticalBoost` `0xac0`, `…detachVerticalBoost` `0xac4`, `…detachVerticalMaxSpeed`
`0xac8`, `…detachGravityTime` `0xad0`, `…Scale` `0xad4`, `…Speed` `0xad8`,
`…detachLengthMin` `0xadc`, `…Max` `0xae0`, `…detachAwaySpeed` `0xae4`,
`…gravityFracMin` `0xae8`, `…Max` `0xaec`, `…detachSpeedLoss` `0xaf0`, `…Min` `0xaf4`,
`…detachLowSpeedThreshold` `0xaf8`, `…Time` `0xb04`, `…WallTime` `0xb08`, `…GroundTime`
`0xb0c`, `…SwingTime` `0xb14`, `…impactVerticalBoost` `0xb18`, `…MaxSpeed` `0xb1c`,
`…speedRampMin` `0xb20`, `…Max` `0xb24`, `…Time` `0xb28`, `…swingingSpeedRamp*`
`0xb2c`–`0xb34`, `…accel` `0xb38`, `…decel` `0xb3c`, `…airSpeedMax` `0xb48`, `…airAccel`
`0xb4c`, `…detachOnGrapple` `0xe14`, `…touchGroundOnAttach` `0xe15`, `…OnDetach` `0xe16`,
`…swing` `0xe18`, `…swingSticky` `0xe19`, `…lastZiplineCooldown` `0xb50`,
`grapple_power_regen_delay` `0xb40`, `…rate` `0xb44`, `grapple_rollStrength` `0xaa8`,
`…rollMax` `0xaac`, `…rollViewAngleMin` `0xab0`, `…Max` `0xab4`, `…rollDistanceMin`
`0xab8`, `…Max` `0xabc`; `generalClass` `0x7a8` (1 and 3 take the titan ConVars). Names from
`tools/settings.py`. `grapple_disableWeaponsWhenActive` is set for every legend: the
weapons go down (`0x7d0dc0`, applied per command by `0x854bc0` through `0x7d1430`) while the
grapple is active, not reeling in, and its hook is more than 128 from the eye.

A catch on an entity (`0x9d1b00`): after placing the hook (`0x61e9a0`) and turning it to face
away from the eye (`0x61eca0`), a hit entity with a model (`+0x38`) that is not a zipline
becomes the hook's parent (`0x61ae90`), so the hook moves with it; the attach callback
(`CodeCallback_OnGrappleAttach`, `0x1723338`) runs with it. Nothing in the catch pulls the
entity, and the script only counts stats and plays chatter. Ours parents the hook to a
`world::Body` (`Player::grapple_parent`) and moves the rope's last point with it; how the image's
rope follows a parented hook is not read.

Not ported: swinging; window hints (`0x86c740`, `0x9d1760`, the `+0x2801`/`+0x2804`
branches of `0x80cb20`, `0x9d2c20` and `0x808550`), no-grapple volumes (`0x9d5170`) and
contents-`0x800` entities that let the hook pass, moving brush anchors, auto-aim and
auto-melee targets, jump pads (`0x8095e0`), status effects (`0x89d7c0`), touching the ground
on attach or detach (both off), the scripted grapple animation (`PlayGrappleAttachedAnimation`,
`0x9d5460`, `+0x2a50`), the weapon's cooldown (script ammo), and the hook entity's own think
and effects (`P_grapple_sparks`). The debounce reads `gpGlobals+0x28`, which ours takes as
curtime. Nothing records a grapple: all of this is ported, not verified.

Player fields: first-person animation state `+0x2a4c` (5: the grapple's; also 1, 3 and 4,
not identified), scripted flag `+0x2a50`, queued sequences `+0x2a54` and `+0x2a58`; the hook
entity's `m_grappleZipline` is its `+0x1300`.

## Melee

A punch. The push it gives on the ground, the lunge toward a target, the blow's trace and the
knockback are ported in `src/sim/melee.rs` and `Pilot::melee_start_activity`/`melee_blow`; the
game shell starts and ends the attack around them. No recording holds a punch, so none of this
is checked against the game: the replay gates only check that the push stays zero and the lunge's
fields stay cleared without one (`melee_push` and the `lunge_*` fields are modelled).

The script starts the attack: the attack button with the fists out presses melee
(`attack_button_presses_melee`), and `CodeCallback_OnMeleePressed` (`sh_melee.gnut`) checks
`AttackerCanStartMelee` (alive, no attack on, not freefalling, not lunging) and calls
`PlayerMelee_StartAttack(PLAYER_MELEE_STATE_HUMAN_KICK_ATTACK)`. The state constants are
registered at `0x9dd628`: NONE 0, HUMAN_KICK_ATTACK 1, TITAN_MELEE_JAB 5. The melee attack
weapon is the active weapon if it is melee, else the offhand melee (`melee_pilot_emptyhanded`,
`melee_anim_1p_number` 2). Script callbacks are called through closure slots in a table at
`0x1722f60` (32-byte entries: name, signature, flags, closure): OnMeleePressed, Held and
Released from the melee buttons' handler `0x9dd940`, OnMeleeStartActivity from `0xa5b180`,
OnMeleeAttackAnimEvent from the weapon's anim-event handler `0xa54850` (a switch from event
72; `AE_MELEE_ATTACK` is 115) and from `0x856610`.

| RVA | Ours | Behaviour |
|---|---|---|
| `0x868280` | `Player::start_melee` | Stops the climbing arms (`0x866e00`), sets attack on (`+0x2c7c`), counter `+0x2c78` += 1 (mod 16), start time `+0x2c80` = curtime, hit entity `+0x2c84` = none, `+0x2c7e` = sprinting, `+0x2c88` = 0, `+0x2c7d` = 0, `+0x2c94` = 1, clears the lunge (`0x868f80`), state `+0x2c90`. A state above 8 only logs. |
| `0x868350` | `Player::end_melee` | Attack off, start time 0, `+0x2c94` 0, clears the lunge, state 0, `+0x1bdd` 0. Inlined in the weapon's holster and switch paths (`0x7cffd0`, `0xa3e710`, `0xa59870`). |
| `0x826540` | `Move::melee_push` | Below. Called once from the floor path of `0x818320` (`0x819908`), after friction and acceleration. |
| `0x868750` | `Player::lunge_time` | The lunge target's flag (`+0x1a0c` bit 11) clear or no target, attack on and a weapon at `+0x1720` with `melee_lunge_time` above 0: that time; else 0.5. |
| `0x868820` | `LUNGE_NO_TARGET_RANGE` | Attack on and a weapon at `+0x1720`: its `melee_lunge_no_target_range`; else the player setting `melee_cone_trace_range` (`0xc30`). |
| `0x868890` | `Player::lunge_accel` | 0 when the lunge time is at or below 0; else 2 × range / time², the division in double. |
| `0x868920` | `Player::lunge_set_target` | `Lunge_SetTargetEntity`. Needs the offhand melee weapon (`+0x2f50`); clears the lunge (`0x868f80`); refuses a target other than one set by `0x86d110`. Target `+0x3adc`; start offset `+0x3af0` = abs origin − the target's origin; end offset `+0x3afc` = the start offset over its 3D length (at least `TINY`) × (half the larger of the player's hull x and y sizes (`0x7dc590`) + half the larger of the target's collision x and y sizes), z 0; start time `+0x3b08` = curtime; abort `+0x3b0c` −1; duration `+0x3b18` from `0x868750`; `+0x3b14` from `0x856db0` (not read by the move) |
| `0x868f80` | `Player::lunge_clear` | Target none, to-position flag `+0x3ae0` 0, target position `+0x3ae4`, offsets zero, start 0, abort −1, flying `+0x3b10` and the byte after it 0, `+0x3b14` 0, duration 0.5, `+0x3b1c` −1, max end speed `+0x3b20` 0 |
| `0x869210` | `Player::lunge_active` | A position lunge, or a target that is alive (`+0x730` clear); and `+0x3b1c` ≤ 0 or curtime − start ≤ it; and no attack on, abort ≤ 0, or curtime (`+0x1d18`) ≤ abort |
| `0x9dc860` | `Player::lunge_running` | Active, and dead or outside the `+0x1790`–`+0x1794` window of `gpGlobals+0x28` (both zero here). With abort ≤ 0: an attack on and curtime − start past the duration (strictly) sets abort to curtime + 0.1. With an abort set: past it, false |
| `0x822b81` | `Move::lunge_movetype` | PlayerMove before categorize: movetype 2 becomes 15 while `0x9dc860` holds (14 for `generalClass` 3, 4 with `+0x4098`); 15 goes back to 2, the lunge cleared, when it stops. Categorize and the optimized-movement check still use the movetype read before |
| `0x8257f0` | `Move::lunge_move` | Movetype 15's move. Below |
| `0x8713f0` | `Player::knock_back` | `KnockBack` (script). A velocity whose largest component is 0, or a time ≤ 0, only warns. Else it fills the slot of `m_playerKnockBacks` (`+0x4120`, four of 0x20: velocity `+8`, start `+0x14` = `+0x1d18`, end `+0x18` = start + time) with the lowest end (compared as ints), sets the abs velocity (`0x61ef30`) to the velocity, and a player on the ground leaves it: movetype 12 becomes 2, the ground entity none (`0x84c870`), `FL_ONGROUND` cleared. Movement never reads the slots |
| `0x9db3c0` | `lunge_cone_trace` | `PlayerMelee_LungeConeTrace` (script native `0xa1d3b0`). Range and angle: the player's `melee_cone_trace_range` (`0xc30`) and `melee_cone_trace_angle`, both > 0, or with a melee weapon its lunge target range (`0xa5ae80`) and angle (`+0x1a9c`). Then `0x9daf10`: life state `+0x3f0` ≥ 2; the eye (`0x869640`) and eye angles (`0x8697b0`) with the pitch moved down by (1 − |normalized pitch| / 90, clamped 0 to 1) × 15°; `0x9da6e0` gathers up to 64 entities from four lists through a filter of the cone's four planes (half the angle about up and right) and range², sorts them (`0xa1dd20`), and returns the first a ray from the eye reaches whole (fraction ≥ 1) at its `WorldSpaceCenter` or else its eye. None found and an aim target at `+0x2f50`+`0x3c`: the same again toward it with range + 1/32 |
| `0x9dbd20` | `attack_trace` | `PlayerMelee_AttackTrace` (script native `0xa1ada0`): from the eye along the eye angles' forward, `range` long. A lunge on (`0x869210` or `0x869300`), or a grapple auto-melee target (`+0x2800`, `+0x27d4`), aims at the target first (`0x9dbac0`: toward its eye or attachment; the range at least `melee_attack_trace_can_use_lunge_distance` (0.5) × the weapon's `+0x1a98`), and a hit on anything else is dropped. `0x9db670` traces the ray (length `0x9da540`); a miss tries the ray turned 5° (−0.0872665 rad) off it about the level axis (`v.y, −v.x, 0`), spun 0°, 90°, 180°, 270° about the view (`0x42c990`), then a level ray from the eye less half of `+0xb8`. The table holds the entity, position, surface, damage type, start |
| `0xa5b180` | `viewmodel::melee_activity` | The weapon's attack activity: `ACT_VM_MELEE_ATTACK1` − 1 + `melee_anim_1p_number` held to 1..3, sent through `0xa3bea0` with flags 4; then OnMeleeStartActivity. |

The push (`0x826540`). With no attack on or state 0, the push `+0x2c98` is zero. With a hit
entity and a titan's `generalClass` (1 or 3), the velocity is clamped along the view (not
ported). Otherwise on the ground, not sliding (`+0x2815`), with no hit entity, `+0x2c88` at
or below 0 and a range above 0: past the lunge time since the start (or a time at or below
0) the push is zero. Within it, with `v` = push + velocity and `f` the view's forward
(`mv+0x3c`), the speed wanted along `f` is `(clamp(forwardmove, 0, 1) × 0.5 + 0.5) × time ×
(2 × range / time²)`, at most 343 u/s for the fists' 60 units in 0.35 s. If `v·f` is below
it, `v` gains `f × accel × frametime` (accel from `0x868890`), scaled down to no faster than
the larger of `|v|` and the wanted speed, and the push takes the difference. Anything else
zeroes it. The floor path then moves by velocity + push (`0x819948`): the speed checks (the
slide's end, the 1 u/s stop) and the move use it, and the velocity keeps none of it.

The lunge toward a target only starts from script: `Lunge_SetTargetEntity` (`0x868920`) and
`Lunge_SetTargetPosition` (`0x868ca0`) have no callers but their script natives, and
`sh_melee.gnut` sets a target in OnMeleeStartActivity, on the server, only for a player or an
NPC that `PlayerMelee_LungeConeTrace` finds (the fists: `melee_lunge_target_range` 60,
`melee_lunge_target_angle` 30), unless `move_slow` is above 0.05, then locks the pitch
(`Lunge_LockPitch`) on the ground; a position only for a door kick. Its ConVars, image values:
`melee_lunge_abort_if_blocked` 1, `_abort_distance` 0.25, `_force_enable_flying` 1, `_slide` 1,
`_use_closest_distance_between_cylinders` 1, `_use_command_time` 1, `_adjust_trace_distance` 0,
`_lag_compensate_target` 1, `_dot_check` 1, `_scale_by_speed` 2.0, `_align_eye_position` 1,
`melee_cone_trace_box_check` 0.5; the server's copy of the move is `0xbaced0`.

LungeMove (`0x8257f0`). The target's position is its origin (lag-compensated by `0x686470`
between interpolation samples for a player); x and y are it plus the end offset; z is the
current z raised to the target's and lowered to the smaller of the target's centre and the
target's z + half the player's hull height (stance by `FL_DUCKING`), plus the end offset's z.
Without flying (`melee_lunge_force_enable_flying` or `+0x3b10`) z stays the current one, and
the wanted position later drops by gravity × frametime × 0.5. `f` = clamp((curtime − start) /
duration, 0, 1) + 0 (an empty span: 0 before its end, 1 after), squared; wanted = (end − origin)
× f² + origin. The velocity becomes (wanted − origin) / frametime, scaled to the stance's speed
(`speed` or, sprinting `+0x2604`, `sprintspeed`, × `+0x2cb8`; `0x869bd0`, `0x869c40`) when
faster. On world ground the wanted position is lifted along the ground normal (`+0x3e60`) to at
least 1/128 above it. A hull trace to it (`0x826fc0`, `0x82a7c0`) moves the origin there when
clear; blocked, `0x8162a0` continues along a walkable plane, then with `melee_lunge_slide` and
still at least 1 unit away a second trace and, when the part of the remaining move along the hit
plane (its length rescaled by √(d·d / (d·d − (d·n)²)), the normal part × 1.0000153) heads toward
the target (against the start offset), a slide along it. With `melee_lunge_abort_if_blocked`,
more than 0.05 s from either end: when the distance left exceeds the share still due ((1 − f²) ×
the whole distance) by more than the hull's half width (`mv+0xd0`), and the origin moved less
than (left / time left) × frametime × `melee_lunge_abort_distance`, the abort time becomes
curtime. The whole and left distances come from `0x825640` (the closest distance between the
two hull cylinders, through `0x31b700`) with `…_use_closest_distance_between_cylinders`. Last
the velocity is capped to the max end speed when that is above 0, else zeroed; the move flags
`+0x620`; and a ground probe down by the step height (`0x806cf0`) only acts with the up vector
upside down.

The blow (`CodeCallback_OnMeleeAttackAnimEvent`): with the attack on, `PlayerMelee_AttackTrace`
at the weapon's `melee_range` (fists 60); nothing hit ends it. A hit sets the attack's hit
entity, deals `melee_damage` on the server, and knocks back: `MELEE_KNOCKBACK_SCALE` 350 along
the view (negated for a player or NPC behind the attacker's centre); a player target is knocked
by it for 0.25 s. The attack ends (`PlayerMelee_EndAttack`) and the lunge clears; the attacker
takes −push, level, × 0.5 (175) through `KnockBack` for 0.25 s unless the hit is within 10 of
the feet (end z − 10 ≤ origin z) or the attacker already moves along it (normalized dot > 0;
a zero vector normalizes to zero); then `GrappleDetach` (player vfunc `0x870`, `0x86c5b0`).

Weapon settings sit in the weapon at `+0x1870` plus the setting's offset. The registry around
`0x24c6a160` (a copy at `0x25c43120`) holds 32-byte records (name, default, `offset << 32 | index << 16 | type`, 0;
types 1 int, 2 float, 3 bool, 4 string): `melee_lunge_time` `+0x224`,
`melee_lunge_target_range` `+0x228`, `melee_lunge_target_angle` `+0x22c`,
`melee_lunge_no_target_range` `+0x230`, `melee_range` `+0x240`, `melee_angle` `+0x244`,
`melee_attack_animtime` `+0x25c`, `melee_raise_recovery_animtime_normal` `+0x260`,
`melee_anim_1p_number` `+0x278` (registered default 1).

Not ported: a lunge to a position, the pitch lock, lag compensation, the cylinders' distance
(ours measures the lunge's distances between origins) and the upside-down probe; the cone's
filter and sort (ours: the centre inside the four planes and range, nearest first), its aim
target fallback; the blow's ray length (`0x9da540`), mask and level fallback (ours: `range`,
Source's `MASK_SHOT`), the grapple auto-melee target; how the attack ends after a miss (the
script ends it only on a hit; ours ends it when the attack activity has run) and the recovery
times; the weapon's own start (`0xa3bea0`); freezelook; damage (the Minecraft mod deals its
own).
