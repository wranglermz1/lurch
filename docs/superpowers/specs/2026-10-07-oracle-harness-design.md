# Oracle Harness and Flat-Ground Movement: Design

Milestone 1 of lurch. It proves the differential method end to end: real movement ticks
are captured from R5Reloaded, replayed through a headless Rust simulation, and pass a
per-tick diff for walking and air movement on flat ground.

## Decisions

| Question | Decision |
|---|---|
| What milestone 1 proves | The harness works end to end, on ground and air movement over flat ground |
| Capture | An in-guest DLL detours `C_GameMovement::ProcessMovement` |
| Transport | A ring in guest RAM, found by a magic header and drained by HAVM reads on the host |
| Loading | A guest-side injector (`CreateRemoteThread(LoadLibraryA)`) |
| Pass criterion | Single-step origin and velocity within `1e-3` units; tightened later, and the reached number is recorded in CLAUDE.md |
| Replay | Single-step diff is the gate. A trajectory diff from the same recordings is reported only, and becomes the gate in a later phase |
| Toolchains | Rust stable (no crates) and `mingw-w64` on the host. Neither is installed until the user approves it at execution time |

## Assumptions recon must confirm

Each one is checked in the oracle build (`5dba2a2f-02ae8471`) and recorded in
`docs/oracle.md` before code depends on it:

1. The oracle is the client-side `C_GameMovement` (vtable rva `0x144a2d8`). The process
   connects to a remote server and runs no server game code.
2. Movement runs once per usercmd, with `dt` taken from the usercmd's frametime
   (`usercmd_frametime_min 0.002857` .. `usercmd_frametime_max 0.1`), not on a fixed tick. If confirmed,
   CLAUDE.md's "fixed tick" line is corrected.
3. Ground speed and acceleration come from player settings, not convars. No
   `sv_accelerate` exists in `ref/_convars_values.ini`.
4. The retail usercmd layout in `HAVM/docs/usercmd-layout.md` gives the field names
   only. Offsets are re-derived for the oracle build.

## Components

```
guest                                     host
r5apex.exe                                tools/drain.py (HAVM reads)
  ProcessMovement <- detour                 finds ring by magic, follows write index,
  lurchcap.dll -> ring in guest RAM ------>  writes traces/*.trace
inject.exe (run once)                     lurch crate: trace, movement, bin/replay
```

- **Recon.** Finds the `ProcessMovement` RVA, and the usercmd, `CMoveData` and
  player-settings offsets. Output: `docs/oracle.md`, with evidence for each entry.
- **`capture/lurchcap.c`.** On load, it checks the image's PE timestamp against
  `5dba2a2f` and refuses to hook on a mismatch. It detours `ProcessMovement`: copies the
  cmd and the state going in, calls the original, copies the state coming out, and writes
  one fixed-size record into a static ring. The ring header holds the magic, the record
  size, the capacity, the write index and a dropped-record counter. No allocation, no I/O.
- **`capture/inject.c`.** Takes a DLL path and injects it into `r5apex.exe`.
- **`tools/drain.py`.** Locates the ring through HAVM, follows the write index, and
  writes the text trace. By default it keeps the first execution of each `seq`; `--all`
  keeps prediction re-runs too. It reports dropped records.
- **`lurch` crate.** Rust, std only, f32 throughout.
  - `trace`: the parser.
  - `movement`: `step(settings, cmd, state) -> state`.
  - `bin/replay`: runs both diffs over the given trace files.

The text trace is the only contract between the capture side and the Rust side. The
binary ring layout is private to `lurchcap.c` and `drain.py`.

## Trace format

Text, one record per movement call. Every f32 is written as 8 hex digits of its bit
pattern, and integers are written as hex.

```
# lurch-trace 1 build=5dba2a2f-02ae8471 map=<name>
settings <key>=<hex> ...
cmd <seq> <frametime> <pitch> <yaw> <roll> <fwd> <side> <up> <buttons> <oldbuttons>
in  <ox> <oy> <oz> <vx> <vy> <vz> <flags> <onground>
out <ox> <oy> <oz> <vx> <vy> <vz> <flags> <onground>
```

- `seq` is the usercmd `command_number`. A gap in `seq` means lost records.
- The settings keys are fixed by recon.
- `cmd`, `in` and `out` always appear as a triple.
- Fields change only by bumping the version. The parser rejects an unknown version.

## Error handling

| Condition | Behaviour |
|---|---|
| Ring overflow | The dropped counter increments, the drain reports it, and `seq` shows the gap |
| Prediction re-run of a `seq` | Recorded every time. The drain keeps the first by default; trajectory mode uses only first executions |
| Build mismatch | The DLL does not hook and says why |
| NaN, broken triple, bad hex, unknown version | The parser fails with the line number. Nothing is skipped |
| `seq` gap in trajectory mode | The trajectory restarts from the next recorded starting state; it is never bridged |

Replay output, per file: the step count, the single-step failure count, the max absolute
origin and velocity errors, and the first failing `seq` with its in/cmd/out lines next to
Rust's output for that step. The exit code is non-zero on any single-step failure.

## Simulation scope

- **Ground:** wish velocity from fwd/side and yaw, `sv_backspeed`, the max-speed clamp,
  friction, then accelerate. Start from published Source behaviour and correct it wherever
  the oracle disagrees.
- **Air:** gravity, air acceleration, and `player_extraairaccelleration`, with the air
  speed cap recovered from the binary.
- **No collision.** The ground is the plane at the recorded starting z. Airborne
  recordings end before landing.
- **Constants:** confirmed against the oracle before use: `sv_gravity 750`,
  `sv_friction 4`, `sv_stopspeed 100`, `sv_maxspeed 320`, `sv_airaccelerate 10`,
  `player_extraairaccelleration 2.0`, `sv_backspeed 0.6`.
- **Out of scope:** crouch, jump, slide, collision, step-up, and every Respawn mechanic.

When a step fails, the fix comes from the oracle's code, never from tuning a constant
until the numbers fit.

## Recordings

Committed under `traces/`, all on flat open ground:

- `idle`
- `walk_fwd`, `walk_back`, `walk_diag`
- `stop` (run, then release)
- `strafe_circle`
- `fall` (walk off a ledge; airborne ticks only)
- `air_strafe`

## Testing

- **Unit tests:**
  - friction below `sv_stopspeed` clamps to exactly 0
  - `dt` of 0.002857 and of 0.1 never flips the sign of velocity
  - diagonal input is clamped to max speed
  - backward input is scaled by `sv_backspeed`
  - one gravity tick from rest gives `vz = -sv_gravity * dt`
  - the parser rejects NaN, orphan lines and unknown versions, naming the line
- **Gate:** a `cargo test` runs replay over every committed trace and requires 0
  single-step failures at `TOL = 1e-3`. Trajectory error is printed, not asserted.
- **Capture check:** `idle.trace` has every `out` equal to its `in`, and contiguous `seq`.
  The DLL and the drain have no other tests.

## Done when

The gate passes on all eight recordings. `docs/oracle.md` records the confirmed
offsets and assumptions. CLAUDE.md states the tolerance reached and the corrected tick
model.

## Later phases (each gets its own spec and plan)

1. Collision against the canyonlands mesh: the bump loop, step-up and the 2-unit snap.
2. Jump, jump grace and the multi-jump penalty.
3. Slide and slide hop.
4. Lurch and tap strafe.
5. Traversal: mantle, wall climb, superglide and superjump.
6. Synthetic-state calls into the oracle, for edge cases play cannot reach.
7. The trajectory diff becomes the gate over long mixed sessions.
