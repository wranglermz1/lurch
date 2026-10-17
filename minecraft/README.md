# lurch in Minecraft

While lurch is on, the Minecraft player moves, aims and is controlled as in Apex, and
Pathfinder's viewmodel and HUD are drawn over Minecraft's world. It works the way passthrough
mods join two running games: Minecraft keeps its world, window and server, and lurch's game
shell runs next to it as a second process.

- `game/src/bin/lurch-mc.rs` is the host. It runs the game shell offscreen, one update per
  Minecraft frame, and takes lurch-game's flags. Its module doc comment defines the protocol.
  - It gets Minecraft's keyboard and mouse as raw events, so the shell's own binds, mouse look
    and sensitivity apply: Shift sprints, Ctrl crouches, C toggles crouch, Q fires the grapple.
  - It gets the blocks around the player as collision.
  - It answers each frame with the player's position, the camera (eye, angles with view punch
    and roll, FOV) and the view angles.
  - It writes the frame's viewmodel, grapple rope and HUD, on a transparent background, to
    `/dev/shm/lurch-mc`.
- `mod/` is a client-only Fabric mod for Minecraft Java 26.3. It needs only Fabric Loader.
  - While lurch is on, it sends input to the host instead of Minecraft. Escape, F1 to F12 and
    the chat keys stay with Minecraft.
  - It scans blocks within 24 horizontally, 8 down and 24 up. The grapple reaches 850 units,
    about 21 blocks. It rescans every 0.5 s or after 4 blocks of movement, and sends the result
    only when it changed.
  - It places the player and the camera from the host's answer.
  - It draws the overlay in place of Minecraft's hand, crosshair and hotbar.
  - In singleplayer, the integrated server accepts lurch's moves as they are. Apex's mantles
    pass through the corner of a ledge, and the server's movement checks would pull the player
    back. The checks it skips are "moved too quickly", "moved wrongly" and colliding with a new
    block, and only for the world's own player while lurch drives it.

F7 toggles lurch. When anything else moves the player, such as `/tp` or a respawn, lurch
restarts there. The arms restart too, so a mantle cut short leaves nothing in the view. Steve
is 0.6 blocks wide and lurch's hull 0.8, so a restart where the hull would be inside a block
moves to the nearest clear place, up to half a block away.

## Running

    cd game && cargo run --release --bin lurch-mc -- --r5r INSTALL --legend pathfinder --autosprint
    cd minecraft/mod && ./gradlew runClient           # or ./gradlew build, then copy
                                                      # build/libs/lurch-mc-*.jar to mods/

Load a world and press F7. Without `--r5r`, the host runs without the viewmodel and the
game's UI. The Gradle wrapper is 9.8.1, the first release that runs on Java 27.

## Units and axes

A block is 40 units, so the standing hull (72 units) is 1.8 blocks, and a 1-block gap fits the
32-unit hull. Source (x, y, z) maps to Minecraft (x, z, -y). Minecraft's yaw is
-(Source yaw) - 90, and pitch is the same in both. lurch's origin is at the spawn position's
block, 64 blocks below the world's floor. That keeps f32 coordinates small and keeps the
shell's kill height out of reach above Minecraft's void.

## What is checked

- `cargo test --release --bin lurch-mc` in `game/`:
  - A block's open top face becomes floor facing up at its height, and positions round-trip
    between the two coordinate systems.
  - A spawn where Steve fits but the hull does not moves clear.
- The shell's own tests (`game/tests/shell/sim.rs`): an empty first command after a spawn on a
  floor of 40-unit tiles leaves the pilot free to move.
- In a 26.3 dev client on a Superflat world (2026-10-17, Intel UHD 770, 75 Hz):
  - F7 connects.
  - The overlay draws the shell's HUD and the tactical panel. The grapple's cooldown counted
    down after a grapple.
  - Minecraft's hand and hotbar are gone.
  - Minecraft held about 70 fps with a shell frame time of about 14 ms.
  - The user played it and confirmed it works.
  - A mantle onto a block beside a 2-block wall: before the server fix, the server pulled the
    player back mid-mantle, lurch restarted inside the block, and the camera flew in from the
    cut-short mantle. After it, a trace (`LURCH_MC_TRACE`) of four mantles showed one spawn,
    and the player moved after each. The user confirmed it, superglides included.
- The earlier flat-floor bridge (see git history) checked walking, sprinting, jumping, sliding
  and `/tp` against lurch's own numbers. The host runs the same simulation.

Not checked by any test: the camera against lurch's eye, the overlay's alignment with the
world, and block collision edge cases such as seams and partial blocks.

## Not modelled

- Sounds. The shell plays them through the game's Miles, which exists only for Windows, and
  this dev client has no audio device.
- Depth between the overlay and Minecraft's world. The grapple rope draws over blocks in front
  of it.
- Timing: the overlay is one readback behind the camera.
- The shell's dev text in the top-left corner, which overlaps Minecraft's F3 screen.
- Minecraft's items, block breaking and placing while lurch is on. Turn lurch off for them.
- Water, ladders, ice, soul sand and other Minecraft-only surfaces. Every collision box is
  solid ground.
- Fall damage: the server still counts falls, so use Creative.
- Multiplayer servers. Their movement checks would pull the player back from mantles; only the
  integrated server's are skipped.
