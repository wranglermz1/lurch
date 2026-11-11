# lurch

An offline practice game for Apex Legends movement. The movement is reimplemented in Rust
and proven against the game's own code: recorded input replayed through lurch gives the same
position and velocity, bit for bit, as the real engine. The goal is the full movement tech
list (strafe accel, air control, lurch and tap strafe, slide and slide hop, superjump, mantle,
wall climb, superglide, jump grace and multi-jump penalty) and the camera's answer to movement
(FOV scaling, bob, punch, shake). `docs/roadmap.md` is the order of work.

The purpose is educational: recovering a movement model from a shipped binary, and proving
the recovery with a differential test.

## Based on R5Reloaded

lurch is based on [R5Reloaded](https://r5reloaded.com), the community-run Season 3 build of
Apex Legends, and relies on it for its assets.

- **The reference.** R5Reloaded ships without anti-cheat, so its movement code can be hooked
  in the running game and called with chosen state. Every number lurch claims is checked
  against recordings taken that way, from R5Reloaded build 5dba2a2f-02ae8471.
- **The assets.** The first-person arms and their animations, the heirloom melee weapons,
  the movement sounds, the test map's props and the game's own HUD and menus are read from
  your R5Reloaded install each time the game starts (`docs/assets.md`). None of it is in this
  repository. Without an install, the game runs with a flat-shaded test map and nothing else.

lurch is not affiliated with or endorsed by Electronic Arts, Respawn Entertainment or the
R5Reloaded team. Apex Legends is a trademark of Electronic Arts.

## What is here

- `src/`: the lurch crate. The headless movement simulation and the replay gates that check
  it against the recordings in `traces/`. Rust stable and std only, no dependencies.
- `game/`: the game you play. A Bevy shell that drives the simulation one command per frame,
  with the first-person arms, sound and UI from the install.
- `minecraft/`: a separate project. Apex movement, the arms and the HUD inside Minecraft,
  through a Fabric mod and a host process (`minecraft/README.md`). Neither the lurch crate
  nor the game depends on it.
- `capture/`, `tools/`, `docs/`: recording the reference, reverse engineering it, and what
  was recovered.

## Setup

### What you need

- Rust stable, 1.95 or later (`rustup`).
- An R5Reloaded install, for the arms, sound and UI. Build 5dba2a2f-02ae8471 is the one
  checked; another build loads with a warning, and its RTech paks do not load.
- Windows to hear sound: it plays through the game's own Miles library, which exists only
  for Windows. On Linux the game runs silent.
- On Linux, Bevy's system packages. On Debian or Ubuntu:

      sudo apt install g++ pkg-config libx11-dev libwayland-dev libxkbcommon-dev libxkbcommon-x11-0 \
        libxcursor1 libxrandr2 libxi6 libx11-xcb1 libvulkan1 mesa-vulkan-drivers

### Run the tests

    cargo test --release

This replays every recording in `traces/` through the simulation and needs neither the game
nor an install. The game shell has its own tests: `cd game && cargo test --release`.

### Play

    cd game
    cargo run --release -- --r5r "C:\Program Files\R5Reloaded\R5R Library\LIVE"

Without `--r5r` the game asks for the install folder (the one holding `r5apex.exe`) on its
first run and keeps the answer in `lurch.cfg` beside the executable. Other flags (sensitivity,
FOV scale, fps cap, legend, heirloom, scroll-wheel binds for tap strafing) are listed by
`lurch-game --help`.

To build the Windows executable on Linux:

    rustup target add x86_64-pc-windows-gnu        # needs mingw-w64
    cd game && cargo build --release --target x86_64-pc-windows-gnu

Default keys:

| Key | Action |
|---|---|
| W A S D | move |
| Space | jump |
| Left Ctrl, C | crouch (hold), crouch (toggle) |
| Left Shift | sprint |
| Q | Pathfinder's grapple |
| E | use (ziplines) |
| Left mouse | punch |
| R | respawn at the current spot |
| [ and ] | previous and next spot on the test map |
| F1, F2, Escape | HUD, dev menu, game menu and settings |

### Minecraft

    cd minecraft/host && cargo run --release -- --r5r INSTALL
    cd minecraft/mod && ./gradlew runClient

Then load a world and press F7. Setup, controls and what is checked are in
`minecraft/README.md`.

### Reverse engineering and recording

Not needed to play or to run the tests.

- Decompiling (`tools/dec.sh`) needs Ghidra 12.x with a JDK 21 or later, and the oracle
  image: a full in-memory dump of R5Reloaded's `r5apex.exe`, build 5dba2a2f-02ae8471, at
  `dumps/current.bin`. The image is not distributed; `tools/sect.py` checks that a dump is
  complete and in the clear. `docs/ground-truth.md` lists the sources.
- Recording new traces needs R5Reloaded running in a Windows VM with the capture DLL injected
  (`docs/capture.md`). `docs/setup.md` describes the workstation it was built on.

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
velocity and all 146 modelled player fields:

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
Ziplines are ported from the image but not recorded, so unverified. So is Pathfinder's
grapple (`docs/oracle.md`, Grapple): its state is among the fields, so the gates will hold it
to a grapple recording once there is one. That includes the hook catching a zipline and the
grab it leads to, the first-person animation state it sets (`+0x2a4c`, a gated field), the
suit power's refill and the view's roll toward the rope; the grapple's sounds, its arms
animation, the rope's shape and the crosshair's in-range answer are ported from the image too
(`docs/feedback.md`, Grapple), none of it recorded.

Melee: the push a punch on the ground gives with no target (`0x826540`) is ported from the
image, and the shell starts the attack on the attack button with the fists out and plays the
melee weapon's attack activity (`docs/oracle.md`, Melee). No recording holds a punch: the gates
only check the push stays zero, and the lunge's fields cleared, without one. The lunge toward a
target, the blow's trace and the knockback it gives are ported but not recorded.

Camera: what the movement call keeps for the view (sprint tilt, view punch springs, fall
speed, sprint transition times, eye height through a duck) is among those fields, and so
is what the view setup updates once a frame between calls (slide tilt, FOV ease, the blend
onto the climbing arms), checked from each call's output to the next call's input. View kicks draw from the command's
random seed, which the recordings lack; the replay checks the kick lands on the same call
and takes the oracle's punch. `docs/camera.md`.

Sound and animation: the fields include the footstep timer, landing and sprint-blend
bookkeeping, a mantle's gun flag, and the climbing arms with their view-offset entity, whose
wall ray is matched against each recording's trace log. The cues movement raises
(footsteps, jumps, landings, crouch, slide boost, sprint end, and the zipline callbacks once a
zipline is recorded) must fall on the calls where the oracle acted. The layers built on them
run outside movement and are not checked against the oracle: the sound names, played through
the game's own Miles, the fists' viewmodel (weapon logic, sequence transitions, overlay
layers, bone setup in the studio library's float forms), and the first-person proxy's arms
through a mantle and a wall climb with the view they carry, all read from the user's
R5Reloaded install at run time with the arms' textures (`docs/assets.md`). The body has no
sequences loaded yet. `docs/feedback.md`.

Collision: `src/world.rs` sweeps the hull against triangles (in memory, or a HAVM `.mesh`).
Every mp_rr_arena_empty recording also replays chained with our own collision against its
floor, exact to `TOL`. No mesh exists for any R5Reloaded map, so collision beyond a flat
floor is unverified. `tests/testmap.rs` drives a pilot over the test map's steps, ramps,
walls and hill and holds it to the limits its settings set (no fidelity claim). The test
map's props lane (loot bin, cargo containers, the Thunderdome barrier, rocks, a fence) is
read from the install and collides with its drawn triangles, not the models' own collision.

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

The first-person arms, their animation and the movement sounds are the game's own, read
from the user's R5Reloaded install when the shell starts; none of it is kept in this
repository (`docs/assets.md`).

The game's own UI is being brought in the same way: the HUD, the crosshairs and the
settings menu, run from the install's RUI assets and the compiled RUI code in `ui.dll`
(`docs/ui.md` holds the plan and its progress).

Not goals: rendering beyond a flat-shaded map, the first-person arms, the heirloom melee
viewmodels (`--heirloom`, `docs/assets.md`) and the UI; weapons beyond the fists, abilities, legends' kits; other Apex models and game assets; map loading
beyond collision; game modes, menus other than settings, anything that is not practising
movement. Deferred, not refused: multiplayer and our own models
(`docs/game-shell-plan.md` section 8).

## Layout

    src/              the lurch crate: trace parser, movement step, replay, test map, sound,
                      animation
    src/assets/       the game's files, read from an R5Reloaded install (docs/assets.md)
    src/sim/          movement, one function per oracle function (docs/oracle.md)
    src/bin/          replay, chain, call, fields, inspect, cues, traces, ltrconv, rootmotion
                      (mantle root motion out of recordings, into src/sim/rootmotion.rs),
                      assets (what an install holds), unzstd
    tests/oracle.rs   the replay gates; tests/frame.rs and tests/testmap.rs the camera
                      and the test map
    game/             the Bevy game shell; its own workspace, root cargo test never builds it
    traces/           recorded oracle traces (lurchtr4); `cargo test` gates on them
    traces/local/     recordings kept off git (none now); gated when present
    traces/pending/   recordings of mechanics not ported yet; `cargo test -- --ignored`
    minecraft/        the Minecraft passthrough: Fabric mod and lurch-mc host (minecraft/README.md)
    capture/          lurchcap3.dll + inject.exe (guest, mingw), drain and drive (host); `make`
    dumps/            where the oracle image goes (dumps/current.bin, gitignored)
    ref/              HAVM's extracted convar values and datamap offsets, retail build
    tools/            see below
    docs/oracle.md    recovered entry points, layouts and settings for the oracle build
    docs/feedback.md  how the oracle answers movement with sound and animation
    docs/assets.md    the game's files the shell reads from an install, and how
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
    tools/session-start.sh  the SessionStart hook: unpacks a compressed oracle image
    tools/cloud-setup.sh  a cloud environment's setup script (docs/setup.md)
    tools/bevy-deps.sh    system packages for building game/ in a cloud session

`rtti.py` with no class names lists the movement-related RTTI names in the image. With
names, it prints each vtable and its function RVAs. It recovers the image base from the
locator pointers, so no base has to be passed in. A vfunc count that reports "hit cap,
unterminated" ran past the end of the vtable: the walk stops at the first slot outside
.text and has no other terminator.

## Legal

Decompiler output is derivative. Recovered constants and a clean reimplementation written
from a behavioural spec are the defensible shape, so the source holds no lifted bytes and no
game file. The oracle image used for reverse engineering is not distributed, and the game's
files are read from your own install at run time. `traces/` holds movement state recorded
from R5Reloaded; `ref/` holds ConVar values and datamap field names and offsets extracted
from a retail build.

## License

MIT (`LICENSE`), for lurch's own code and documentation. It does not cover the game's files
lurch reads from an install.
