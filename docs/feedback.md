# Sound and animation: recovered behaviour

How the oracle (build 5dba2a2f-02ae8471) answers movement with sound and animation, as far
as movement drives it. Addresses are RVAs into `dumps/current.bin`. This is a behavioural spec
written from the decompiles; `src/sim/step.rs`, `src/sound.rs` and `src/anim/` implement it.

The split matters for verification:

- **Inside the movement call.** Footstep timing, the wall-impact step, fall velocity, landing
  bookkeeping and the sprint-blend timestamps run in `ProcessMovement`, change player fields,
  and are gated bit-exact by the replay tests like the rest of movement. Each place where the
  oracle calls out to sound, animation or rumble raises a `Cue` (`src/sim/cue.rs`).
  `cues_fall_where_the_oracle_acted` checks the cue stream against the oracle's own record.
- **Client frame code.** The body's animation state, the viewmodel, the slide and freefall
  loops run outside prediction. They are ported from the decompiles but nothing records them yet.

What the game files supply, and a later session loads: the player settings' sound strings, the
surface property names, the sound events behind the names, and the models' sequences. Until
then names with empty parts are silent (as in the game) and `NoAnims` plays nothing.

## Cues

| Cue | Oracle | Sound | Animation |
|---|---|---|---|
| `Footstep` | `0x859840` (timer), `0x67d3e0` (sound) | `<footstep_type>.<gait>.<surface>` | none for the local player |
| `Jump` | `0x809cb0` | `<footstep_type>.jump.<surface>` | event 6 (wall-relative up: 9) |
| `Land` | `0x820fe0` → `0x820790` | (follows as `LandSound`) | event 11–14 by height |
| `LandSound` | head of `0x820790`, next call | `<footstep_type>.land|heavyland.<surface>` | |
| `Crouch`, `Stand` | `0x821610` (duck) | `sound_standToCrouch_1p`, `sound_crouchToStand_1p` | |
| `SlideBoost` | `0x821da0` (start slide) | `sound_jumpjet_slide_start_1p`, rumble `rumbleOnSlideBegin` | |

`0x804c10(player, name_1p, name_3p)` is the shared emitter: the local player hears the `_1p`
name; the `_3p` one is the server's to send to others. Names built from `footstep_type` gain
`.stealth` when the settings' `stealthSounds` is set, and `_3P` / `.stealth_3P` for others.

## Footsteps

`0x859840`, called in PlayerMove after categorize and the airborne fall-velocity update,
before duck. State: `m_flStepSoundTime` (`+0x25f0`, ms), `m_nStepside` (`+0x1a38`), the probe
handle `+0x1a24`.

1. A probe queued last call resolves now: if the player is not dead, not sliding, the step
   side flips and the footstep sounds. The oracle's probe is an async job; every recorded step
   resolves on the next call.
