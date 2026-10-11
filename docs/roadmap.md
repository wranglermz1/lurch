# Roadmap

The goal: an offline game for practising Apex movement, where movement and camera behave
as the game's do, proven by replaying recordings from the real engine. Pick work from the
top. Each item names how it is checked.

## 1. Movement still to port

- **Mantle animation types.** The mantle is ported and exact on its recordings
  (`docs/oracle.md`, Mantle). Between start and exit the position is the first-person
  model's root motion, recovered from recordings by `src/bin/rootmotion.rs` for the types
  recorded so far (3, below, and 5, above). Record mantles of the other types on the
  workstation (level, high, ducked, hanging at rest, jumping off) and rerun the tool; until
  then they follow the procedural path, which ends in the same place. The curves are the
  movement of the arms model's `ACT_VM_MANTLE_*` sequences, which the game shell now loads
  from the install: porting Studio_SeqMovement (`0x1d6c80`) would give every type from the
  model, with the recorded curves as its gate.
- **Superglide.** Mantle plus a jump in the frame the mantle ends. Record one; the jump-off
  and end paths it runs through are ported.
- **Ziplines.** Ported from the image (`docs/oracle.md`, Zipline) and on the test map, but
  never recorded. Rebuild the capture DLL (it now snapshots the ridden zipline into each
  call) and record the takes `docs/capture.md` lists; each should replay exact call by call
  and chained. Then: the server's rope physics if the recorded rope points move while
  ridden, and the viewmodel's mount sequence length if `+0x29bc` is not `mountZiplineTime`.
  The recordings then also gate where the zipline sounds' callbacks fall.
- **Grapple.** Pathfinder's tactical is ported from the image (`docs/oracle.md`, Grapple)
  and bound to Q in the game shell, but never recorded. Record the takes `docs/capture.md`
  lists, as Pathfinder; each should replay exact call by call and chained, and the takes
  settle what the image leaves open: whether the anchor checks' velocity changes are really
  dropped (`0x80c030` copies the velocity at `0x80c352`, before the unwrap and the anchor
  checks, and writes the copy back to `mv+0x124` at `0x80c4c8`, so the pull's starting
  impulse never reaches the move and moving away faster than `grapple_detachAwaySpeed` lets
  go at once; the server's copy does the same: its update `0xb8ceb0` copies the velocity at
  `0xb90201` and writes it back at `0xb902a3` around `0xf9dcf0`, whose start of the pull
  `0xf98690` works on the global move data), and when and along what the ability fires
  (`attackParams.dir`). The reach is measured from the eye every tick (`0x9d57d0`); the
  grapple data holds no firing point, and playing R5Reloaded did not show a hook reaching
  past that. Playing it never showed a hook that caught and then let go either, which the
  image predicts when moving away along the rope faster than 500 as the pull starts: a take
  doing that would settle it. The hook catching a zipline (`0xa12820`) and the grab it leads to,
  the camera's grapple roll (`0x6851f0`), the sounds, the arms' grapple animation, the rope
  (`0x6c0810`) and the crosshair's in-range ring are ported too; a take grappling a zipline
  and one at a wall would check the first-person animation state and the roll. Not built:
  the proxy's node table (transitions are found by the sequences' nodes), the RUI asset
  behind the crosshair (ours is a ring), `P_grapple_sparks`.
- **Superjump.** On the tech list, not yet recorded.
- **Climbing slopes.** Every climb recording is on a vertical wall. The ported rules climb
  any surface too steep to stand on (the test map's 46° and 50° ramps, then mantle onto
  the top), but not 60°. Record a run at a steep slope to confirm.

Any session can decompile these (`tools/dec.sh`, once Ghidra is installed). Write what
is recovered into `docs/oracle.md` as behaviour, not decompiler output.

## 2. Game shell

Built in the cloud (M1 to M9 of `docs/game-shell-plan.md`, which holds its status): `game/`
plays the test map in a window at one command per frame, from raw input with per-command
button edges, through `Pilot::step` and `Player::view_frame`, with the camera from
`Pilot::eye`. Scripts drive it headless or under xvfb to the same result. Next: M10, play and
measure on the workstation (its checklist is in the plan).

## 3. Camera feel

