# lurch

Apex Legends movement, reimplemented in Rust, in our own engine: an offline game for
practising movement. The goal is behavioural equivalence with the real game on the full
movement tech list (strafe accel, air control, lurch and tap strafe, slide and slide hop,
superjump, mantle, wall climb, superglide, jump grace and multi-jump penalty) and on the
camera's answer to movement (FOV scaling, bob, punch, shake). `docs/roadmap.md` is the
order of work.

Educational purpose: recovering a movement model from a shipped binary and proving the
recovery with a differential test.

## Meta-Rules for Claude

Maintain this file under 200 lines. Keep it high-level. Do not explain code mechanics
here; the source files carry that.

No Code Comments: default to zero comments. Only write a comment for a non-obvious "why".
If a comment is needed to explain what a function does, the naming is wrong.

The lurch crate has no dependencies and gets none. Bevy 0.19.1, pinned exactly, is approved
for the game crate in `game/` only, with the features its Cargo.toml lists. Ask before adding
any other crate, feature or toolchain.

## Non-goals

Rendering beyond a flat-shaded map and a stand-in viewmodel. Weapons, abilities,
legends; Apex models, animations and other game assets. Map loading beyond
collision. Game modes, menus, anything that is not practising movement. Deferred, not
refused: multiplayer and our own models (`docs/game-shell-plan.md` section 8).

## Definition of done

"1:1" is a number, not a feel. A recorded input stream replayed through our simulation
produces the same per-tick origin and velocity as the real engine, inside a stated
tolerance. Until that harness exists, no fidelity claim is made.

The oracle is the game's own code, not gameplay footage. R5Reloaded ships without
anti-cheat, so the movement function can be hooked in-process and called with synthetic
state, then diffed against ours.

## Ground truth

Lives in the HAVM repo at /home/fz/HAVM/HAVM, already extracted. Do not re-derive it.

- `ref/_convars_values.ini` — 3769 convars with flags, defaults and live values, read from a
  retail image (base 7ff7dc990000), not the oracle; confirm against 5dba2a2f.
  `sv_gravity 750`, `sv_friction 4`, `sv_stopspeed 100`, `sv_maxspeed 320`,
  `sv_airaccelerate 10`, `player_extraairaccelleration 2.0`,
  `wallclimb_vertical_gain_reduction 128`, `automantle_forwarddist 26`,
  `slide_max_angle_dot 0.6`, `wallrun_slipduration 1.0`, `cl_phys_maxticks 3`.
- `ref/_offsets.ini` — retail build 6abfe359-04164401. Datamap field names and offsets,
  795 of them on the player. Names carry over; offsets do not fit the oracle build. The
  traversal state machine (`m_traversalState`, `m_traversalType`, `m_traversalProgress`,
  `m_traversalBegin/Mid/End`) and the wallrun and slide fields come from here.
- `maps/*.mesh` — extracted collision geometry, retail maps. None of them is a map the
  oracle runs, so none can verify collision against recordings. The guest hosts its own
  match under Flowstate. Recordings come from mp_rr_desertlands_64k_x_64k and
  mp_rr_arena_empty (large and flat; `map mp_rr_arena_empty` in the console).
  Keep `flowstate_modified_movement 0` (Apex movement) in the playlist.
- `dumps/current.bin` (HAVM `dumps/5dba2a2f-02ae8471/dump.bin`) — full plaintext image of `r5apex.exe` under
  R5Reloaded main. `.text` and `.rdata` 100% held, no encrypted pages.
- `dump.bin` — retail `r5apex_dx12.exe`, probably build 6abfe359 (same capture
  as `_offsets.ini`; the earlier 6ab42c93 label is unverified). Newer mechanics live here
  (`m_jumpGraceTapCount`, `tapstrafe_repeated_tapping_*`) that the S3 build lacks.

### Entry points, build 5dba2a2f-02ae8471, base 0x7ff7b6a90000

The R5Reloaded build retains RTTI, so these were resolved by name rather than by
pattern. Retail strips it.

    C_GameMovement   vtable rva 0x144a2d8   17 virtuals   client, predicted
    CGameMovement    vtable rva 0x14db2f0   17 virtuals   server
    IGameMovement    vtable rva 0x14da9c0   17 virtuals
    CPlayerMove      vtable rva 0x1564958    5 virtuals
    CWallrunCurve    vtable rva 0x15a8650

