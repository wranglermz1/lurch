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
| `0x0a8` | ground plane normal, initialised to (0, 0, 1) |
| `0x0d4` | max speed (`0x804af0`) |
| `0x0f0` | f32[3], accumulated by the ground move |
| `0x118` | origin |
| `0x124` | velocity |

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
| `0x81ebb0` | `set_ground` | Landing slowdown (`0x869a80`) |
| `0x805710` | (surface) | Surface friction ×1.25, capped at 1, by the trace's surface index |

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

## Ghidra caveat

A raw image loaded writable makes Ghidra print some double constants as `0.0`. `dec.sh` marks
memory read-only; a literal `0.0` or `1.4e306` that looks wrong is still checked against the
assembly.

## Open

- Friction and acceleration internals: `0x817c90`, `0x817ad0`, `0x826540`, `0x808d10`.
- `0x8049e0`: a speed multiplier. It returns 1.0 unless the entity from `0x7d0870`
  (probably the active weapon) sets one at `+0x1a14`.
- The constant drag term at settings `0x7e4` has no name in the registry.
- Duck, slide, wallrun/climb, mantle (traversal movetype 12) and superglide: not ported.
- Jump and keyboard redirect are ported but no recording exercises them yet.