How the game's camera answers movement: FOV scale when sprinting and sliding
(`cl_fovScale`, ability FOV scaling), view bob when walking, the landing dip, strafe roll,
view punch, shake (`shake_viewmodelFactor_*`, `viewmodelShake*`), and the camera through a
climb and a mantle. This is client code in the same image (`C_BasePlayer` view setup,
`CalcView` and what it calls). First recover it as a spec, then port it. Check: a later
capture hook records the engine's view setup each frame (eye origin, view angles with
punch, FOV, view matrix), and a replay gate compares ours to it as the movement gate does.

Done: the state the movement call keeps (sprint tilt, view punch springs and the jump,
landing and wall kicks, sprint transition times, eye height through a duck) and the view
setup's frame update (slide tilt, slide FOV ease) are ported and exact on the recordings.
The camera is composed from them: `Player::eye_position`, `camera_angles` (punch and
roll) and `view_fov`. Next: sprint shake, and the camera step jolt a footstep adds
(`cockpit_stepJolt_*` in `0x67d3e0`, `docs/feedback.md`), on the `Footstep` cue. On the
workstation, record the command's random seed so kicks can be drawn in replay, and hook the
view setup's output to check what is composed.

## 4. Sound and animation assets

The framework is in (`src/sound.rs`, `src/anim/`, spec in `docs/feedback.md`), and the game
data now comes from the user's own R5Reloaded install at run time (`docs/assets.md`): the
legend's settings and sound names, the sound events through the game's own Miles, the fists'
viewmodel and its sequences, the arms' mantle and climb sequences, the zipline sounds.
`cargo run --release --bin cues -- --r5r INSTALL RECORDING` names every sound and viewmodel
activity with the legend's own names. Still to do:

- Surface property names by index (`Surfaces`), and surface indices for map geometry: every
  surface plays as `default` now.
- The player model's sequences for the body (`AnimSet` over the legend's body model): the
  body still runs on `NoAnims`.
- Recording the client's own animation state (the body fields at `+0x1c30`–`+0x1c48`, the
  viewmodel's sequence, cycle and layers, the sequence and pose parameters) would let a gate
  check the body and viewmodel as the movement gates do. Until then they are ported, not
  verified.

## 5. Viewmodel

The legend's arms on the fists' viewmodel, read from the install, skinned on the CPU with
every vertex weight, drawn by a second camera (`order: 1`, `RenderLayers::layer(1)`) at the
legend's `viewmodelfov`. The fists' weapon logic, sequence transitions, overlay layers and
their sounds are ported (`src/anim/viewmodel.rs`, `src/anim/transition.rs`). Still to do, in
order:

- The first-person proxy's arms are drawn through a mantle and a wall climb, with the view
  they carry (`docs/feedback.md`, First-person proxy). Left: retail's mantle clips
  (`mantle_*_NEW_linear`, build 6ab42c93), which would need retail's paks read at run time;
  the zipline mount (`ACT_VM_ZIPLINE_MOUNT`); the `ledgeAngle` pose parameter.
- Placement: CalcViewModelView (`0x62a940`) with sway and bob from the fists' script, and the
  view of the camera bone (`0x626110`, `0x62a100`). The viewmodel sits at the eye now.
- The oracle's own bone concatenation and bone merge (Source's forms are used).
- Materials: the arms draw their albedo, normal map and ambient occlusion through Bevy's
  standard material. Gloss, spec, emissive and cavity are read but unused, and the game's
  own shading is not ported.
- A Windows build runs in the analysis guest (`docs/setup.md`); the real game is never
  launched there.

## 6. Collision polish

- About 1 ms per trace against a full map mesh; keep the area around the player in memory.
- The engine leaves a hull about 0.014 units closer to triangle leaves than the 1/32 skin
  in `src/world.rs`. The kernel is reached through the handler tables at `0x1300660` etc.
- On the arena floor the 1/32 skin is already right: every logged floor hit (34,130 of them)
  ends where `World` puts it. Only about a third of the fractions match to the bit: the
  engine's numerator is off from the exact distance by ~1e-6, as if the hit were solved in a
  local frame of magnitude ~10 (Respawn's quantised collision vertices relative to a node
  origin). Matching that needs the map's collision tree, so it waits for the workstation;
  it is the drift `chain_world` tolerates within `TOL`.

## Later

Real map props (S3 rpak model collision), more maps. The test map's props collide with their
drawn triangles until the models' collision is read.
