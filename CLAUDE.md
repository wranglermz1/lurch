# Rules for Claude in lurch

lurch reimplements Apex Legends movement in Rust and proves each part against the game's
own code. `README.md` is the overview, `docs/roadmap.md` the order of work,
`docs/setup.md` how sessions run. Facts live there and in `docs/`; this file is rules.
`.claude/rules/` adds rules for Rust, docs and workstation-only work when you open those
files.

## Scope

- Take work from the top of `docs/roadmap.md` unless the user names something else.
- Build only what practising movement needs. Never build rendering beyond a flat-shaded
  map and a stand-in viewmodel, netcode or prediction, weapons, abilities, legends,
  animation as art, map loading beyond collision, multiplayer, menus or game modes.

## Evidence

- The authority is the game's code: `dumps/current.bin`, R5Reloaded build
  5dba2a2f-02ae8471. Not footage, not what you remember of the game, not the Source SDK.
  Only the base layer (trace and clip, bump loop, step-up, 2-unit ground snap) follows
  published Source behaviour.
- Claim fidelity only for what a replay gate checks. Every report says what is verified and
  what is not.
- `ref/` comes from a retail build. Confirm a value against the oracle image before using it.
- Before decompiling, read what `docs/oracle.md`, `docs/feedback.md` and
  `docs/ground-truth.md` already record, and extend them rather than re-derive it.
- Recording, the guest, HAVM and map meshes exist only on the workstation. In a cloud
  session, say when a task needs them instead of guessing what the game does.

## Reverse engineering

- Decompile with `tools/dec.sh`, then `tools/annotate.py`. Name things with
  `tools/rtti.py`, `tools/settings.py` and `tools/tables.py`. Run tools from the repo root.
- Give every address as an RVA into `dumps/current.bin`.
- Check a decompiled `0.0` or `1.4e306` constant against the disassembly before using it.
- Read ConVar values and settings offsets from the image, never from memory.
- When the decompiler drops a call's arguments, read them from the disassembly.

## Code

- Rust stable with std only; tools are POSIX shell and Python stdlib. Ask before adding a
  crate, a toolchain or any other dependency.
- No comments by default. Write one only for a "why" the code cannot show. If a comment is
  needed to say what code does, rename instead.
- Mirror the oracle's call tree: one function per oracle function, its RVA in
  `docs/oracle.md`, names ours.
- f32 in the oracle's evaluation order. Keep forms like `(a - a) + k * a` and `!(a < b)`:
  they keep the oracle's signed zeros and NaN answers.
- One movement step per usercmd, dt from that usercmd's frametime. Input is per-command
  button edges, never polled state.
- Collision is a swept hull against triangles, never a rigid-body or constraint solver.
- Keep `cargo clippy --release --all-targets` free of warnings. Do not run `cargo fmt`;
  the tree is not formatted by it.

## Verification

- Run `cargo test --release` before every commit. It must pass on a fresh clone.
- A new recording that fails goes in `traces/pending/` until its mechanic is ported, then
  moves to `traces/`.
- When movement writes a new player field, read it from the recordings and add it to
  `oracle::player`, `diff_player` and `with_state`, so both replay gates check it.
- When you add or change a gate, break the code once on purpose and watch the gate fail.

## Docs

- Write what you recover into `docs/` as behaviour: conditions, order, constants, the names
  of settings and ConVars. Never paste decompiler output into docs or source.
- A recovered offset, setting or function goes in `docs/oracle.md`, or `docs/feedback.md`
  for sound and animation. A change in what the gates cover goes in README.md's
  "Fidelity reached".
- Keep this file to rules for Claude and under 200 lines. Put facts in README.md or `docs/`.

## Plugins

- The caveman and ponytail plugins are on for this project (`docs/setup.md`). If they are
  not loaded, follow their intent anyway: short chat replies, the smallest change that works.
- caveman shortens chat replies only. Code, comments, commit messages and docs keep the
  repository's plain, full-sentence style.
- Fidelity beats brevity. ponytail never removes oracle behaviour or rewrites an oracle
  float form to save lines.
- No ponytail `shortcut:` comments. Write a known limit in the module's doc comment
  ("Not modelled: ...") or in `docs/roadmap.md`.
- In `src/sim/` the test for new logic is a replay gate (a field in the diff or a
  recording), not a hand-written expectation.

## Git and legal

- The repository is private and must stay private: it holds the oracle image
  (`dumps/current.bin.zst`). Never publish or paste its contents elsewhere.
- No lifted bytes in source: recovered constants and a clean reimplementation only.
- Commit messages follow the history: `feat(sim): ...`, `fix(sim): ...`, `test: ...`,
  `docs: ...`, `chore: ...`, a summary line, then what changed and why.
