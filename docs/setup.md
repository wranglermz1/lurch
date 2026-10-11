# Where a session runs

## Workstation

Has everything: HAVM at `/home/fz/HAVM/HAVM`, Ghidra, and the Windows guest (`analysis1`)
running the game. Recording (`docs/capture.md`), dumping, and map meshes
(`docs/maps.md`, `maps/` is local) happen only here.

`tools/dump.sh` wraps HAVM's `bin/redump` rather than reimplementing it: pages already
held are never re-read, and a build change archives the old image instead of overwriting
it. It needs the guest up and `bin/redump` built in HAVM. Overrides: `HAVM`, `PROC`,
`HAVM_DOMAIN`, `HOST_CPUS`. Guest vCPUs own host cores 0-11, so host-side work stays
pinned to 12-17.

### The Windows build

The game shell's sound runs through the game's own Miles, which exists only for Windows, so
the shell is built for Windows and run in the guest, against the guest's own R5Reloaded
install (`docs/assets.md`). The real game is never launched for it. On the workstation:

    rustup target add x86_64-pc-windows-gnu      # once; needs mingw-w64
    cd game && cargo build --release --target x86_64-pc-windows-gnu
    x86_64-w64-mingw32-strip -o lurch-game.exe target/x86_64-pc-windows-gnu/release/lurch-game.exe
    scp -i ~/.ssh/win-analysis_ed25519 lurch-game.exe fz@192.168.8.248:/C:/xfer/lurch/

Over SSH a program runs in session 0, with no desktop: the shell's window cannot open
there, but `vmshot INSTALL SCRIPT OUTDIR FRAMES` renders a script's frames with the
viewmodel offscreen (Vulkan on the guest's GPU) and `milescheck INSTALL play NAME...` plays
sounds through Miles. The window itself is tried from the guest's own desktop.
`/mnt/guestro` on the host can mount the guest's disk read-only (`qemu-nbd`) to read its
install without starting the guest's game; unmount it and disconnect the device after.

## Cloud

A cloud session (claude.ai/code, the apps, `claude --cloud`) starts from a fresh clone.
It can build, test, read the recordings and decompile. It cannot record, reach the guest
or HAVM, or load map meshes; that work goes back to the workstation.

At session start, `tools/session-start.sh` (the repository's SessionStart hook) unpacks
`dumps/current.bin.zst` into `dumps/current.bin`, with `zstd` or, failing that, Python's
`zstandard`. It reports anything the session lacks, such as Ghidra.

### The environment's setup script

The environment a cloud session runs in has a setup script, run as root before Claude Code
starts and cached with the environment. Paste `tools/cloud-setup.sh` into it (the cloud
environment menu in a session's title bar, then Edit, then Setup script). It installs:

- `zstd`;
- Ghidra 12.0.4 at `/opt/ghidra` (the image already has the JDK 21 it needs);
- the caveman and ponytail plugins (below).

Without it, run `tools/cloud-setup.sh` by hand in a session: Ghidra is usable at once, the
plugins from the next session. Where Ghidra release downloads are blocked,
`tools/decomp.sh` builds Ghidra's decompiler from a source clone instead.

### The game shell

A cloud session can build `game/` after `tools/bevy-deps.sh` (its system packages), run its
headless tests and take xvfb screenshots on lavapipe. How the shell plays is judged on the
workstation (`docs/game-shell-plan.md`, M10).

## Plugins

The project uses two third-party Claude Code plugins, both MIT-licensed:

- [caveman](https://github.com/JuliusBrussee/caveman) (`caveman@caveman`, pinned to
  v3.2.0): terse chat replies. `/caveman lite|full|ultra` changes the level; "stop caveman"
  or "normal mode" turns it off.
- [ponytail](https://github.com/DietrichGebert/ponytail) (`ponytail@ponytail`, pinned to
  v5.1.0): the smallest code that works; no unrequested abstractions or dependencies.
  `/ponytail lite|full|ultra|off` changes the level; `/ponytail-review` reviews a diff.

Both run Node hooks on session start, subagent start and each prompt; they keep their state
under `~/.claude` and `~/.config` and make no network requests. `CLAUDE.md` says how they
apply in this repository.

Where they come from depends on where Claude Code runs:

- **Terminal, desktop app, IDE**: `.claude/settings.json` declares both marketplaces and
  enables both plugins. Trusting the folder registers the marketplaces; each person then
  installs once:

      claude plugin install caveman@caveman --scope project
      claude plugin install ponytail@ponytail --scope project

  To opt out on one machine, set the plugin to `false` in `.claude/settings.local.json`.
- **Cloud sessions**: they ignore plugins a repository's settings enable, so the setup
  script above installs them at user scope before Claude Code starts.
