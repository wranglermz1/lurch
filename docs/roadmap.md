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
  then they follow the procedural path, which ends in the same place.
- **Superglide.** Mantle plus a jump in the frame the mantle ends. Record one; the jump-off
  and end paths it runs through are ported.
- **Ziplines.** Ported from the image (`docs/oracle.md`, Zipline) and on the test map, but
  never recorded. Rebuild the capture DLL (it now snapshots the ridden zipline into each
  call) and record the takes `docs/capture.md` lists; each should replay exact call by call
  and chained. Then: the server's rope physics if the recorded rope points move while
  ridden, and the viewmodel's mount sequence length if `+0x29bc` is not `mountZiplineTime`.
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

The framework is in (`src/sound.rs`, `src/anim/`, spec in `docs/feedback.md`): movement raises
cues, verified against the oracle; the sound layer names what plays; the body and viewmodel
pick activities and drive pose parameters. What it waits on is game data, which a session
with the game files supplies:

- Player settings strings: `footstep_type`, the `sound_*` fields, `classActivityModifier`
  (`PlayerSounds`, `BodySettings`).
- Surface property names by index (`Surfaces`), and surface indices for map geometry.
- The sound events behind the names, and a mixer to play them (`Mixer`).
- Sequences from the player and viewmodel models (`AnimSet`): activity and modifiers, length,
  looping, root motion, events, pose parameter ranges, matched to the numbers from
  `tools/tables.py`.

Check: `cargo run --bin cues` over a recording names every sound and activity with no
placeholder left. Recording the client's own animation state (the body fields at
`+0x1c30`–`+0x1c48`, the sequence and pose parameters) would let a gate check the body as the
movement gates do.

## 5. Viewmodel

First a stand-in model driven by the recovered sway and bob. The real arms and weapon need
the game's models and animations, which are art and stay out of scope until asked for.
In the shell it is a second `Camera3d` (`order: 1`) rendering `RenderLayers::layer(1)`, the
sun on layers 0 to 2 (it already is), and `IsDefaultUiCamera` on the main camera so the HUD
stays put (`docs/game-shell-plan.md` section 8).
The viewmodel's answer to movement (sprint loop, jump and landing overlays) is in
`src/anim/viewmodel.rs`.

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

Real map props (S3 rpak model collision), more maps.