## Layout

    tools/dump.sh     dump the running game into dumps/current.bin
    tools/sect.py     is a dump complete and in the clear?
    tools/rtti.py     resolve a class name to its vtable and vfunc RVAs
    tools/dec.sh      decompile oracle functions by RVA (Ghidra headless, /opt/ghidra)
    tools/decomp.sh   the same with Ghidra's C++ decompiler built from source, no install
    tools/settings.py name the player-settings offset globals
    tools/annotate.py name ConVars, settings and constants in dec.sh output
    tools/record.sh   record one scripted scenario (drive + drain) into traces/
    tools/fuzz.py     seeded random drive script, for record.sh
    tools/bevy-deps.sh   system packages for building game/ in a cloud session
    docs/oracle.md    recovered entry points, layouts and settings for the oracle build
    docs/camera.md    the camera's answer to movement, recovered; what is ported and gated
    docs/game-shell-plan.md  the Bevy game shell plan (roadmap item 2) and its status
    capture/          lurchcap3.dll + inject.exe (guest, mingw), drain and drive (host); `make`
    src/              the lurch crate: trace parser, movement step, replay, test map
    src/bin/rootmotion.rs   mantle root motion out of recordings, into src/sim/rootmotion.rs
    game/             the Bevy game shell; its own workspace, root cargo test never builds it
    traces/           recorded oracle traces (lurchtr4); `cargo test` gates on them
    traces/local/     recordings kept off git (none now); gated when present
    traces/pending/   recordings of mechanics not ported yet; `cargo test -- --ignored`
    docs/maps.md      getting R5Reloaded map collision into a mesh
    docs/capture.md   recording on the guest
    docs/roadmap.md   order of work
    dumps/            the oracle image, committed zstd-compressed; a SessionStart hook unpacks it
    ref/              HAVM's extracted convar values and datamap offsets, retail build

Run the tools from the repo root.

`dump.sh` wraps HAVM's `bin/redump` rather than reimplementing it: pages already
held are never re-read, and a build change archives the old image instead of
overwriting it. It needs the guest up and `bin/redump` built in HAVM. Overrides:
`HAVM`, `PROC`, `HAVM_DOMAIN`, `HOST_CPUS`. Guest vCPUs own host cores 0-11, so
host-side work stays pinned to 12-17.

`rtti.py` with no class names lists the movement-related RTTI names in the image.
With names, it prints each vtable and its function RVAs. It recovers the image
base from the locator pointers, so no base has to be passed in. A vfunc count
that reports "hit cap, unterminated" ran past the end of the vtable: the walk
stops at the first slot outside .text and has no other terminator.

## Fidelity reached

`cargo test` replays every recording two ways, and both must be bit-exact on origin,
velocity and all 90 modelled player fields:
- call by call, each call starting from the oracle's state;
- chained, the simulation carrying its own state from call to call for whole recordings.

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

Camera: what the movement call keeps for the view (sprint tilt, view punch springs, fall
speed, sprint transition times, eye height through a duck) is among those fields, and so
is what the view setup updates once a frame between calls (slide tilt, FOV ease), checked
from each call's output to the next call's input.
View kicks draw from the command's random seed, which the recordings lack; the replay
checks the kick lands on the same call and takes the oracle's punch. `docs/camera.md`.

Collision: `src/world.rs` sweeps the hull against triangles (in memory, or a HAVM `.mesh`).
Every mp_rr_arena_empty recording also replays chained with our own collision against its
floor, exact to `TOL`. No mesh exists for any R5Reloaded map, so collision beyond a flat
floor is unverified. `tests/testmap.rs` drives a pilot over the test map's steps, ramps,
walls and hill and holds it to the limits its settings set (no fidelity claim).

## Where a session runs

The workstation has everything: HAVM, Ghidra, the guest with the game. A cloud session
has this repo, and with it the oracle image (unpacked to `dumps/current.bin` at session
start; needs `zstd`). It can build, test, read the recordings and decompile: with
Ghidra 12.x at `/opt/ghidra` (or `GHIDRA=`) and a JDK 21+, or with `tools/decomp.sh`,
which builds the decompiler from a Ghidra clone where release downloads are blocked.
It cannot record or load map meshes (`maps/` is local). Work that needs those goes back to the
workstation; say so rather than guess at what the game does.
A cloud session can build `game/` after `tools/bevy-deps.sh`, run its headless tests and
take xvfb screenshots on lavapipe. How the shell plays is judged on the workstation.

`cargo test` must stay green on a fresh clone. A new recording that fails goes in
`traces/pending/` until the mechanic is ported, then moves up to `traces/`.

## Approach

The engine is Source-derived, so the base layer is known ground: trace-and-clip player
move, a bump loop with velocity clipping, step-up, the 2-unit ground snap. Write that
from the published Source behaviour. Ground and air acceleration are not Source's, and
neither are the Respawn additions (lurch, slide, traversal, wall climb). Those are
recovered from the binary.

Collision is a swept hull against level geometry, not a rigid-body solver. A constraint
solver smears exactly the artefacts that make the movement feel the way it does.

One movement step per usercmd, with dt taken from that usercmd's frametime: there is no
fixed movement tick (see `docs/oracle.md`). Deterministic, f32 to match the game. Input arrives as a usercmd with
per-command button edges, not polled held state: a scroll-wheel tap strafe is a press and
release inside one command, which is what caps the tap rate.

The lurch crate stays headless: no renderer, no engine, no window. The game shell in `game/`
drives it, one command per rendered frame, and makes no fidelity claim of its own.

## Legal

Decompiler output is derivative. Recovered constants and a clean reimplementation written
from a behavioural spec are the defensible shape. No lifted bytes in the source. Exception: `dumps/current.bin.zst` holds the oracle image so
any session can decompile. The repository is private for that reason and must stay so.
