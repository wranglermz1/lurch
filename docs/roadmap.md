# Roadmap

The goal: an offline game for practising Apex movement, where movement and camera behave
as the game's do, proven by replaying recordings from the real engine. Pick work from the
top. Each item names how it is checked.

## 1. Movement still to port

- **Mantle.** Movetype 12. Needs the workstation: with `traversal_anim 1` the mantle's
  position each call is the traversal animation's root motion. `TraversalMove` (`0x8139e0`)
  plays activity `0x2fe + type` (types 3 to 12) on the first-person model and places the
  player by its bone transform, so the sequences' keyframes have to come from the game's
  model files (or a capture hook that logs the root offset per call). The rest is portable
  from the image and is mapped in `docs/oracle.md`. Recordings: `traces/pending/mantle_*.ltr`.
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

## 6. Viewmodel

First a stand-in model driven by the recovered sway and bob. The real arms and weapon need
the game's models and animations, which are art and stay out of scope until asked for.

## 7. Collision polish

- About 1 ms per trace against a full map mesh; keep the area around the player in memory.
- The engine leaves a hull about 0.014 units closer to triangle leaves than the 1/32 skin
  in `src/world.rs`. The kernel is reached through the handler tables at `0x1300660` etc.

## Later

Real map props (S3 rpak model collision), more maps.