2. If the timer is above 0 it runs down by `frametime × 1000 × rate`, where `rate` is 1 at or
   above 90 % of the stance's `speed × speedScale` and `0.5 + 0.5 × speed / full` below
   (speed horizontal, or 3D when the up vector is not the world's). Still above 0: done.
   Below 0: clamped to 0.
3. A step is due when: flags have neither `0x10` nor `0x20`, two unidentified player states
   (`+0x1764`, bit 0 of `+0x2cac`) are clear, movetype is not 9 or 10, speed
   (3D) is at least the stance's `footstepMinSpeed`, the player is on the ground, horizontal
   speed (3D when wall-relative and rising) is above 1e-4, and a ground surface is known
   (`+0x3a58`). The timer gains `footstepIntervalSprint` when sprinting or wall-relative (if
   that is positive), else `footstepInterval`, plus 100 when ducked. A probe goes out:
   a box of half-width `ground_trace_hull_radius` (12) swept from 0.9 hull heights above the
   origin to 200 below, mask `0x640403b`.

Stance block fields: `footstepInterval` `+0x30` (int), `footstepIntervalSprint` `+0x34`
(int), `footstepMinSpeed` `+0x40`. Recorded values: stand 400 / 275 / 30, crouch 450 / 475 / 20.

The sound (`0x67d3e0`): gait `down` while bleeding out, else `wallrun` when wall-relative,
else `crouch` when fully ducked (`traversal_state` 2), `sprint` when sprinting, `jog`.
Format `%s.%s.%s` of `footstep_type` (settings `+0x328`), gait and the surface's name. It also
adds the camera step jolt (`cockpit_stepJolt_origin` / `_angles`, ×2 sprinting, ×0.5 ducked,
×(1 − 0.9 × ADS fraction)); not ported (camera, roadmap 5).

**Wall impact** (`0x81aad0`, in the walk move after the ground or air move): horizontal speed
lost over the move (with gravity's direction removed) above half the settings'
`impactSpeed` (380) restarts the timer at 400 and sounds a footstep on the last ground surface
at once (`0x81a930`). Its other effects (a script impact event, a speed-scaled call at
`0x67dcc0`) are not ported.

## Landing

Fall velocity (`m_flFallVelocity`, `+0x1a68`) is `−v.z` whenever the player is airborne:
RunCommand's pre-think writes it (`0x8702d0`), and PlayerMove again after categorize. While
standing, a networked copy (e.g. 0.03125) can arrive between calls.

At the end of the walk move (`0x81a080` tail-calls `0x820fe0`):

1. The landing probe queued last call resolves: `land`, or `heavyland` if the landing was
   hard; a hard landing also raises the `GameMovementImpactEvent` (landing impact effect).
2. On the ground and not dead: below 80 (400 for titans), fall velocity is zeroed and nothing
   happens. Otherwise (projected onto −up when the up vector is not exactly world up, and
   reduced by a downward-moving ground's speed) it is a landing. A probe goes out as for a
   footstep; `+0x19cc` keeps the speed if at least `√(2 g · 12 · hardFallDist)`, else 0.
   Fall velocity is zeroed.
3. `0x820fe0` then: the landing view kick (`0x807f70`, camera, not ported), a landing
   notification to the weapons, the animation event by height and `+0x25c0` = the speed.
   Height thresholds are `√(2 g · 12 · landDist*)` (feet): below Low → 11, below Medium → 12,
   below High → 13, else 14. ConVar `enable_height_based_land_anims` (1) gates this.

Recorded settings: `hardFallDist` 20, `landDistLow/Medium/High` 3 / 11 / 33,
`viewkickFallDistMin/Max/MaxScale` 10 / 70 / 12.

## Jump, duck and slide sounds

- Jump (`0x809cb0`, before leaving the ground): `<footstep_type>.jump.<surface>` on the last
  ground surface; `.skip.` instead when the jump is a skip and ConVar `skip_sounds` is set (0).
  With `wallrun` set, also `sound_jumpjet_jump_{start,body,finish}`; with `boostEnabled`,
  `sound_boost_*`. Both are off in every recording.
- Duck (`0x821610`): `standToCrouch` when the duck begins (before the slide check), and
  `crouchToStand` when it ends from down (with `end_slide`).
- Slide (`0x821da0`): when the boost block runs (enabled, cooldown passed), the boost sound and
  `rumbleOnSlideBegin`, whether or not the boost added speed.

## Slide loop

Client frame (`0x682650`). While the local player slides on the ground and is alive, a probe
goes out every frame (same shape, up = world up) and resolves a frame later. The first surface
starts `<sound_slide_prefix>.start.<surface>_1p`; a different surface stops it and plays
`.bump.`; leaving the slide stops it and plays `.stop.`. `slideEffectTable` drives the dust
particles; not ported.

## Freefall loop

The end of PlayerMove (`0x8221b0`), first prediction only. On the ground (or zipline, or
attached grapple): with a world up vector, stop the air loops (the body stops, `finish` plays).
Airborne and not on a ladder, falling, with no air loop running, and (`+0x307c` ≥ 1 s ago or
`+0x26e0` < 1 s ago; both unidentified, taken as true):
- if last call was already falling: start after `freefall_sound_autoplay_time` (1 s) since
  leaving the ground;
- if this is the first falling call: start if a ray `freefall_sound_height` (200) down hits
  nothing.

Starting plays `sound_freefall_start_1p`, then loops `sound_freefall_body_1p`, with
`sound_freefall_finish_1p` kept for the stop. Last call's velocity is kept at `mv+0xd8`.

## Sprint blend

`0x865240`, 0–1, read by the sprint view offset (`0x8585d0`, `sprintViewOffset`) and client
code. Sprinting: `min(max(0, (t − (sprintStartDelay + intent)) / sprintStartDuration),
(t − start) / sprintStartFastDuration) + startFrac`, clamped; not sprinting:
`endFrac − (t − end) / sprintEndDuration`, clamped. A zero duration counts as done.
`update_sprint` (`0x865510`) stamps `m_stickySprintStartTime` (`+0x2618`) when the intent to
sprint begins, and on each change `m_sprintStartedTime/Frac` (`+0x2608/+0x260c`) or
`m_sprintEndedTime/Frac` (`+0x2610/+0x2614`), the fraction rounded to 1/1023. These are sent
to the local player, so a server update replaces them with tick-stamped values between calls.
Recorded: delay 0.2, duration 0.8, fast 0.2, end 0.15.

## Body animation

The player's animation state, updated per client frame (`0x920940`):

- Eye yaw and pitch normalised; velocity yaw (UTIL_VecToYaw); moving above 0.5 units/s
  horizontal; wall-relative when the up vector is not the world's.
- Direction, from `a = eye yaw − velocity yaw`, kept until it leaves its band by more than
  `anim_playerMovementAngleMargin` (10). The four-way quadrant uses `aimAngleBackwardEnd`,
  `ForwardStart`, `ForwardEnd`, `BackwardStart` (−120, −60, 60, 120): backward outside the
  backward bounds, forward inside the forward bounds, right above, left below. With
  `quadMoveAnims` the direction is the quadrant; otherwise forward or backward at ±90.
  Sprinting forces forward. Standing still: forward unless it was backward (quad: none).
- Modifiers (`0x9208d0`, names in `tools/tables.py modifiers`): crouch (ducking or down),
  movingforward/backward/left/right, sprinting, air (no ground entity), sliding, the
  settings' `classActivityModifier`, plus weapon and ability ones not modelled.
- Main activity (`0x91f3a0`), in order: mantle (`ACT_MP_VEHICLE_DRIVER_IDLE` + traversal
  type); a jump in progress (event 6 → `JUMP_START`, until its sequence ends or 0.2 s on the
  ground); crouch (`CROUCH_IDLE`, or `CROUCHWALK` + direction); landing (`0x91e960`:
  `JUMP_LAND`, `_LOW`, `_MEDIUM`, `_HIGH` by the landing speed and `landDist*`, + direction,
  until it has played); sliding (`CROUCHSLIDE`, airborne `CROUCHSLIDE_AIR`, the body held
  at the slide's starting yaw); for 5 s after a slide, the slide-end sequence already playing,
  or within 0.1 s one picked by the eye's turn since the slide began (< −135 backward-right,
  < −45 right, < 45 forward, ≥ 135 backward-left, else left); then on the ground the
  locomotion activity, in the air `JUMP_FLOAT` once a jump or fall has gone on.
- Locomotion (`0x91f0d0`): speed is velocity along the eyes' forward and right, side divided
  by `speedScaleSide`, backward by `speedScaleBack`. At or above `1.05 × sprintspeed ×
  speedScale` (or off-forward above `1.05 × speed × speedScale`) → `SLIDE` (sliding without
  crouching); sprinting → `SPRINT`; below `0.45 × speed` → `WALK`; else `RUN`. All but
  sprint + direction. Standing-stance speeds, whatever the stance.
- Grounded for animation (`0x91e8a0`): on the ground, or in the air under 0.2 s and not
  within 0.5 s of a jump.
- An activity's sequence counts as played out (`0x91e810`) by the sequence's finished flag,
  or 0.2 s after it was asked for if a different one is playing.
- Sequence choice (`0x9200c0`): the activity's sequence under the modifiers, plus the
  weapon's.
- Body yaw (`0x921960`): standing, the body stays until the eyes are more than
  `mp_maxbodyyaw` (60) away, then turns to them; moving, it faces the eyes (quad sets: the
  movement, rotated per direction); sprinting, the movement if `sprintAnimOrientToMoveDir`.
  It eases in at `1 − e^(−7 dt)` and is then held within `aim_yaw`'s range of the eyes.
- Pose parameters: `aim_pitch` (eye pitch), `aim_yaw` (eye yaw − body yaw), `crouchFraction`
  (`0x869500`: smoothstep of the duck timer over 400 ms, 200 sliding, 200 unducking),
  `oneHandFraction`, `move_yaw` / `move_yaw_backward` (`0x921640`: movement direction against
  the body, mirrored past ±90, eased at `1 − e^(−7 dt)`).
- Playback rate (`0x91fe20`): moving on the ground, the speed over the sequence's root
  motion per second (scaled by `|cos move_yaw| + |sin move_yaw|` without quad sets), within
  `movement_anim_playback_minrate/maxrate` (0.25, 10), sprint capped at
  `movement_anim_sprint_playback_maxrate` (1.25); else 1. Rounded to eighths
  (`mp_scaleAnimationSpeeds`).
- Events (`0x91d940`): 6 jump, 7 double jump, 8 dodge, 9 wall jump (left, right or up by the
  wall's normal against the eyes) start the jump state; 11–14 do nothing here, landing is seen
  as an airborne-to-grounded change.

## Viewmodel

- Player events reach each child of the player (`0x7e84e0`): 6 and 9 → `ACT_VM_JUMP`, 7 →
  `ACT_VM_DOUBLEJUMP`, 11–14 → `ACT_VM_LAND`, `_LOW`, `_MEDIUM`, `_HIGH`, played once on
  overlay layer 1 at full weight, with the weapon's modifiers (`0xa396c0`).
- Weapon idle (`0xa439b0`): while the owner sprints and the weapon has an `ACT_VM_SPRINT`
  sequence, it plays it (one-handed: `ACT_VM_ONEHANDED_SPRINT`); otherwise, once the current
  activity's time is up, idle (`ACT_VM_IDLE`, or an alternate).

## Tables

`tools/tables.py dumps/current.bin` prints the activity table (883, `0x7ba2e0`), the
animation event table (143, `0x803d20`) and the activity modifier names (35, pointer array at
`0x131ae50`). Sequences name activities and events; code compares their numbers, so the
asset loader maps names to these numbers. Movement's activities are `ACT_MP_STAND_IDLE` (649)
to `ACT_MP_MANTLE_JUMPOFF` (778); footstep events `AE_FOOTSTEP_L/R` are 125/126.

## ConVars (live values in the image)

`sv_footsteps 1`, `ground_trace_hull_radius 12`, `skip_sounds 0`,
`freefall_sound_height 200`, `freefall_sound_autoplay_time 1`,
`enable_height_based_land_anims 1`, `mp_maxbodyyaw 60`,
`anim_playerMovementAngleMargin 10`, `movement_anim_playback_minrate 0.25`,
`movement_anim_playback_maxrate 10`, `movement_anim_sprint_playback_maxrate 1.25`,
`mp_scaleAnimationSpeeds 1`, `cl_footstep_event_max_dist 4000`.

## Not covered

- Sound: wall run, wall hang and climb sounds (`0x81ca50`, `0x81ae40`, `0x863cc0`:
  `sound_climb_{start,body,finish}`; `climbEnabled` is set in every recording), mantle,
  water enter/leave, jumpjets, grapple, zipline, rumble.
- Animation: wall run and climb activities (`0x91ecd0`), dodge, double jump, melee, weapons,
  downed, death, zipline, grapple, skydive, vehicles, stagger, IK.
- Camera, though found here: the landing view kick `0x807f70`, the step jolt in `0x67d3e0`,
  the sprint view offset `0x8585d0` and tilt `0x85ffc0`, slide FOV (`slideFOVScale`).
