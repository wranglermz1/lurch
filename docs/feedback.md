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

The game files supply the player settings' sound strings, the sound events behind the names,
and the models' sequences; the shell and `cues` read them from the user's install
(`docs/assets.md`). Surface property names are not read yet: every surface is `default`.
Without an install names with empty parts are silent (as in the game) and `NoAnims` plays
nothing.

## Cues

| Cue | Oracle | Sound | Animation |
|---|---|---|---|
| `Footstep` | `0x859840` (timer), `0x67d3e0` (sound) | `<footstep_type>.<gait>.<surface>` | none for the local player |
| `Jump` | `0x809cb0` | `<footstep_type>.jump.<surface>` | event 6 (wall-relative up: 9) |
| `Land` | `0x820fe0` → `0x820790` | (follows as `LandSound`) | event 11–14 by height |
| `LandSound` | head of `0x820790`, next call | `<footstep_type>.land|heavyland.<surface>` | |
| `Crouch`, `Stand` | `0x821610` (duck) | `sound_standToCrouch_1p`, `sound_crouchToStand_1p` | |
| `SlideBoost` | `0x821da0` (start slide) | `sound_jumpjet_slide_start_1p`, rumble `rumbleOnSlideBegin` | |
| `SprintEnd` | StopSprint `0x8653c0`, from `update_sprint` | | weapons in the sprint state raise |
| `ZiplineMount`, `ZiplineStart`, `ZiplineStop` | `0xa14aa0`, `0xa15360`, `0xa16060` | `player_zipline_attach`, `_loop`, `_detach` (below) | |
| `GrappleFire` | `0x86d160` | `sound_grapple_fire_1p` | |
| `GrappleCatch` | `0x9d1b00` | impact effect `grapple`; `sound_grapple_retract_1p` stops | |
| `GrapplePull` | `0x9d1970`, `0x9d1b00` pulling | `sound_grapple_traverse_1p` (loop) | |
| `GrappleDetach` | `0x86c220` | impact effect `grapple_extract` when attached; the pull's loops stop; `sound_grapple_retract_1p` | |
| `GrappleReset` | `0x9d4760` | `sound_grapple_retract_1p` stops; `sound_grapple_reset_1p` | |
| `GrappleAnim` | `0x9d5290`, `0x9d5650` | | the proxy's grapple sequences (below) |

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
3. `0x820fe0` then: the landing view kick (`0x807f70`, `docs/camera.md`), a landing
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

The fists (`mp_weapon_melee_survival`, model `ptpov_emptyhand`), or an heirloom in their place
(`docs/assets.md`), are the weapon the viewmodel plays; `src/anim/viewmodel.rs`. Weapon code
runs per command, in movement's order:

- Deploy (`0xa43b60`), when the weapon is drawn: nothing if already deploying (state 1).
  Otherwise WeaponIdle first, then `ACT_VM_DRAW_TO_SPRINT` if the owner sprints (none:
  stop there), else `ACT_VM_DRAWFIRST` the first time and `ACT_VM_DRAW` after. Then state 1.
- WeaponIdle (`0xa439b0`), each command: if the owner sprints and the weapon has an
  `ACT_VM_SPRINT` sequence (`0x5fa240`, no modifiers), state 8, and unless sprint is
  already the activity, send `ACT_VM_SPRINT` and set the next idle to now. Otherwise once
  the next idle time has come: state 0, and if the weapon idles or is inspecting, send
  `ACT_VM_IDLE` (or an alternate). A weapon idles (`+0x16ac`, set at `0xa3c702`) when ConVar
  `weapon_doIdleForSurvivalMelee` (image value 0) is set or its class is not
  `mp_weapon_melee_survival` (the string at `0x14b9a08`): the fists never idle, they hold the
  end of what they last played; the kunai idles.
- StopSprint (`0x8653c0`) raises (`0xa44520`, from sprint) every weapon in state 8: state 3,
  then with `weapon_sprint_raise_delay` `ACT_VM_RAISE_FROM_SPRINT`, else `ACT_VM_RAISE`,
  else `ACT_VM_IDLE`; next idle after the sequence's length.
