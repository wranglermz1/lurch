# Recording on the guest

Needs the Windows guest (libvirt domain `analysis1`) up with r5apex.exe in a match, and
`capture/` built (`make`). The user starts the game and places the player; everything
else runs from the host.

- Reach the guest: `ssh -i /root/.ssh/win-analysis_ed25519 fz@192.168.8.248 '<cmd.exe command>'`.
  Ping never answers; `arping -c 2 192.168.8.248` checks it is up.
- Inject the capture DLL into the running game once per game start:
  `ssh ... 'cd /d C:\xfer\lurch && inject.exe lurchcap3.dll'`. `drain` says
  "lurchcap3.dll is not loaded" when this was skipped.
- Record: `MAP=<map> tools/record.sh NAME STEP...`, steps as for `capture/drive`
  (`keys:seconds`, keys comma-joined, `-` for none). The player must not touch the
  keyboard or mouse while drive plays the keys.
- A take that moves the player away (a climb, a mantle onto a roof) needs the user to
  reset the position before the next. Calibrate distances from a fixed point: walking
  into a wall first, then backing off, gives a repeatable start.

Ziplines. A zipline is entity data, not player state, so on entry to each call the DLL also
snapshots the zipline the player rides (`+0x2998`), the entity its rope runs to, that one's
next and the zipline's previous: handle, kind, abs origin and angles, then the fields from
`+0xa00` (`docs/oracle.md`, Zipline). They travel as the call's first trace entries, slot
`0x1000 | index << 4 | chunk`, so drain and the `.ltr` format are unchanged;
`oracle::ziplines` reads them back and the replay's tracer serves them. The use that grabs a
zipline runs before the movement call and is not recorded; its outcome arrives in the next
call's player state. drive presses use as `use`. Takes to record, on a map with ziplines
(World's Edge has them; Flowstate can place one): walk under a rope and `use:0.05`, then
ride to the end; the same, jumping (`jump:0.05`) and crouching (`duck:0.05`) off mid-rope;
grabbing while running and while falling past a rope (hold `use`); a vertical rope to its
top; a chain. Keep the DLL from before this change out of those takes: it records no
ziplines, and the replay then has none to ride.

Grapple. The grapple's state is player state (`+0x2770`–`+0x2804`, `docs/oracle.md`,
Grapple), so the DLL needs no change. Play Pathfinder (`pilot_survival_grapple`); drive presses
the tactical as `offhand1`. The shot runs after the movement call and is not recorded; it
arrives in the next call's player state. Takes, on mp_rr_arena_empty unless a wall is needed
(World's Edge has them): fire at a wall standing (`offhand1:0.05`, then nothing for 3 s); the
same while running and in the air; a duck press mid-pull (`duck:0.05`); a jump mid-pull; firing
again mid-pull to let go; a miss (aim at the sky); a pull over a ledge that bends the rope.

Known capture faults, which the replay tolerates:

- In `climb_walk` about one call in eleven holds another caller's CMoveData (other view
  angles, no buttons). Those calls are skipped, and the chain restarts after each: a call
  whose CMoveData angles differ from the player's command angles (`+0x2188`) is not replayed.
- The command's random seed is not captured, so view kicks cannot be drawn in replay (see
  `docs/camera.md`). Logging `usercmd+0x184` with each call would fix that.
- A trace entry can come out all zero though none was counted lost. A query that matches
  nothing in the log takes such an entry as a miss; the call's outcome must still agree.
