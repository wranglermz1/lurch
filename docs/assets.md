# The game's files, read from the install

lurch keeps no Apex asset. The game shell, `cues` and `assets` read what they need from the
user's own R5Reloaded install at run time (`src/assets/`, `game/src/miles.rs`). Facts here
are for build 5dba2a2f-02ae8471; addresses are RVAs into `dumps/current.bin`.

## Finding the install

`--r5r PATH`, else asked once on the console with
`C:\Program Files\R5Reloaded\R5R Library\LIVE` offered, then kept in `lurch.cfg` beside the
executable (`game/src/install.rs`). An install holds `r5apex.exe` and
`paks/Win64/common.rpak`. Its build is the executable's PE timestamp and checksum; another
build loads with a warning, and its RTech paks do not load (below).

## What is read

| What | Where | Used for |
|---|---|---|
| Player settings layouts | `common_early.rpak` (RTech), `common.rpak` | naming the settings fields |
| The legend's player settings | `settings/player/mp/pilot_survival_*` whose `footstep_type` is the legend's name, in `common.rpak`, else `common_sdk.rpak` | sound names, `armsModel`, `viewmodelfov`, `classActivityModifier` |
| The fists' script | `platform/scripts/weapons/mp_weapon_melee_survival.txt` and its `#base` | `viewmodel`, the `*_time` keys |
| The fists' viewmodel | the script's `viewmodel` (`ptpov_emptyhand.rmdl`), its rigs and sequences, from `common.rpak` | viewmodel animation |
| The legend's arms | `armsModel`, from `common.rpak`, vertex data from its starpak | the drawn arms |
| The arms' materials | each mesh's material through skin family 0, its textures from `common.rpak` and `pc_all.starpak` | albedo, normal map, ambient occlusion |
| The grapple's look | `lurch::assets::GRAPPLE_HOOK` (`projectile_grapple_hook_anchor_bolt`) from `common.rpak`, its material (`grapple_gun`) there too; the rope's material `cable/grapple` from `common_early.rpak` | the hook entity and the rope (`docs/feedback.md`, Grapple) |
| The test map's props | `testmap::PROP_PAK` (the Firing Range's `mp_rr_canyonlands_staging.rpak`), the models `testmap::build` places, their albedo and vertex colours | the props lane, drawn and colliding |
| Sound | `mileswin64.dll`, `binkawin64.dll`, `audio/ship/audio.mprj`, bank `general`, language `english` | every sound |

## Paks

RPak version 8: a 0x80-byte header, then the data compressed whole. Header flag `0x8000` is
zstd (the paks R5Reloaded rebuilt; `src/assets/zstd.rs`, a std-only RFC 8878 decoder).
Flag `0x100` is Respawn's own stream format (the original Season 3 paks, such as
`common_early`); its decoder (`src/assets/rtech.rs`) needs the 0x500-byte lookup table at
`0x1313180`, read from the install's own executable when its build is the oracle's.

Assets used, by type and version: `mdl_` 10 (studio version 54), `arig` 4, `aseq` 7,
`stgs` 1, `stlt` (layouts), `matl` 15, `shds` 11, `txtr` 8. Names sit at header +0x10 (`mdl_`, `stgs`) and +8 (`arig`,
`aseq`) and are matched ignoring case and slash direction. Streamed data (`*.starpak`) is
addressed by a starpak index in the low 12 bits and an offset above them. Vertex data is
`0tVG` version 1: packed 64-bit positions, packed normals, up to three packed weights plus
an extra-weight list through the mesh's bone map, and one UV set.

## Models and sequences

The viewmodel's bones come from its studio header, and its sequences from its rigs, numbered
as the game numbers them: the model's own `ref` first, then each rig's in order. A rig's
bones are matched to the model's by name, ignoring case, as the engine's virtual model does.
Bone setup and its RVAs are in `docs/feedback.md` (Viewmodel).

A model's body parts each hold models, one drawn at a time by the body value (bodygroups).
The arms are loaded with body 0's: the first model of each part. Pathfinder's arms have the
parts `body`, `zipline_back`, `zipline_front`, `l_hand` and `l_hand_usb`, whose second model
body 0 does not draw.

A static prop has one bone, `static_prop`, at the origin, and no rigs. Its vertices may carry
an RGBA colour (vertex flag `0x10`, four bytes after the normal); the cargo containers' red
and blue are these colours over a grey albedo. The shell multiplies the albedo (texture
slot 0) by them. A prop's collision in the shell is its drawn level-0 triangles, read before
the simulation starts; the models' own collision data is not read.

## Materials and textures

A mesh's material reference goes through the model's skin family 0 (`numskinref` at studio
`+0xe0`, the table at `+0xe8`, one `short` per reference) to a texture slot, whose GUID
names a `matl`. A material (version 15, header `0x100`) holds its texture GUIDs at `+0x60`;
how many is its shader set's (`shds`, GUID at `+0x58`) count at `+0x18`. Their meaning is the
pixel shader's resource order; for the arms' skinned shader (`_sknp`) it is
`albedoTexture`, `normalTexture`, `glossTexture`, `specTexture`, `emissiveTexture`,
`aoTexture`, `cavityTexture`, read from the shader's own resource names.

