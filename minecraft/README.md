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
  - While lurch is on, it sends input to the host instead of Minecraft. Escape, F1 to F12, the
    chat keys and the hotbar's slot keys (1 to 9 by default) stay with Minecraft, unless one of
    lurch's keys is bound there.
  - Minecraft's Controls screen has an "Apex (lurch)" section: Tactical (Q), Ultimate (Z),
    Apex Jump (Space) and Apex Crouch (Left Ctrl). Minecraft saves them in its `options.txt`;
    the mod sends them to the host on connecting and on every change, and the host binds lurch's
    actions to them in place of its own (`bind`). Minecraft marks Q, Space and Left Ctrl as
    clashing with its own drop, jump and sprint keys; while lurch is on, they go to lurch only.
    The Ultimate key does nothing yet but name the key the ultimate's panel shows.
  - With an empty main hand the player has Apex's fists and moves with their speed scale
    (`move_speed_modifier` 1.15 in `mp_weapon_melee_survival`). Holding anything, a block or
    a tool, puts the fists away and shows Minecraft's own hand and item, and the speed scale is
    1.0, a weapon's without a `move_speed_modifier`. Through a grapple, mantle or climb the
    host draws Apex's arms and Minecraft's hand is hidden, whatever the player holds.
  - It scans blocks within 24 horizontally, 8 down and 24 up. The grapple reaches 850 units,
    about 21 blocks. It rescans every 0.5 s or after 4 blocks of movement, and sends the result
    only when it changed.
  - It places the player and the camera from the host's answer.
  - It sends Minecraft's light at the player's eye: the lightmap colour its own hand would be
    multiplied by (`lightmap.fsh` worked out on the CPU from the inputs Minecraft last rendered
    the lightmap with), and whether the dimension uses the Nether's cardinal lighting. The host
    then shades the arms as Minecraft shades its hand, in place of the shell's sun and sky:
    two fixed diffuse lights at 0.6 plus 0.4 ambient, times that colour, on gamma-encoded
    colours, with the arms' normal maps and no specular. Day and night, caves, torches, the
    Nether, night vision and the darkness effect all reach the arms, and the grapple's rope
    and hook too. All of them take the light at the eye, not where the rope or hook is.
  - It draws the overlay in place of Minecraft's hand and crosshair, under Minecraft's own HUD.
    The two HUDs share one bottom row, laid out to be roughly symmetrical (`rui::HostHud`):
    - Left: the Apex player panel (legend portrait and frame) where the game puts it. Its
      name and bars are left out. In their place Minecraft draws its armour, hearts and hunger
      as three rows of its own sprites, at a whole number of window pixels per sprite pixel (as
      many as fit three rows in the panel's height: 1 below about 1000 pixels high, 2 at
      1080p, 3 at 1440p). Air bubbles go above the panel.
    - Middle: Minecraft's hotbar, with the experience bar, the selected item's name and a
      mount's hearts, centred and standing on the Apex panels' bottom edge. It is drawn at
      Minecraft's GUI scale or smaller: the largest whole scale that leaves room for the
      tactical's panel between the hotbar's offhand slot and the player panel (1 at 480p, 3 at
      1080p, 4 at 1440p with the GUI scale on auto).
    - The tactical's panel stays where the game puts it unless it would reach the hotbar; then
      it moves left of the offhand slot.
    - Right: the ultimate's panel (`ability_hud`, Pathfinder's zipline) at the right edge, as
      far in as the player panel is on the left. It never charges: no ultimate is modelled.
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
  - Minecraft's hand is gone.
  - Minecraft held about 70 fps with a shell frame time of about 14 ms.
  - The user played it and confirmed it works.
  - The arms' lighting at noon and at midnight; the user confirmed it matches the world.
  - A grapple through the host under full and under dim night light: the rope and hook darken
    with the arms.
  - A mantle onto a block beside a 2-block wall: before the server fix, the server pulled the
    player back mid-mantle, lurch restarted inside the block, and the camera flew in from the
    cut-short mantle. After it, a trace (`LURCH_MC_TRACE`) of four mantles showed one spawn,
    and the player moved after each. The user confirmed it, superglides included.
- Hands and binds in the same client, by eye (2026-10-17): with Tactical bound to G in
  Minecraft's options, the tactical's panel showed G. Keys 1 and 2 switched between an empty
  slot (Apex fists) and oak planks (Minecraft's hand with the planks); the user played it and
  confirmed it. The speed while holding, and the grapple with a block in hand, were not looked
  at.
- The shared HUD in the same client, by eye, without a test (2026-10-17): at 854x480 and at
  2560x1440 fullscreen, with damage, a diamond helmet and iron chestplate and a shield in the
  offhand, the panel showed three rows at 1 and 3 window pixels per sprite pixel. The hotbar
  was centred at scale 1 and 4, its bottom level with the panels'. The tactical's panel kept
  its place at 854x480 and moved left of the offhand slot at 2560x1440. The ultimate's panel
  sat at the right edge.
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
- Using items, breaking and placing blocks, attacking and opening the inventory while lurch is
  on: the mouse buttons and E still go to lurch. Turn lurch off for them.
- Absorption and extra health: more than one row of hearts grows upward over the armour row.
- Water, ladders, ice, soul sand and other Minecraft-only surfaces. Every collision box is
  solid ground.
- Fall damage: the server still counts falls, so use Creative.
- Multiplayer servers. Their movement checks would pull the player back from mantles; only the
  integrated server's are skipped.
