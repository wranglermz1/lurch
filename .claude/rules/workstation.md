---
paths:
  - "capture/**"
  - "traces/**"
  - "tools/dump.sh"
  - "tools/record.sh"
  - "tools/fuzz.py"
  - "docs/capture.md"
  - "docs/maps.md"
---

# Workstation-only work

- Capture, recording, dumping and map meshes need the workstation: HAVM at
  `/home/fz/HAVM/HAVM` and the `analysis1` guest running the game. In a cloud session,
  do not run them; say what needs running and where.
- Keep host-side work on cores 12-17 (`taskset -c 12-17`); the guest's vCPUs own 0-11.
- `tools/dump.sh` wraps HAVM's `bin/redump`. Use it; never reimplement the dump.
- Record on mp_rr_desertlands_64k_x_64k or mp_rr_arena_empty, with
  `flowstate_modified_movement 0` in the playlist (`docs/capture.md`).
- The user starts the game and places the player. Before `tools/record.sh` plays keys,
  tell the user to keep off the guest's keyboard and mouse until it finishes.
- `traces/local/` holds recordings kept off git; the gates use them when present.