A texture (version 8, header `0x38`): width and height `+0x10`, `+0x12`, format `+0x16`,
array size `+0x1e` (1, or 0 in the Season 3 paks such as `common_early`, read as 1), and
three mip counts: optional streamed
`+0x1d`, permanent `+0x21`, streamed `+0x22`, which sum to the full chain. The permanent
mips are the smallest, in the asset's CPU data; the next ones are in the starpak, the
largest in the optional starpak (`*.opt.starpak`). Each part holds its mips smallest first,
each padded to 16 bytes. R5Reloaded ships no optional starpak, so a 2048 texture tops out
at 512. The format is an index the oracle turns into a DXGI format through the table at
`0x130a760`: 0, 1 BC1 (1 sRGB), 2 to 5 BC2 and BC3, 6, 7 BC4, 8, 9 BC5, 10, 11 BC6H, 12, 13
BC7 (13 sRGB), then uncompressed ones, which lurch does not read. The arms' albedo is
BC1 sRGB, the normal map BC5 with DirectX's green (down the image: its gradient follows the
cavity map's in both axes), gloss, AO and cavity BC4. The shell hands the blocks to the GPU
as they are and samples with wrapping, as the arms' UVs run past 1.

## Settings and scripts

A settings asset is values at offsets; its layout names them. Weapon scripts are KeyValues:
the `#base` file first, then the file's own top-level `WeaponData` keys, case ignored. Keys
the script leaves out take the registered default, 0 for every `*_time` lurch reads
(registry at `0x120620`).

## Sound

The game's Miles 10 is loaded and set up as CSOM_Initialize (`0x95a140`) does: startup
parameters, startup, the driver on the default output (the oracle picks it from
`-xaudio2`, `-nosound`, `-wasapi`, `-dsound`, `-rme`, else default, `0x9587e0`) with
`miles_channels` (2) channels at `miles_samplerate` (48000) unless `-channels` or `-rate`
is given, Bink Audio
registered, a streaming cache of `0x4000000`, the project loaded for the language, each bank
with its streams and patches, then one queue. Each wait on the project or a bank services
the drivers, or a session-0 process hangs. `MilesEventGetDetails` takes a seventh
argument in this Miles (10.0.57), passed null. The service thread runs as `0x959500` does;
the listener follows the camera each frame (`0x956720`).

Global controllers: the client's sound frame (`0x5bf0b0`) sets them each frame through
`0x95b8d0`, which keeps a name and value and marks it changed; the sound update (`0x956b50`)
sends the changed ones with `MilesQueueControllerValue`. `0x5beb30` sets `VoiceCommVolume`
(`sound_volume_voice`, 0 while the window is out of focus unless `sound_without_focus`),
`LobbyMusicVolume` (`sound_volume_music_lobby`), `GameMusicVolume`
(`sound_volume_music_game`), `DialogueVolume` (`sound_volume_dialogue`) and `SfxVolume`
(`sound_volume_sfx`), each ConVar times 100; the `_sp` ConVars stand in for single player.
`0x5bec80` sets, for the local player, `IsTitan` (100 for general class 1 or 3, else 0),
`LowHealth` and `TitanHealth` (100 for a pilot), `PlayerSpeed` (the length of the absolute
velocity) and `AmmoClipPct` (from the active weapon's clip). Without `SfxVolume` the
footstep events peak at 0.0002 on the guest's output and `UI_Menu_Accept` at 0.17; with it,
a sprint step peaks at 0.02. lurch sets the volumes and the pilot's three once, `PlayerSpeed` from the last
command of each frame, and not `AmmoClipPct`.

A sound name resolves as CSOM_AddEventToQueue (`0x95b480`) resolves it: FNV-1 64 over the
name lowercased with `.` read as `_`, looked up among every event and source the banks list
(`0x9afb8b`, `0x9af740`). So `wraith.effort_land_1p` plays the event
`wraith_effort_land_1p`. The emitter is `0x954c60`. Windows only: elsewhere the shell is
silent.

## Checking

`LURCH_R5R=INSTALL cargo test --release --test assets` loads the legend, the fists and their
sequences from an install and poses them, and checks the arms' textures' mip chains; without
the variable it checks only the zstd decoder against the oracle image.
`cargo run --release --bin assets -- INSTALL --seqs mdl/weapons/empty_handed/ptpov_emptyhand.rmdl`
lists the fists' sequences, `--settings KEYS FILTER` a pak's settings, `--names PAK MAGIC
NAME_AT FILTER` a pak's assets of a type by name, and `--material PAK NAME` a material's
textures.
`cargo run --release --bin cues -- --r5r INSTALL traces/sprintjump.ltr` replays a recording
with the legend's sound names and the fists' viewmodel. `milescheck INSTALL play NAME...`
(game crate, Windows) plays names through the game's Miles; `vmshot` renders a script's
frames with the viewmodel (`docs/setup.md`).