- SendWeaponAnim (`0xa58440`, `0xa35d40`): picks the sequence (below), or returns false
  when there is none. The weapon keeps the activity playing (`+0x132c`) and the ideal
  activity and sequence (`+0x1328`, `+0x1324`). The ideal sequence again, when the ideal
  activity is an idle (`ACT_VM_IDLE` to `_ALT3`) and no transition plays, is left playing.
  While a transition plays (activity `ACT_TRANSITION`, 6) and has not finished (`0xa393d0`:
  the viewmodel's `m_bSequenceFinished`; true when the activity is 0 or −1), the ideal
  sequence again is left alone, and a sequence the transition's exit node enters at
  (`0x33dee0` against `0x33de70`) only becomes the ideal. Otherwise the new sequence becomes
  the ideal, and FindTransitionSequence (`0x5f9d40` → `0xb41910`, forward only) gives what
  plays from the sequence playing. A transition plays as `ACT_TRANSITION` unless the
  activity is a draw (`ACT_VM_DRAW` to `_DRAWCATCH`, `ACT_VM_ONEHANDED_DRAW`,
  `_DRAWFIRST`) or the viewmodel is not drawn (`0xa50fe0`: its `EF_NODRAW`); else the ideal
  plays under its own activity. The weapon's length for what plays (`0xa4c820`) is the
  script's time for its activity (`deploy_time` for draw, `deployfirst_time`, `raise_time`,
  `raise_from_sprint_time`, `sprintcycle_time` for either sprint) when above 0, else the
  sequence's own (Studio_Duration `0x1d6100`, 1 / cycles per second). The playback rate is
  own length / weapon length, at most 12 (a weapon setting at `+0x260` for the attack
  activities 484 to 491 and 593 to 600). Next idle = now + weapon length, + 0 for
  `ACT_VM_CHARGE` to `_VER4`.
- FindTransitionSequence (`0xb41910`): from the playing sequence's exit node to the new one's
  entry node (each through the rig's node remap, +1; a 0 node is replaced by studio `+0x108`,
  0 in the rigs read). Equal nodes, or either 0, give the new sequence. Otherwise, both nodes
  in the same rig, the rig's node table (studio `+0x10c`: per node an offset from the header
  to a count and that many `short` pairs, target node and rig sequence) gives the transition
  to the target, else the new sequence. The kunai's table: 1→2 `regrip_to_crouch`, 1→3
  `idle_to_sprint`, 2→1 `regrip_to_idle`, 2→3 `crouch_to_sprint`, 3→1 `sprint_to_idle`,
  3→2 `sprint_to_crouch`, 4→3 `draw_to_sprint`, 5→1, 5→2, 5→3 and 6→3 the one-handed ones;
  the fists' sequences have no nodes.
- On the viewmodel (`0x7e3530`): a delta sequence (`STUDIO_DELTA`) plays on overlay layer 0
  at weight 1 from cycle 0; any other becomes the main sequence from cycle 0, and layer 0's
  weight goes to 0.
- Player events reach each child of the player (`0x7e84e0`): 6 and 9 → `ACT_VM_JUMP`, 7 →
  `ACT_VM_DOUBLEJUMP`, 11–14 → `ACT_VM_LAND`, `_LOW`, `_MEDIUM`, `_HIGH`, on overlay layer 1:
  rate 1, weight 1, cycle 0, order 1, fading in and out over the sequence's own fade times.
  The fists' jump and plain land are single-frame all-zero deltas; `_LOW`, `_MEDIUM`,
  `_HIGH` move the hands.

Sequence choice (SelectWeightedSequence `0xb40b50`, with the modifiers of `0xa396c0`): the
current sequence stays if it has the activity and a negative weight. Otherwise each sequence
of the activity scores (`0xb40550`) one point per modifier it lists, less 1001 for each it
wants that the weapon lacks and each it rules out that the weapon has; the best wins, ties
drawn by weight from the entity's random stream. With no modifiers all are drawn by weight.

A weapon's modifiers (`0xa396c0`) are the player's flags, one byte per modifier number from
`+0x32b0` (35, numbered by the name table at `0x131ae50`, ids at `0x2371a50`), then `althand`
when the weapon's `+0x29c0` is 1, then the class modifier (`+0x3ebc` when `+0x3ebe`, the
legend's `classActivityModifier`). `0x854bc0` sets the flags each command, after movement:

| Byte | Modifier | Set from |
|---|---|---|
| `+0x32b0` | `ads` | `+0x1881` (zoomed) |
| `+0x32b1` | `crouch` | duck state `+0x262c` is 1 or 2 (going down, down) |
| `+0x32c4` | `wallrun` | `+0x2680` (z of the up vector at `+0x2678`) below 1 |
| `+0x32c5` | `wallhang` | `+0x276d` |
| `+0x32c6` | `sprinting` | `+0x2604` |
| `+0x32c8` | `air` | no ground entity (`+0x3dc`) |
| `+0x32c9` | `airfast` | no ground entity and `+0x2a48` (not identified) |
| `+0x32ca` | `sliding` | `+0x2815` |
| `+0x32ce` | `grapple` | `+0x2800` |
| `+0x32d0` | `airsprint` | no ground entity and \|`m_sprintEndedTime` − left-ground time (`+0x30bc`)\| ≤ 0.05 |

When any of them changes (or the one-handed state `+0x1bf1`, from `0x86b040`), each weapon
in state 0 gets next idle 0, so WeaponIdle sends idle again with the new modifiers, one in
state 17 sends activity `0x222`, one in state 5 `0x1d8`. The fists' movement sequences name
none of these modifiers; the kunai's name `crouch` (crouched idle), `airsprint` (draw, idle,
land, raise in the air after a sprint), `sprinting`, `air` and `sliding` (the raise into a
slide).

