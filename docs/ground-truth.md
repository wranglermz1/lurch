# Ground truth

Where the facts about the game come from. Most of it lives in the HAVM repo at
`/home/fz/HAVM/HAVM` on the workstation, already extracted; it is not re-derived here.

## Sources

- `ref/_convars_values.ini`: 3769 ConVars with flags, defaults and live values, read from a
  retail image (base 7ff7dc990000), not the oracle; confirm against 5dba2a2f.
  `sv_gravity 750`, `sv_friction 4`, `sv_stopspeed 100`, `sv_maxspeed 320`,
  `sv_airaccelerate 10`, `player_extraairaccelleration 2.0`,
  `wallclimb_vertical_gain_reduction 128`, `automantle_forwarddist 26`,
  `slide_max_angle_dot 0.6`, `wallrun_slipduration 1.0`, `cl_phys_maxticks 3`.
- `ref/_offsets.ini`: retail build 6abfe359-04164401. Datamap field names and offsets,
  795 of them on the player. Names carry over; offsets do not fit the oracle build. The
  traversal state machine (`m_traversalState`, `m_traversalType`, `m_traversalProgress`,
  `m_traversalBegin/Mid/End`) and the wallrun and slide fields come from here.
- `maps/*.mesh`: extracted collision geometry, retail maps. None of them is a map the
  oracle runs, so none can verify collision against recordings. The guest hosts its own
  match under Flowstate. Recordings come from mp_rr_desertlands_64k_x_64k and
  mp_rr_arena_empty (large and flat; `map mp_rr_arena_empty` in the console).
  The playlist keeps `flowstate_modified_movement 0` (Apex movement).
- `dumps/current.bin` (HAVM `dumps/5dba2a2f-02ae8471/dump.bin`): full plaintext image of
  `r5apex.exe` under R5Reloaded main. `.text` and `.rdata` 100% held, no encrypted pages.
  Committed as `dumps/current.bin.zst`.
- `dump.bin`: retail `r5apex_dx12.exe`, probably build 6abfe359 (same capture as
  `_offsets.ini`; the earlier 6ab42c93 label is unverified). Newer mechanics live here
  (`m_jumpGraceTapCount`, `tapstrafe_repeated_tapping_*`) that the S3 build lacks.

## Entry points, build 5dba2a2f-02ae8471, base 0x7ff7b6a90000

The R5Reloaded build retains RTTI, so these were resolved by name rather than by pattern.
Retail strips it.

    C_GameMovement   vtable rva 0x144a2d8   17 virtuals   client, predicted
    CGameMovement    vtable rva 0x14db2f0   17 virtuals   server
    IGameMovement    vtable rva 0x14da9c0   17 virtuals
    CPlayerMove      vtable rva 0x1564958    5 virtuals
    CWallrunCurve    vtable rva 0x15a8650

`docs/oracle.md` follows the call chain from there.
