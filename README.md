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
velocity and all 113 modelled player fields:

- call by call, each call starting from the oracle's state;
- chained, the simulation carrying its own state from call to call for whole recordings.
  Where something outside movement changes the player between calls (a server update of
  the networked sprint timestamps, a restored fall speed), the chain takes those fields
  from the oracle and goes on.

Both pass again on the built-in pilot settings (`src/sim/pilot.rs`). A third run takes
nothing from the oracle but each command and the clock: a pilot spawned where the
recording starts, move data built from the command (`src/sim/setup.rs`). It is bit-exact
on origin and velocity for every recording, taking a mantle's animation root motion from
the recording as it takes collision.

Recordings: walking, turning, sliding and seeded random input over World's Edge terrain;
wall climbs there (walking, sprinting, ducking off, releasing, jumping off); on
mp_rr_arena_empty jumps, sprint jumps, bunny hops, lurch, tap strafe, air turning, crouch,
crouch jump, air duck, slide, slide hop, slide jump, mantles (sprinting at a chest-high
wall, and at the end of a wall climb); and seeded random input (`tools/fuzz.py`). The
camera's lag behind a step up or ground snap is ported and exact. Out of the mantle
animation's root motion, only types 3 and 5 are recorded (`src/sim/rootmotion.rs`).
Ziplines are ported from the image but not recorded, so unverified.

Camera: what the movement call keeps for the view (sprint tilt, view punch springs, fall
speed, sprint transition times, eye height through a duck) is among those fields, and so
is what the view setup updates once a frame between calls (slide tilt, FOV ease), checked
from each call's output to the next call's input. View kicks draw from the command's
random seed, which the recordings lack; the replay checks the kick lands on the same call
and takes the oracle's punch. `docs/camera.md`.

Sound and animation: the fields include the footstep timer, landing and sprint-blend
bookkeeping. The cues movement raises (footsteps, jumps, landings, crouch, slide boost)
must fall on the calls where the oracle acted. The sound and animation layers built on them
run outside movement and are not checked against the oracle yet; the game's sound events
and sequences are not loaded (roadmap 4). `docs/feedback.md`.

Collision: `src/world.rs` sweeps the hull against triangles (in memory, or a HAVM `.mesh`).
Every mp_rr_arena_empty recording also replays chained with our own collision against its
floor, exact to `TOL`. No mesh exists for any R5Reloaded map, so collision beyond a flat
floor is unverified. `tests/testmap.rs` drives a pilot over the test map's steps, ramps,
walls and hill and holds it to the limits its settings set (no fidelity claim).

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

The lurch crate stays headless: no renderer, no engine, no window. The game shell in `game/`
(Bevy) drives it, one command per rendered frame, and makes no fidelity claim of its own.

Not goals: rendering beyond a flat-shaded map and a stand-in viewmodel; weapons,
abilities, legends; Apex models, animations and other game assets; map loading beyond
collision; game modes, menus, anything that is not practising movement. Deferred, not
refused: multiplayer and our own models (`docs/game-shell-plan.md` section 8).

## Layout

    src/              the lurch crate: trace parser, movement step, replay, test map, sound,
                      animation
    src/sim/          movement, one function per oracle function (docs/oracle.md)
    src/bin/          replay, chain, call, fields, inspect, cues, traces, ltrconv, rootmotion
                      (mantle root motion out of recordings, into src/sim/rootmotion.rs)
    tests/oracle.rs   the replay gates; tests/frame.rs and tests/testmap.rs the camera
                      and the test map
    game/             the Bevy game shell; its own workspace, root cargo test never builds it
    traces/           recorded oracle traces (lurchtr4); `cargo test` gates on them
    traces/local/     recordings kept off git (none now); gated when present
    traces/pending/   recordings of mechanics not ported yet; `cargo test -- --ignored`
    capture/          lurchcap3.dll + inject.exe (guest, mingw), drain and drive (host); `make`
    dumps/            the oracle image, committed zstd-compressed, unpacked at session start
    ref/              HAVM's extracted convar values and datamap offsets, retail build
    tools/            see below
    docs/oracle.md    recovered entry points, layouts and settings for the oracle build
    docs/feedback.md  how the oracle answers movement with sound and animation
    docs/camera.md    the camera's answer to movement, recovered; what is ported and gated
    docs/game-shell-plan.md  the Bevy game shell plan (roadmap item 2) and its status
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
    tools/decomp.sh       the same with Ghidra's decompiler built from source, no install
    tools/annotate.py     name ConVars, settings and constants in dec.sh output
    tools/settings.py     name the player-settings offset globals
    tools/tables.py       the activity, animation event and activity modifier tables
    tools/record.sh       record one scripted scenario (drive + drain) into traces/
    tools/fuzz.py         seeded random drive script, for record.sh
    tools/session-start.sh  the SessionStart hook: unpacks the oracle image
    tools/cloud-setup.sh  a cloud environment's setup script (docs/setup.md)
    tools/bevy-deps.sh    system packages for building game/ in a cloud session

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