Client, per frame:

- Sequence transitions, for every animating entity (`0x5fc3f0`; ConVar
  `sequence_transitioner_enable`): CheckForSequenceChange (`0x898d60`) keeps the sequence
  being left (its cycle, rate and time as last recorded) when the new one does not snap
  (`STUDIO_SNAP`); the fade is the old sequence's fade-out or the new one's fade-in,
  whichever is shorter (`0xb42420`). Up to 6 are kept, the oldest dropped. UpdateCurrent
  (`0x899250`) records the sequence now playing and drops each kept one whose time + fade
  has passed, with every one older. Weights (`0x899600`), newest first: `s = 1 − (now −
  time) / fade`, clamped to [0, 1] in double, then `(3 − 2s) s²`; each weight is the product
  of its own and every newer one's.
  On the viewmodel (whose class answers true at vtable `+0x140`; the first-person proxy's
  answers false) overlay layers 0 and 1 keep transitions of their own too (`+0x16e0`, `0x1b8`
  apart), from the layer's sequence (`+0x14c0`) and parity (`+0x1a50`, which a player event
  steps, so the same sequence sent again also counts as a change). UpdateCurrent records the
  layer's cycle (`+0x1598`), rate and weight; a layer not active (`+0x1424`) records nothing.
  So a landing cut into a jump fades the jump out over the landing's fade-in (0.05 s on the
  kunai) instead of dropping it.
- StandardBlendingRules (`0x5edc60`): the main sequence; then the kept ones newest first
  (MaintainSequenceTransitions `0x899720`, ConVar `anim_transitionsequences`), each at
  `ClampCycle((now − time) × cycles per second × rate + cycle)`, skipped when the one older
  than it is at full weight or its own weight is 0; then the overlay layers in order
  (AccumulateLayers `0x7bfe80`), on the viewmodel each of layers 0 and 1 followed by its own
  kept ones (`0x7c0744`) while the layer's weight is above 0. A kept layer sequence blends at
  its transition weight alone, not times the layer's fades.
- A viewmodel layer's weight (`0x7bfe80`): its weight capped at 1; a non-looping layer with
  fades is ramped linearly in over the first `fade_in / length` of its cycle and out over the
  last `fade_out / length`, its cycle held in [0, 0.999].
- Events (`0x601a50`, `0x5f1750`): the client events (`AE_CL_*`, or flagged client) crossed
  since the last frame, on the main sequence (from −0.01 when it is new) and on each layer
  with weight (from −1). A wrap past the end of a looping cycle fires both ends.
- FireEvent (`0x5f3d50`): `AE_CL_PLAYSOUND` (35) plays its options as a name;
  `AE_CL_PLAYSOUND_FOR_TYPE` (36) plays `<footstep_type>.<options>`, the type of the player
  that owns the entity; `AE_GROUND_CONTACT_L/R` (127, 128) build third-person names only
  (`%s.%s.%s_3P`), none of which exists for a legend. So a medium landing says
  `effort_land_1p`, a high one `effort_landHigh_1p`, a low one nothing.
- The viewmodel's FOV is the legend's `viewmodelfov` (view setup `0x5d4740`; the ConVar
  `ss_viewmodelfov` is split-screen only).

