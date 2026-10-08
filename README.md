# lurch

Apex Legends movement, reimplemented in Rust, in our own engine: an offline game for
practising movement. The goal is behavioural equivalence with the real game on the full
movement tech list (strafe accel, air control, lurch and tap strafe, slide and slide hop,
superjump, mantle, wall climb, superglide, jump grace and multi-jump penalty) and on the
camera's answer to movement (FOV scaling, bob, punch, shake). `docs/roadmap.md` is the
order of work.

The purpose is educational: recovering a movement model from a shipped binary, and proving
the recovery with a differential test.

## Definition of done

"1:1" is a number, not a feel. A recorded input stream replayed through our simulation
produces the same per-tick origin and velocity as the real engine, inside a stated
tolerance. Without a gate that checks it, no fidelity claim is made.

The oracle is the game's own code, not gameplay footage. R5Reloaded ships without
anti-cheat, so the movement function can be hooked in-process and called with synthetic
state, then diffed against ours. Build 5dba2a2f-02ae8471; `docs/ground-truth.md` lists
the sources.

## Fidelity reached

`cargo test` replays every recording two ways, and both must be bit-exact on origin,
velocity and all 48 modelled player fields:

- call by call, each call starting from the oracle's state;
- chained, the simulation carrying its own state from call to call for whole recordings.
  Server updates of the networked sprint timestamps are taken from the oracle.

The fields include the footstep timer, fall velocity, landing and sprint-blend bookkeeping.
The cues movement raises for sound and animation (footsteps, jumps, landings, crouch, slide
boost) must fall on the calls where the oracle acted. The sound and animation layers built
on them run outside movement and are not checked against the oracle yet; the game's sound
events and sequences are not loaded (roadmap 6).

Recordings: walking over World's Edge terrain; on mp_rr_arena_empty jumps, sprint jumps,
bunny hops, lurch, tap strafe, air turning, crouch, crouch jump, air duck, slide, slide hop,
slide jump; and seeded random input (`tools/fuzz.py`). Still from the oracle each call:
settings, ConVars, view angles. Wall climb and mantle are not ported.

Collision: `src/world.rs` sweeps the hull against triangles (in memory, or a HAVM `.mesh`).
Every mp_rr_arena_empty recording also replays chained with our own collision against its
floor, exact to `TOL`. No mesh exists for any R5Reloaded map, so collision beyond a flat
floor is unverified.

## Approach

The engine is Source-derived, so the base layer is known ground: trace-and-clip player
move, a bump loop with velocity clipping, step-up, the 2-unit ground snap. That layer is
written from the published Source behaviour. Ground and air acceleration are not Source's,
and neither are the Respawn additions (lurch, slide, traversal, wall climb); those are
recovered from the binary.

Collision is a swept hull against level geometry, not a rigid-body solver. A constraint
solver smears exactly the artefacts that make the movement feel the way it does.

One movement step per usercmd, with dt taken from that usercmd's frametime: there is no
fixed movement tick (`docs/oracle.md`). Deterministic, f32 to match the game. Input
arrives as a usercmd with per-command button edges, not polled held state: a scroll-wheel
tap strafe is a press and release inside one command, which is what caps the tap rate.

Phase 1 is headless: a simulation that eats a usercmd stream and emits a per-tick
trajectory to diff. A window, a flat-shaded map and the camera come later (roadmap 4, 5).

Not goals: rendering beyond a flat-shaded map and a stand-in viewmodel; netcode,
prediction, rollback; weapons, abilities, legends, animations as art; map loading beyond
collision; multiplayer; game modes, menus, anything that is not practising movement.

## Layout

    src/              the lurch crate: trace parser, movement step, replay, sound, animation
    src/sim/          movement, one function per oracle function (docs/oracle.md)
    src/bin/          replay, chain, fields, inspect, cues, traces, ltrconv
    tests/oracle.rs   the replay gates
    traces/           recorded oracle traces (lurchtr4); `cargo test` gates on them
    traces/local/     recordings kept off git (none now); gated when present
    traces/pending/   recordings of mechanics not ported yet; `cargo test -- --ignored`
    capture/          lurchcap3.dll + inject.exe (guest, mingw), drain and drive (host); `make`
    dumps/            the oracle image, committed zstd-compressed, unpacked at session start
    ref/              HAVM's extracted convar values and datamap offsets, retail build
    tools/            see below
    docs/oracle.md    recovered entry points, layouts and settings for the oracle build
    docs/feedback.md  how the oracle answers movement with sound and animation
    docs/ground-truth.md  where the ground truth comes from
    docs/setup.md     where sessions run, and setting up a cloud session
    docs/capture.md   recording on the guest
    docs/maps.md      getting R5Reloaded map collision into a mesh
    docs/roadmap.md   order of work
    CLAUDE.md         rules for Claude; .claude/rules/ adds rules by area
    .claude/settings.json  the SessionStart hook and the project's plugins

Tools run from the repo root:

    tools/dump.sh         dump the running game into dumps/current.bin (workstation)
    tools/sect.py         is a dump complete and in the clear?
    tools/rtti.py         resolve a class name to its vtable and vfunc RVAs
    tools/dec.sh          decompile oracle functions by RVA (Ghidra headless)
    tools/annotate.py     name ConVars, settings and constants in dec.sh output
    tools/settings.py     name the player-settings offset globals
    tools/tables.py       the activity, animation event and activity modifier tables
    tools/record.sh       record one scripted scenario (drive + drain) into traces/
    tools/fuzz.py         seeded random drive script, for record.sh
    tools/session-start.sh  the SessionStart hook: unpacks the oracle image
    tools/cloud-setup.sh  a cloud environment's setup script (docs/setup.md)

`rtti.py` with no class names lists the movement-related RTTI names in the image. With
names, it prints each vtable and its function RVAs. It recovers the image base from the
locator pointers, so no base has to be passed in. A vfunc count that reports "hit cap,
unterminated" ran past the end of the vtable: the walk stops at the first slot outside
.text and has no other terminator.

## Legal

Decompiler output is derivative. Recovered constants and a clean reimplementation written
from a behavioural spec are the defensible shape, so the source holds no lifted bytes. The
one exception is `dumps/current.bin.zst`, the oracle image, so that any session can
decompile. The repository is private for that reason and must stay so.
