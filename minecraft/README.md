# lurch in Minecraft

lurch's movement drives the Minecraft player, the way passthrough mods join two running
games: Minecraft keeps rendering and running its world, and a separate process runs lurch.

- `bridge/` is `lurch-bridge`, a Rust program (std and the lurch crate only). It listens on
  `127.0.0.1:25600` and steps one `Pilot` per connection. Its module doc comment defines the
  line protocol.
- `mod/` is a client-only Fabric mod for Minecraft Java 26.3. Once per rendered frame it sends
  the player's view angles, movement keys and frametime, then places the player and the
  first-person camera where the bridge says. It needs only Fabric Loader, not Fabric API.

F7 toggles lurch in game. When anything else moves the player, such as `/tp`, a respawn or a
server correction, lurch restarts at the new position.

## Running

    cd minecraft/bridge && cargo run --release        # leave running
    cd minecraft/mod && ./gradlew runClient           # dev client; or ./gradlew build and
                                                      # copy build/libs/lurch-mc-*.jar to mods/

Make a Superflat world in Creative, then press F7. The Gradle wrapper is 9.8.1, the first
release that runs on Java 27.

## Units and axes

A block is 40 units, so the standing hull (72 units) is 1.8 blocks and the eye (60 units) is
1.5 blocks. Source (x, y, z) maps to Minecraft (x, z, -y). Minecraft's yaw is -(Source yaw) - 90,
and pitch is the same in both. lurch's origin sits at the spawn position, which keeps f32
coordinates small.

## What is checked

- `cargo test --release` in `bridge/`: forward walks where Minecraft faces and stays on the
  floor, with the eye above the feet.
- By hand over TCP (2026-10-17): sprinting east gave 7 blocks/s, a jump rose 0.85 blocks, and a
  round trip took 33 µs.
- The mod compiles against 26.3. Its mixin targets (`Minecraft.renderFrame`, `Entity.move`,
  `Camera.update` after `alignWithEntity`) were matched to the 26.3 jar with `javap`. It has not
  been run in a client yet.

## Not modelled

- Collision with anything but a flat floor at the height where lurch was turned on. Blocks
  are not sent to the bridge yet.
- The grapple, autosprint, Apex's arms, HUD and sounds.
- Fall damage: lurch's ground flag is sent to the server, but the server still counts falls,
  so use Creative.
- Riding, swimming, ladders and other Minecraft-only movement. Turn lurch off for them.
- Multiplayer servers. Their movement checks are untested.
