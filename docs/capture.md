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