Not ported: firing, reloading, switching, holstering, melee, inspecting, one-handed and
alternate activities (`0xa582a0`); the random draw among ties (the first is taken); the
modifiers `airfast`, `althand` and `onehand_was_sprinting`; the client's own frame advance
(cycles run from the time a sequence was sent, as the transitions' do, and a transition
counts as finished when its cycle reaches 1 at a command); the node table itself (ours finds
a transition among the sequences without an activity by their nodes, which gives the
kunai's table above); the viewmodel's place and its sway and bob (CalcViewModelView
`0x62a940`, which sets the viewmodel's pose parameters through `0x628f20` and calls the
client mode's `+0x40` on `0x22e3280`, not followed yet), the sprint shake (`0x626110`) and
the camera bone (`0x62a100`); the oracle's bone concatenation. Nothing records the
viewmodel, so none of this is gated.

## First-person proxy

The arms that mantle and climb are not the viewmodel. They belong to the player's
first-person proxy (`C_PredictedFirstPersonProxy`, handle `+0x313c`), an animating entity on
the legend's `armsModel` (`pov_pilot_light_wraith` for Wraith). That model has its own
traversal sequences: `ACT_VM_MANTLE_BELOW`, `_LEVEL`, `_ABOVE`, `_HIGH` (569–572), their
`_CROUCH_` forms (573–576), `ACT_VM_MANTLE_IDLE` (577), `ACT_VM_MANTLE_JUMPOFF` (578),
`ACT_VM_TRAVERSAL_WINDOW_LEFT/RIGHT`, all with no modifiers.

- start_traversal (`0x8100f0`) shows the proxy (`+0x3e8` 1, `0x9caa00`) when `traversal_anim`
  is set; end_traversal (`0x815210`) hides it (`+0x3e8` 0). ShouldDraw (`0x9c9ac0`) draws the
  proxy only while the player's movetype is 12.
- traversal_move (`0x8139e0`): on the first call and whenever the type (`+0x26e8`) changes,
  the proxy takes activity `0x236` + type (SelectWeightedSequence `0x5fa240`, ResetSequence
  `0x7c4d20`), its animation time now and its cycle kept. Type 11 (rest) then plays at rate
  1; type 12 (jumping off) at the sequence's duration over the jump-off time; every other
  type has rate 0, and each call sets its cycle to the traversal cycle (`+0x1eb4`). The
  proxy's sequence and cycle are what the player's position follows (root motion `0x1d6c80`
  on the proxy's model).
- Pose parameter `ledgeAngle` is copied from the player's model to the proxy each call.
- Events: the proxy reads the main sequence only (`0x5f15c0` → `0x5f1750`, from −0.01 when
  the sequence is new) and has no overlay layers. FireEvent (`0x9c9bf0`) is the base one
  (`0x5f3d50`) except that `AE_CL_VSCRIPT_CALLBACK` (120) can be skipped in prediction.
  Wraith's arms: every mantle sequence calls a script callback at cycle 0;
  `mantle_below` also plays `<footstep_type>.effort_wallMantleBelow_1p` at 0.375 and
  `mantle_level` `<footstep_type>.effort_wallMantleLevel_1p` at 0.469 (the crouched forms
  play neither); `mantle_jumpoff` plays `<footstep_type>.effort_mantle_1p` and `mantle_idle`
  plays `mantle_idle`, both at 0.
- The callbacks are bound in the install's `scripts/vscripts/client/cl_player.gnut`
  (SetupFirstPersonProxyEvents): `mantle_smallmantle`, `mantle_mediummantle` and
  `mantle_lowmantle` play the sound of the same name on the proxy, `mantle_extralowmantle`
  plays `mantle_extralow`. High mantles call the small one, above the medium, level the low,
  below the extra low.
- Placement, every view frame (`0x680170`, called from the local player's view setup
  `0x67eff0` before punch and tilt are added): while traversing, the proxy's transform is
  ConcatTransforms(ledge frame, inverse of its `ref` attachment in its current pose, rotation
  divided by the player's model scale `+0xf0c`). The ledge frame is AngleMatrix(VectorAngles(
  traversal forward, up)) at the ledge point, both through the ledge entity's frame
  (`0x8675f0`, `0x867390`). So `ref` stays pinned to the ledge and the arms animate about it.
  For 0.15 s after the hand time the proxy sits back by (1 − t)·16 along (f.x + 0.5·f.y,
  f.y − 0.5·f.x, f.z + 1) for a mantle (f for a window), f the view's forward and t the time
  since the hand time over 0.15; before the hand time it sits fully back. The clock is player
  `+0x1d18` plus globals `+0x2c` (curtime and 0 during movement). Outside traversal the
  proxy is placed so its `camera` attachment is at the eye (or by the player setting
  `firstpersonproxyoffset` without one), and a wall climb places it at the origin facing the
  wall.
- The view (`0x9c9c70`, right after placement): while the proxy shows a traversal activity
  (0x237–0x242) with `traversal_anim` and `view_offset_entity_enable`, the eye and angles come
  from its `camera`, `ref` and `camera_base` attachments, lerped in over
  `traversal_viewLerpInDuration` from the traversal start; afterwards the difference eases out
  over 0.2 s (`traversal_viewLerpOut`, `_Pos`, `_Angle`). Not yet recovered in full.
- Hand time (`+0x2738`): start_traversal sets it to now + 1 s when the active weapon's
  viewmodel is drawn (`0xa50fe0`: not `EF_NODRAW`) and to now otherwise; each mantle call
  pulls it to now once the gun is no longer up or the viewmodel is not drawn. Gun up
  (`+0x2764`, 1 or 0): ((`automantle_gun_enable_height` (33) − eye z) + h) / (h + hull half
  width) is above the call's target or progress. The countdown elsewhere (7 after a refused
  low ledge, decremented each non-traversal call) is overwritten before the mantle reads it.
  Recorded: `mantle_sprint.ltr` starts at now + 1 and drops to now as the gun goes down at
  progress 0.57; `mantle_climb.ltr` starts at now, because the fists are hidden on the wall.
  When the weapons go down (`0x7d0dc0`, applied by `0x7d1430`): both hands at mantle rest
  (type 11) or while the viewmodel is hidden; otherwise, with only the fists, nothing.

Retail (build 6ab42c93, its `common.rpak` read through HAVM's maptools): the shared arms
rig `animseq/weapons/arms/pov_mp_core/` holds the same clips with the same lengths and events,
plus `mantle_{above,below,high,level}_NEW_linear` and their `_crouch` forms at weight 999
against the old clips' 1, so retail plays those nearly always. Idle and jump-off are
unchanged. The retail fists (`ptpov_emptyhand`) have no mantle clips either.

Ported in `src/anim/proxy.rs`: the sequence, cycle, transitions and events, and through
`Sfx::anim_event` their sounds (`cues` prints them for a recording); the hand time, the
placement with its slide in, and the view onto `camera` with its lerp in, end clamp and ease
out. The game shell draws the arms model with that pose and placement in the world view
while the hands are in. Only the gun flag `+0x2764` is movement (`src/sim/mantle.rs`), gated
on every recording; nothing records the proxy, so the rest is ported, not verified. The decode
is checked against the recordings in one way: `REF` in the model's space at each cycle is
exactly the recorded root motion turned around (`src/sim/rootmotion.rs`, types 3 and 5).

What the R5Reloaded clips show from the view: `mantle_below` brings the hands within about
30° of the centre for cycles 0.1 to 0.4, `mantle_level` for 0.1 to 0.2, `mantle_jumpoff` at
its start and `mantle_idle` throughout; `mantle_above` (a ledge below the eye) keeps them
behind the camera. With the fists out the hands come in late (the fists are still drawn
0.1 s into `mantle_sprint.ltr`), so on a sprint mantle they show briefly or not at all, as
in R5Reloaded itself.

Inferred, not recovered: that a mantle puts the fists away with Holster (`0xa44340`:
`ACT_VM_LOWER` in half its time, hidden after) and raises them at the end (`0xa44520`).
Both R5Reloaded and retail show the fists gone through a mantle; the recordings fit the
timing (drawn at the start, gone before 1 s); which call disables them (`0x7d0dc0` through
`0x7d1430`) is not settled; a wall climb puts them away the same way. Not modelled: the
weighted draw among clips (the first is taken), `ledgeAngle` (start_traversal computes an
asin of the two vectors it is given, in degrees; it stays 0, which plays `mantle_idle`'s
middle animation), and whether a hidden proxy still reads its events. `mantle_idle` is
silent through the game's Miles.

### The climb's arms

A wall climb shows the same proxy. The climbing arms (`0x866880`) run from movement: from
the wall branch of the ground move (`0x818320`, while climbing up) and when wall_contact
(`0x81ca50`) starts a climb with `climbEnabled` and `traversal_anim`. Unless the player is
in one of the special states `+0x2a4c` 1, 3, 4 or 5 (`0x870860`, `0x870920`, `0x8709e0`,
`0x9d55c0`) or traversal is blocked (`+0x2c7c`), a ray goes from the eye (`0x858710`: the
view offset, pitch offset and zipline sway, without the step smoothing the camera adds)
along the up vector reversed, half the hull's floor diagonal plus
`wallrunAllowedWallDistance` long. If the wall lies within the allowed distance:

- the proxy's rate is 1 while the player's absolute velocity (`+0x148`, the previous
  command's) rises faster than `FLT_EPSILON`, else 0;
- pose parameter `aim_yaw` is the player's yaw less its eyes', within ±90 (with
  `wall_climb_pose_paramteter_hands_enabled`, 1 in the image); the two agree in every
  recording, so it is 0, the middle of `wall_climb`'s three-way blend;
- the first time (`+0x276c` clear): the proxy is shown (`0x9caa00`), its transitions
  dropped, `wall_climb` played from its start at rate 1 (`ptpov_wall_climb` without the
  ConVar), and the player's view-offset entity set to it: `+0x2860` the proxy's handle,
  lerp in `+0x2864` 0.2 s plus the active weapon's `+0x1e3c` (0 for the fists), lerp out
  `+0x2868` 0, `+0x286c` cleared, `+0x276c` set.

Otherwise it stops (`0x866e00`; also run by `end_climb` `0x866760`, climb_think's hang with
`wallrunAdsType` 1, not ported, and `0x868280` and `0x9d5290`, not identified): `+0x276c`
cleared, the view-offset entity let go (`0x869fc0`), and the proxy hidden unless the
movetype is 12. start_traversal sets movetype 12 before it ends the climb, so a mantle out of
a climb keeps the arms shown and blends from `wall_climb` into the mantle clip.

Every view frame the view setup (`0x67eff0`) runs `0x9cad50` for each first-person proxy
(the list at `0x2570d728`) whose owner is the local player. Each moves the owner's blend
`+0x2870` toward 1 by frametime / max(0.001, lerp in) while a view-offset entity is set, or
toward 0 by frametime / max(0.001, lerp out) when none is, clamped. The recordings move it by
twice a frame's time each frame, so the player owns two proxies. Each proxy also keeps how
far it has slid in, `+0x1358`: the larger of itself and the blend while the view holds an
entity, else the smaller (with lerp out 0, at once 0); its render offset (`0x9c93f0`,
outside movetype 12 while shown) is −(1 − that) × (`first_person_proxy_blend_distance` (50)
along the camera's forward + the eye height along its up), from the camera's last angles
(`+0x2178`).

Placement (`0x680170`) while climbing with the arms on: at the player's origin, its angles
VectorAngles(wall normal reversed, up) with the pitch taken `fmodf(pitch, 2.5)`.
ShouldDraw (`0x9c9ac0`) draws the proxy while it is the view-offset entity.

The view (`0x86a8d0`, from `0x869640` and `0x67eff0`): while the view-offset entity is set,
its `CAMERA` relative to `CAMERA_BASE` (that one's rotation scaled by 1 / model scale)
applied to the eye's matrix gives an origin and angles, kept at `+0x2874` and `+0x2880`.
`0x67eff0` then moves the eye `+0x2870` of the way there, origin by lerp, angles by
quaternion slerp (left alone when equal). So the camera follows the hands' heave once the
blend is in, and lets go at once when the climb ends.

Gated: `+0x276c`, `+0x2860` (set or not), `+0x2864`, `+0x2868`, `+0x286c` are movement fields
in every replay, and `+0x2870` is checked by the view-frame gate. The ray is matched against
each recording's trace log. The proxy's pose, placement and the view are ported, not
verified.

## Climb and zipline sounds

The climb plays `<footstep_type>.effort_climb_1p` from `wall_climb`'s event at cycle 0, each
time the looping clip comes round, and only while the player rises (the proxy's rate). The
movement-state sounds (`0x8640b0` → `0x863cc0`) would play
`sound_climb_{start,body,finish}_1p` on a change of the climb state, but `0x8640b0` passes
that state as 0 before and after, so they never play in this build.

Zipline sounds come from the server's script (the install's `pilot/_zipline.gnut`), on the
callbacks movement runs from the script tables (client `0x1723080`, server `0x1725da0`; each
entry name, signature, flags, handle): `CodeCallback_CanUseZipline` in find_zipline (a veto),
`CodeCallback_ZiplineMount` at the end of mount_start (`0xa14aa0`, server `0xfd94e0`),
`CodeCallback_ZiplineStart` as mounting completes (`0xa15360`, server `0xfd9fb0`),
`CodeCallback_ZiplineMove` each ride call (`0xa15a80`) and `CodeCallback_ZiplineStop` in
detach_zipline (`0xa16060`, server `0xfdadd0`). The rider hears `player_zipline_attach` on
Mount, `player_zipline_loop` from Start, and on Stop the loop stopped and
`player_zipline_detach`. The client's own script defines only Mount, and plays nothing.
Movement raises `Cue::ZiplineMount`, `ZiplineStart` and `ZiplineStop` there
(`src/sim/zipline.rs`); the cue gate checks them against the mount's start time, the riding
state and the detach time once a zipline is recorded. Not modelled: R5Reloaded's script
counts rides and, from the fourth without touching ground or mantling, plays no attach and
refuses the zipline (`SetPlayerCanUseZipline`); the Move callback's jet effects.

## Grapple

Sounds. The local player's are the client's (the server's copies send the `_3p` names to
others): the fire sound from the Grapple native; on the catch (`0xb90330`) the impact effect
and the reel-in sound stopped; when the pull starts (`0xf98690`) the traverse loop, or
`sound_grapple_reel_in_target_1p` on a hooked target; a catch while pulling starts it again;
on letting go (`0xbf7a70`) the `grapple_extract` impact effect at the anchor when it was
attached, the traverse, swing and reel-in-target loops stopped, and the reel-in started;
when the hook is back at the eye the reel-in stopped and the reset sound. Every legend's
settings name the same sounds: `Pilot_Grapple_Fire`, `PIlot_Grapple_Traverse_1P` (also the
swing's), `Pilot_Grapple_Retract_1P`, `Pilot_Grapple_Reset`, `Pilot_Grapple_ReelInTarget_1P`.
The impact effects are a per-surface table the game loads at run time; on a `default` surface
the local player's hook plays `Default.Grapple.Impact_1P_vs_3P` and
`Default.Grapple.Extract_1P_vs_3P`. All seven resolve through the game's Miles.

Animation. The grapple plays on the first-person proxy, not the viewmodel (`0x9d5290`,
`docs/oracle.md`, Grapple): `ACT_VM_GRAPPLE_START` (554), `_IDLE` (555, looping) and `_STOP`
(556). Pathfinder's arms (`pov_pilot_heavy_pathfinder`) hold `ptpov_cloudcity_grapple_fire`
(0.267 s, nodes 0 to 1, `P_grapple_sparks` at its camera), `_fire_trans` (1 to 2), `_idle`
(1.9 s, node 2), `_lower_trans` (2 to 3) and `_lower` (0.4 s, 3 to 0), so a shot plays fire,
its transition, then idle, and letting go the lower's transition and lower. The proxy is
the view-offset entity meanwhile, so the camera follows its `camera` against `camera_base`,
and it is placed with `camera` on the eye.

The arm points along the rope. Every view frame, after punch and the other view offsets,
while `0x9d55c0` holds (state 5, the view-offset entity is the proxy, and the grapple is
active or reeling in; also the proxy's `+0xa2e`, unidentified and taken as set), the view
setup (`0x67eff0`) turns the proxy to the view's angles and moves it so its `CAMERA_BASE`
attachment sits on the eye. It then takes the rope's first point (the grapple data's
`+0x14`, `+0x2784` on the player, which `vfunc +0x880` returns), its direction from the eye
into the view's frame (AngleIMatrix `0x42d130`), and VectorAngles (`0x42c5d0`, atan2 by
`0x2fe780`) of that, pitch and yaw each fmodf 360 and wrapped into [−180, 180]. Each is eased
toward by a critically damped spring of rate 5 over gpGlobals frametime: x = 5 dt, k = 1 /
(0.48 x² + x + 1 + 0.235 x³), and with c the value less the target and t = (5c + rate) dt,
the rate becomes (rate − 5t) k and the value (t + c) k + the target (player `+0x3118` and
`+0x311c` for pitch, `+0x3120` and `+0x3124` for yaw; `0x9d5650` clears them). The eased
pitch and yaw go to the proxy's `traversalPitch` and `traversalYaw` (their indices cached at
`+0x1350`, `+0x1354` by `0x9c9560`, which also caches the `ref`, `camera`, `camera_base` and
`grapple_rope` attachments) through SetPoseParameter (`0x5fa340`). Each grapple sequence
auto-layers `ptpov_cloudcity_grapple_poses`: a one-frame 3 × 3 delta grid over
`traversalYaw` (45 to −45) and `traversalPitch` (−55 to 45), which bends the arm toward the
point (Bone setup, below). The rig's own ranges are −90 to 90 and −45 to 45. The rope leaves its `grapple_rope` attachment
(`+0x134c`, looked up by `0x9c95ad`). The weapons go down while the hook is out more than 128
from the eye and not reeling in (`0x7d0dc0`), and come up on letting go (`0x7d1430`).

The rope (`0x6c0520` for each player with the grapple active, `0x6c0810` for one): its points
from the hook entity through the bends to the hand, where the hand is the proxy's
`grapple_rope` attachment in state 5 (remembered with the time), else the eye + 7 along the
eye angles' forward + 3 right − 12 up, eased from the remembered place over 0.3 s. Each span
is cut into 50 pieces at t = j / 51 (others' ropes: 20); each piece is the span's point at t
plus four harmonics `sin((k + 1) π t)` along two directions across the span (the horizontal
perpendicular and its cross with the span), turning at ±(k + 1) · time ·
`rope_wiggle_rotate_speed` (2, alternating sign), sized span length × magnitude ×
`rope_wiggle_harmonic_falloff` (0.75)ᵏ, and while fully tight also × sin((k + 1) · time ·
`rope_wiggle_oscillate_speed` (64)) (`0x6beb50`). The magnitude runs from
`rope_wiggle_magnitude_loose` (0.04) to `…_tight` (0.002) by 1 − (1 − q²)(1 − tightness), q
the hand-to-hook distance squared over `grapple_maxLength` squared; tightness is 1 pulling or
reeling in, rising over `grapple_pullDelay_human` from the catch, 0 in flight. The tube has 6
sides (others' 4) and width 1, drawn in `cable/grapple` with its V coordinate scaled by
`rope_texels_per_world_unit` (8) over the material's width (`0x6bf3e0`). The hook entity is
`projectile_grapple_hook_anchor_bolt`, facing away from the eye (`0x9d1b00`).

Ported in `src/sim/grapple.rs` (the cues and the player's state), `src/anim/proxy.rs` (the
sequences and the aim), `src/rope.rs` and the game shell. The proxy's node transitions are found among
its sequences by their nodes; the model's node table is not read. The game shell's own: the
hook is not drawn within 48 of the eye, where it would fill the view; the fists are not drawn
while the proxy shows; the crosshair's indicator is a ring that turns blue in range (the
RUI asset `ui/crosshair_grapple` is not read). Nothing records a grapple: ported, not
verified.

## Bone setup

How a sequence at a cycle becomes bone transforms (`src/anim/bones.rs`), in the studio
library's float forms:

| RVA | Function |
|---|---|
| `0x1c8ad0` | the animation for a blend position |
| `0x1c8c30`, `0x1c8e50` | one value, and two neighbouring values, of a compressed run |
| `0x1c9450` | CalcBoneQuaternion: 48- or 64-bit quaternions, or packed Euler angles through AngleQuaternion (`0x42e940`, its own SIMD sine and cosine) |
| `0x1c97b0`, `0x1c9b40` | CalcBonePosition, CalcBoneScale (half-precision tracks) |
| `0x1ca3b0` | CalcAnimation for one rig's sequence |
| `0x1db160` | the bones a sequence weights |
| `0x1d44b0` | Studio_LocalPoseParameter: the entity's pose parameter (kept normalised to the model's range) placed in the sequence's axis range, clamped to [0, 1], times the axis size − 1: the index (truncated) and the fraction past it; a sequence with pose keys (`+0xa0`) walks them instead |
| `0x1d4800` | Studio_SetPoseParameter: a value into the parameter's range, clamped, as the entity keeps it. lurch's entities keep every parameter at 0 but the proxy's two the grapple aims (Grapple, below) |
| `0x1d8e40` | CalcPose: the cycle wrapped or clamped (a cycle-pose sequence takes it from its pose parameter, `+0xb4`), then by the two axes' fractions s0, s1 (each within 2⁻²³ of 0 or 1 counts as on the grid point): one grid point; two along an edge, blended by the fraction (an all-zeros delta on one side scales the other instead, `0x1d41e0`); or, inside a cell, the three corners of the triangle the point is in, the cell cut from (0, 0) to (1, 1) when the indices' sum is even and from (1, 0) to (0, 1) when odd, by their barycentric weights (one under 1e-4 is dropped): the first corner blended with the second by its share of the two, then the third by its weight |
| `0x1d3770` | BlendBones (ConVar `BlendBonesMode`, 0 in the image): rotations by the sign-aligned normalised blend of SlerpBones, positions and scales linearly |
| `0x1d41e0` | ScaleBones: a rotation drawn toward identity (`QuaternionNormalize`, `0x42e090`), positions scaled, scales raised to the weight when the sequence animates scale |
| `0x1d2dc0` | SlerpBones, delta sequences included, normalising with `rsqrtps` |
| `0x1da7c0` | AccumulatePose (without the pose cache and IK): CalcPose, its local auto-layers (`0x1d9ad0`, for a sequence flagged `0x200`) added to that pose at weight 1, the blend in, then its other auto-layers (`0x1d9700`) into the result at the sequence's weight |
| `0x1d9700`, `0x1d9ad0` | Auto-layers (32 bytes at `+0x98`, count `+0x94`): the layer's sequence by its asset GUID (`+0x0`), flags `+0xc`, start, peak, tail and end `+0x10`–`+0x1c`. Between start and end the weight ramps in to the peak and out from the tail by the cycle (eased by `3 − 2s` · s² with flag `0x40`, a cross-fade past the tail with `0x80`, not times the weight with `0x200`) and the layer plays the cycle's part between them unless real-time (`0x8000`); a pose layer (`0x4000`) is also weighted by its pose parameter. Flag `0x1000` marks local layers; `0x2000` layers play only when the bone setup asks (lurch's never does). Not ported: the cycle a real-time layer takes from the time |
| `0x1d6000`, `0x1d6100` | cycles per second (frame rate / (frames − 1); 0 for one frame), duration (1 / that) |
| `0xb4cfb0` | ClampCycle: looping wraps into [0, 1); otherwise [0, 0.999] |

Constants: rotation scale `0x15bb274`, scale scale `0x15bb2c4`, Quaternion64 scale
`0x15bb224`, run lengths `0x172d9f0`, packed-frame bits and bias `0x12fc154`, `0x12fed60`.

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
`mp_scaleAnimationSpeeds 1`, `cl_footstep_event_max_dist 4000`,
`sequence_transitioner_enable 1`, `anim_transitionsequences 1`,
`cl_anim_sequence_transition_full_weight_optimization 0`,
`fire_animevents_overlay_not_active 0`, `weapon_sprint_raise_delay 1`, `sound_volume 1`,
`sound_volume_sfx 1`, `sound_volume_dialogue 1`, `sound_volume_music_game 1`,
`sound_volume_music_lobby 1`, `sound_volume_voice 1`, `miles_channels 2`,
`miles_samplerate 48000`,
`ss_viewmodelfov 54`.

## Not covered

- Sound: wall run and wall hang sounds, water enter/leave, jumpjets, rumble.
- Animation: wall run and climb activities on the body (`0x91ecd0`), dodge, double jump,
  melee, weapons beyond the fists' movement, downed, death, the zipline mount
  (`ACT_VM_ZIPLINE_MOUNT`), the grapple on the body (`ACT_MP_GRAPPLE`,
  `ACT_MP_GESTURE_GRAPPLE_FIRE`), skydive, vehicles, stagger, IK.
- Camera, though found here: the landing view kick `0x807f70`, the step jolt in `0x67d3e0`,
  the sprint view offset `0x8585d0` and tilt `0x85ffc0`, slide FOV (`slideFOVScale`).
