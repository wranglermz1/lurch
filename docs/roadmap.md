# Roadmap

The goal: an offline game for practising Apex movement, where movement and camera behave
as the game's do, proven by replaying recordings from the real engine. Pick work from the
top. Each item names how it is checked.

## 1. Movement still to port

- **Slope jump bookkeeping.** `jumped_recently` diverges on slopes. Recordings:
  `traces/pending/terrain_turn.ltr`, `terrain_fuzz.ltr`. Fixable from the recordings alone.
- **Wall climb.** `0x81ae40`, wall contact `0x81ca50` / `0x81bb60`. A climb makes the wall
  the ground: up vector `+0x266c` turns to the wall normal and the non-floor branch of
  `0x818320` runs with the `climb*` settings. Recordings: `traces/pending/climb_*.ltr`.
- **Mantle.** Movetype 12. Entry `0x811650` (automantle) and `0x810960`, ledge finder
  `0x80f360`, move `0x8139e0`. `+0x262c` (the retail `m_traversalState` offset) stays 0
  through a mantle in this build; the real offset is still to be found. Recordings:
  `traces/pending/mantle_*.ltr`.
- **Superglide.** Mantle plus a timed jump. Record once mantle replays.
- **Superjump.** On the tech list, not yet recorded.

Any session can decompile these (`tools/dec.sh`, once Ghidra is installed). Write what
is recovered into `docs/oracle.md` as behaviour, not decompiler output.

## 2. Settings without a recording

Stance speeds, accelerations, gravity, the `climb*` / `wallrun*` / `slide*` settings and the
movement ConVars are read from each recording today. Build them in, so a simulation can run
from nothing. Check: the replay gates still pass when fed the built-in values instead.

## 3. Test map

A plain map built in code (`World::from_tris`): a large floor, walls of several heights
(too tall to climb, climbable to the top, chest-high for a mantle), boxes, ramps and slopes
at known angles, a long hill to slide down. Real map props are deferred.

## 4. Game shell

Window, mouse and keyboard into usercmds (per-command button edges, so a scroll-wheel tap
strafe works), first-person camera, the map drawn flat-shaded. Needs a windowing and
rendering crate; ask before adding one.

## 5. Camera feel

How the game's camera answers movement: FOV scale when sprinting and sliding
(`cl_fovScale`, ability FOV scaling), view bob when walking, the landing dip, strafe roll,
view punch, shake (`shake_viewmodelFactor_*`, `viewmodelShake*`), and the camera through a
climb and a mantle. This is client code in the same image (`C_BasePlayer` view setup,
`CalcView` and what it calls). First recover it as a spec, then port it. Check: a later
capture hook records the engine's view setup each frame (eye origin, view angles with
punch, FOV, view matrix), and a replay gate compares ours to it as the movement gate does.

Leads from the sound and animation work (`docs/feedback.md`): the landing view kick
`0x807f70` (`viewkickFallDist*`, seeded random, into the punch fields at `+0x20b0`), the
footstep jolt in `0x67d3e0` (`cockpit_stepJolt_*`), the sprint view offset `0x8585d0`
(`sprintViewOffset`, by `sprint_fraction`) and tilt `0x85ffc0` (`sprinttiltMaxRoll`), the
duck view height in `0x8214c0`. The landing kick and step jolt run inside movement, so their
fields may already be in the recordings.

## 6. Sound and animation assets

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

## 7. Viewmodel

First a stand-in model driven by the recovered sway and bob. The real arms and weapon need
the game's models and animations, which are art and stay out of scope until asked for. The
viewmodel's answer to movement (sprint loop, jump and landing overlays) is in
`src/anim/viewmodel.rs`.

## 8. Collision polish

- About 1 ms per trace against a full map mesh; keep the area around the player in memory.
- The engine leaves a hull about 0.014 units closer to triangle leaves than the 1/32 skin
  in `src/world.rs`. The kernel is reached through the handler tables at `0x1300660` etc.

## Later

Real map props (S3 rpak model collision), more maps.
