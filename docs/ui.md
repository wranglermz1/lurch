# The game's UI

The HUD, the crosshairs and the settings menu, drawn the way the game draws them, from the
user's R5Reloaded install at run time. This file is the UI's plan, its progress and what
has been recovered about it. A session picking the UI up reads "How to pick this up", then
takes the first unticked task.

## How to pick this up

- Tick a task (`- [x]`) when it is done and checked, and write under it what was done,
  where it lives and what is still open. A task that turns out bigger than its line gets
  split into new tasks in place.
- Facts recovered from the game go in "Recovered" below as behaviour, with RVAs: into
  `dumps/current.bin` for the engine's RUI runtime, `ui.dll+RVA` for the compiled RUI code.
- The install on the workstation host is a partial copy at `/root/r5r-live` (the paks the
  shell reads, plus `ui*.rpak`, `ui.dll`, `ui_sdk.dll`); the full install is in the guest
  (`docs/setup.md`), which can be reached at the address `ip neigh show dev virbr2` gives
  for its MAC when `192.168.8.2` does not answer.
- Decompile the engine with `tools/dec.sh` as usual. Decompile `ui.dll` by laying it out
  with `tools/peimage.py` first:

      python3 tools/peimage.py /root/r5r-live/paks/Win64/ui.dll /tmp/ui.img
      IMAGE=/tmp/ui.img SLICE=$(stat -c%s /tmp/ui.img) CACHE=$PWD/ghidra-cache/ui \
          tools/dec.sh OUT.c 0x135a30

- Nothing read from the install is committed: not the RUI assets, not code or bytes from
  `ui.dll`, not fonts or images.
- Measure progress with `cargo run --release --bin ruicensus -- /root/r5r-live [NAME...]`: it
  runs every element (or the named ones) from `ui.dll` with its defaults and counts what
  stops each, opcode, callback or job type. Pick work from the top of its list, but know that
  an element that hides itself with its defaults never reaches its later callbacks (until
  sprintf was ported, `compass_flat` showed no text callback at all). Check a
  change by eye with `game/target/release/vmshot /root/r5r-live SCRIPT OUTDIR FRAMES`, which
  renders the shell offscreen with the UI.

## Where the last session stopped (2026-10-14)

The user asked for the tactical's panel and the zipline's use prompt, both 1:1 (tasks under
"The tactical's HUD and the zipline prompt"), and for the pause and settings menus (tasks
under "Menus"). Nothing of the UI track is committed (`main` at 8655ef5 has none of it; the
work is only in the working folder). At the end of this session `cargo test --release` (with
`LURCH_R5R`) and `cargo clippy --release --all-targets` pass on both crates.

Done on 2026-10-14:

- The faint box behind the prompt's labels: explained, and the game should draw it too
  (Recovered, "The UI shader", last paragraphs). Nothing changed in the code.
- The special image source and the screen blur (Recovered, "The special image source and the
  screen blur"). The image job draws it as the image or as the mask (`special_source` in
  `src/rui/draw.rs`; `Frame::screen_blur` says the blur ran). The shell runs the blur in
  `game/src/blur.rs` and `blur.wgsl` on the camera that draws the HUD. The tactical panel's
  backdrop now draws, and so would `screen_blur` and `tabs_background`. Census: 872 of 1405
  elements run and draw fully (821 before). `ruicensus` now runs with the blur on, as the
  image's ConVar default has it.
- Checked in `vmshot`: a debug view of `_rt_SmallFB0` showed the scene blurred, without the
  HUD. The panel looks as before over the test map: its dark fill is close to opaque over
  the backdrop (85% of the blurred scene, multiplied into the panel's shape image 383).
  Not compared with the game.

Seen and open:

- Not modelled: a nested element's styles taking the parent's colour matrix (`0x28f2b0`),
  `hint_display` (every usable but ziplines), the token table's key-name entries and aliases,
  a shape job (type 2) from the special source.
- `vmshot` scripts must hold a key from frame 0: a pilot whose first command after a spawn has
  no input stays stuck at the spot (seen 2026-10-13 in the library alone: `Pilot::new` at a
  spot, one empty command, then forward commands never move it). Reported to the user; not a
  UI matter.
- `Screenshot::image` of an image no camera draws into saves black (Bevy redirects a
  camera's output for a screenshot). To look at `_rt_SmallFB0`, show it with a temporary
  `ImageNode` in `vmshot` instead.

The settings menu, later the same day (what the user meant by "the UI work"):

- "ConVars in the shell" and "The ConVars lurch acts on" are done (`game/src/convar.rs`;
  Recovered, "The ConVar files").
- "Layout files" is done: `src/assets/keyvalues.rs` reads KeyValues as the engine does,
  conditions included, and `src/assets/vgui.rs` builds a layout's panel tree with the scheme's
  templates, nested files and button child groups (Recovered, "KeyValues and the layout
  files"). `menus_build_their_panels_from_the_install` in `tests/assets.rs` checks it on
  `system.menu` and `misc.menu`. Nothing draws the panels yet.

- "Layout" is done: `src/vgui.rs` (Recovered, "Layout and Solve").
- "Panel kinds", in part: RuiPanel, ImagePanel and the paint order are done (`create_rui`,
  `paint_rui`, `paint_list`; Recovered, "RuiPanel and RuiButton", "ImagePanel and paint
  order"). Nothing is drawn in the shell yet.

- RuiButton's text and states, and the panel under the cursor, are done (`label_text`,
  `paint_button_text`, `set_state`, `panel_at`, `focused`, `unfocused`; Recovered, "Input").
  Label waited for VGUI's fonts (done 2026-10-16 through RUI; Recovered, "VGUI's text").

- The pause menu draws in the shell (Escape), with the cursor's focus and the upper-case
  callback (`+0x50`) its buttons needed. Images draw with their own material (`vgui.wgsl`),
  ordered among the RUI runs.

Fixed on 2026-10-16: the pause menu's buttons looked focused until hovered and plain while
hovered. The globals' clock (`+0x90`) was frozen at the template's value, so every state fade
of `generic_button` stayed at its first frame (Recovered, "Instances and a frame"). The shell now
sets the clock from real time for the HUD and the menus. In `vmshot` a button now has a grey
edge, fades to an orange-red edge under the cursor and back when it leaves.
`the_system_menu_shows_two_buttons_and_focuses_under_the_cursor` checks that the fade moves
with the clock (it fails with the clock frozen).

Also on 2026-10-16: Settings opens `MiscMenu` (task "`MiscMenu`'s frame"). Its blur, cover,
vignette, logo, tab strip background and the details panel's box draw. The tabs and the
shoulder hints stop on transform opcode 11; the details panel has no text, the footer no width,
and no tab's content shows (the tab scripts are not ported).

Later on 2026-10-16 (the user asked for the full settings menu), in progress:

- Transform opcodes 11 and 12 ported (`op_basis`, `op_turn` in `src/rui/transform.rs`), checked
  bit for bit against the engine's own handlers run from `dumps/current.bin`
  (`opcodes_11_and_12_match_the_engines_handlers`). Census: 1181 of 1405 draw fully.
- `polyShape` hit tests (`poly_shape`, `within_shape` in `src/vgui.rs`), so the slanted tabs
  hit as the game's do.
- The settings menu's scripts start in `src/vgui/settings.rs` (`SettingsMenu`): tabs (titles,
  the selected one, a click switches, unused ones hidden, the tab last shown opens again), the
  details panel cleared per tab. Test `the_settings_menu_opens_on_its_tabs_and_a_click_switches_them`.
- DialogListButton (the switch rows): `ListButton` and `Menu::sync_lists`, `Menu::click` in
  `src/vgui.rs`; the shell syncs them from `ConVars` each frame and writes the ConVar a click
  picks. Not yet checked by a test.
- Found: the settings details images (`rui/menu/settings/settings_*`) are in none of this
  build's paks (host or guest); the game should draw the missing image or nothing there.
- The rows' titles and the details panel on focus: each tab's `SetupSettingsButton` and
  `SetupSettingsSlider` calls are read from its script at run time (`rows` in
  `src/vgui/settings.rs`, which compiles the script's `#if` branches for PC). Checked by the
  settings test (a row's title, the description of the row under the cursor).
- Scrolling (`_scroll_panel.gnut`): the wheel and the scroll bar's handle (Recovered, "Scroll
  panels"). Checked by the settings test (a notch, the top, a drag, a released handle).
- Sliders (`SliderControl`, Recovered, "SliderControl"): `src/vgui/slider.rs` and the slider
  part of `Menu` (`sync_sliders`, `press`, `drag`, `release`, the layout of its children).
  The Audio tab's volumes and the mic threshold draw with their bar, value and default mark,
  and a press or drag on a bar sets the ConVar. Checked by
  `sliders_follow_their_convar_and_the_cursor` and in `vmshot` (a click at a quarter of the
  master volume's bar shows 25%). `vmshot` now has the ConVars at the image's defaults.
- Fixed on the way: a RuiPanel is never the panel under the cursor (its constructor makes it
  so), and a panel whose clip rectangle is empty paints nothing (PaintTraverse `0x4c4130`);
  `menus_paint_in_zpos_order` had expected the footer's zero-width buttons to paint.
- Seen in `vmshot`, not explained: the Audio tab's subheader bars (`SubheaderBackgroundWide`,
  `ui/menu/common/menu_header_bar_wide`) draw as solid black bars. The texture is BC1, opaque
  black with a light dotted line along its bottom; how the surface blends a VGUI image is
  not recovered (`vgui.wgsl` multiplies by the draw colour, alpha blended). Compare with the
  game.
- The Mouse/Keyboard and Video tabs' bodies, which the engine makes (Recovered, "The engine's
  tab bodies"): their rows draw and act; the Video tab's switches have no choices yet (its
  constructor fills them), and the key bind list waits for Label.
- Found: VGUI's text is drawn through RUI (`drawtext_vgui`; Recovered, "VGUI's text").
- Label is done that way (`src/vgui/label.rs`, `Paint::Text`; Recovered, "VGUI's text"): the
  subheaders' text draws on every tab. The footers too: the pause menu shows Back and Dev
  Menu, each settings tab Back and Restore Defaults (`Menu::set_footer`), each button as wide
  as its text. The text measure (`Ui::text_size`, `0x2f5190`) and the colour conversion
  (`srgb_to_linear`, `0x270130`, checked bit for bit against the engine's code run from the
  dump) are ported. lurch now reads `dev_localization_files.txt` too, as the engine does
  (`DEV_MENU` is in `dev_english.txt`). Checked by `labels_draw_their_text_through_rui` and the
  system menu test (the footer's widths and places).
- Seen, not compared: a small mark over the B of the footer's Back, the Escape keycap that
  `%[B_BUTTON|ESCAPE]%` puts there in style `` `1 ``.
- The key bind list is recovered in part, not ported (Recovered, "The key bind list"): the
  action list, sections and columns, the colours, the layout of rows and columns, how the
  current keys fill in. `scripts/kb_act.lst` was copied from the guest into the host's
  `/root/r5r-live` for it.
- The Video tab's rows: their choice labels are known from the constructor (`0x9a9d30`); which
  choice shows comes from the video config through per-row code, and the resolution and
  aspect lists from the display's modes (`0x9af5b0`); not ported.
- Menu sounds (Recovered, "Menu sounds"): a button the cursor enters, a press, a switch's
  choice and a slider's change play the game's names through Miles (`Menu::sounds`, the
  shell's `MenuSound`). Checked by the system menu, settings and slider tests (the names, in
  order); not yet heard in the guest.
- The key bind list is ported (Recovered, "The key bind list"; task "Mouse/Keyboard tab"):
  rebinding works in the shell and saves to lurch's `settings.cfg`. Found on the way: lurch's
  RUI does not clip `drawtext_vgui`'s glyphs to its `clipRect` (a Label in a scrolled panel
  draws past the panel's edge); the mechanism is not traced.
- Next: the TextEntry beside the sliders, the Video tab's choices, the arrow keys, the footer
  buttons acting (Restore Defaults); then the game comparison.

Next, in order:

1. The pause menu in the game: a 1080p screenshot of R5Reloaded's to compare (the user
   starts the game), above all the buttons' edges and the cover's darkness.
2. Transform opcode 11, for the settings tabs (`tab_button_lobby`) and the shoulder hints;
   then the tabs' labels and the tab shown (`AddTab`, `_tabs.nut`), Label (VGUI's fonts) for
   the footer, and the arrow keys.
2. When the user starts the game in the guest: the prompt and the panel at 1080p (the
   cooldown, the backdrop, the shadow box over sky); and `find` on the twelve ConVars whose
   flags disagree with the game's files ("The ConVar files").

The UI pixel shader's full disassembly is on the guest at
`C:\xfer\shaders\a84403b56d2e69e3.dxbc.txt`. Two throwaway scripts help and are not in the
repo: a scan of `dumps/current.bin`'s code for RIP-relative references to an address (rel32
at each offset plus 4, plus 0, 1 or 4 bytes of immediate), and a `.pdata` lookup that follows
chained unwind info to a function's real start (the dump keeps `.pdata` at `0x2a360000`).
RTTI gives a class's bases: its locator's `+16` is the class hierarchy descriptor, whose base
class array lists each base's type descriptor. Weapon setting names by offset come from the
registration code at `0x100000` to `0x170000`:
`mov eax, offset`, a store of `ax`, then `lea rax, name`.

Files in the VPKs: the SDK's `bin\revpk.exe -unpack vpk\NAME_dir.vpk OUTDIR\` in the guest's
install unpacks one (OUTDIR must exist). `C:\xfer\vpk` holds `englishclient_frontend` and
`C:\xfer\vpkc` holds `englishclient_mp_common`, unpacked on 2026-10-14. A `.vcs` shader's
DXBC blob starts at `+0x3c` (its size at `+0x18` of the blob). `C:\xfer\shaders\dis.ps1`
disassembles every `.dxbc` in that folder. The blur's shaders are disassembled there too.

## Approach

An RUI ("Respawn UI") element is two things in the install:

- an asset of type `ui` in `ui.rpak`: its arguments (name hashes, types, offsets into the
  element's data block), their defaults, and the tables its draw calls read;
- a function of the same name exported by `paks/Win64/ui.dll`, compiled by Respawn from the
  element's source. It runs once per frame on the element's data block and calls back into
  the engine through a table of 34 functions (`0x1300ad0`) to read curves, colours and
  images and to ask for draws.

The engine side (the callbacks, the per-frame update, turning draws into quads and text,
the UI shader) is reimplemented in Rust from the image. The compiled RUI functions are run
as they are, loaded from the install's `ui.dll` the way the shell already loads the game's
Miles DLL: `ui.dll` imports nothing and is built for a fixed base, so it is mapped there
and called with the Windows x64 convention, on Windows and on x86-64 Linux alike. So every
element the game has is available once the callbacks are right, and none of its logic is
re-derived or copied.

Where the code goes:

- `src/assets/rui.rs`: the `ui` asset (and `uimg`, `font`) read from a pak.
- `src/rui/`: the RUI runtime, headless: `ui.dll` loading, the callback table, an element's
  per-frame update, its draw list (quads with atlas coordinates and colours, text runs).
  Tests run it against an install when `LURCH_R5R` is set, like `tests/assets.rs`.
- `game/src/hud.rs` and new modules: what the HUD shows and with which arguments (ported
  from the client scripts), and drawing the draw list with Bevy.

How it is checked: the runtime's pieces against the image as they are ported, and the
drawn result against screenshots of the real game in the guest (the user starts the game;
see `.claude/rules/workstation.md`). No replay gate covers the UI, so nothing here is
claimed beyond what such a comparison shows.

## Tasks

### Foundations

- [x] **Find the UI in the install.**
  `ui.rpak` (RTech stream) holds 1405 `ui` assets (version 39), 5 `uimg` atlases, 2
  `font` assets and the UI shaders; `ui_mp`, `ui_mainmenu` and `ui_sdk.rpak` hold only
  atlases. `paks/Win64/ui.dll` (PE timestamp `0x5dba2a2e`, the oracle's build) exports
  1955 functions, one per element plus `__hidden` variants. Layouts, menus, `.vfont` fonts
  and schemes are loose under `platform/resource`, the client scripts under
  `platform/scripts/vscripts` (`docs/assets.md`, The UI).
- [x] **The `ui` asset.** `src/assets/rui.rs` reads every element's blocks (layout under
  Recovered), the atlases and the palette; `lurch::assets::rpak::guid` is the engine's name
  hash. Checked by `tests/rui.rs`: `crosshair_grapple`'s arguments resolve by the names
  `mp_ability_grapple.txt` passes (`ammoFrac`, `isGrappleInRange`, `grappleInRangeTime`,
  `grappleOutOfRangeTime`, `weaponIsDisabled`, `crosshairMovementX`/`Y`), with their types.
  Open: the eighth argument (float at `+0x14`) is unnamed; the argument type numbers other
  than 3, 5 and 9 are not yet tied to the script functions that pass them.
  The defaults hold pointers to constant strings (text, image names); the pak's pointer
  descriptors say where (`Pak::pointers_in`), and each instance gets pointers to the
  element's copies of them (`Element::strings`), as the engine's relocated pak memory does.
- [x] **Run `ui.dll`.** `src/rui/code.rs` maps it section by section (raw `mmap`/`mprotect`
  syscalls on Linux, `VirtualAlloc` on Windows) and calls exports with `extern "win64"`.
  Checked: `crosshair_grapple` runs on the Linux host and its outputs match what its
  decompiled body computes (`tests/rui.rs`, with `LURCH_R5R=/root/r5r-live`).
- [ ] **The callback table.** Ported in `src/rui/api.rs`: `+0x00`, `+0x08`, `+0x10`,
  `+0x18`, `+0x20` (text layout, see Text; it returns the text's size), `+0x28` (fit),
  `+0x30`, `+0x38`, `+0x40` (sprintf), `+0x48` (format, see Localization), `+0x60`,
  `+0x68`, `+0x78` (project, Recovered "Projecting a point"; its unit test pins a point
  ahead, to the side and behind), `+0x90`, `+0xa0` to `+0xb8` (the mappings, one to four
  outputs), `+0xc8` (font), `+0xd0`. Each other slot stops the frame and names itself
  (`Stop::Api`), so an element needing it is not drawn. Next, by the census of 2026-10-13
  evening (1405 elements, 821 draw fully; the most stopped by opcode 12, 221): `+0x50`
  upper case (`0x2fb7b0`, 185), `+0x70` random (`0x2fbca0`, 57), `+0xd8` (`0x2fc4f0`).
- [ ] **The transform code.** `src/rui/transform.rs` runs opcodes 0, 1, 2, 4 to 9 and 13,
  each from its disassembly (5 to 7 for a plane topology only); any other opcode stops the
  frame (`Stop::Unported`). Opcode 10 is ported too (Recovered, "Transform opcode 10"). Still
  to port: 12 (rotation, with the engine's sine: `draw::sin_quarter_turns` is that sine), 11,
  3.
- [ ] **The per-frame update and the draw list.** `Ui::update` ports `Rui_Draw`'s set-up
  and `Ui::draws` (`src/rui/draw.rs`) the job walk for type-1 jobs: an image, its crop, the
  edge padding for anti-aliasing (`0x301320`, plane case), and the styles (`0x2fc900`, not
  nested, no hue rotation). Each other case is listed in `Draws::unported` and skipped.
  The mask path is ported (`mask_map`, from the disassembly at `0x2f98f6`). The emitter
  (`emit`, `0x2f6880`) has its nine-slice path (`nine_slice`, from the disassembly: the nine
  parts, which the crop cuts away, each part's outer anti-aliased sides, and how each axis
  of the template is scaled; docstring). Job type 2 (`shape_job`, `0x2f9d50`) is ported: a
  quad whose crop lanes carry the box as `[-1, 1]` for the shader's shape mode. The text job
  is under Text. The special image source (`0x2000`) draws as an image or a mask (Recovered,
  "The special image source and the screen blur"). Next: job types 3, 4 and 5, the
  styles of nested instances. Checked by eye in `vmshot` (the prompt's keycap and line, the
  dot); not against the game.
- [x] **Atlases.** `uimg` version 10 read (`src/assets/rui.rs`), registered into one image
  table as `0x2f39c0` does, textures uploaded (BC7 and BC1 sRGB). Fonts: see Text.
- [ ] **The UI shader.** The image mode and the tail every mode shares are ported to WGSL
  (`game/src/rui.wgsl`, a Bevy material reading this frame's style buffer, premultiplied
  blending): the 4-tap supersampling, repeat and clamp, the mask, the edge anti-aliasing,
  the colour matrix, tint, desaturation. Image bounds ride on each vertex instead of the
  `g_imgBounds` buffers. The text mode is under Text. The shape mode is ported from the
  disassembly (the disc, its ring, the two angles' half-planes, the three colours over the
  image; its distances' derivatives taken before the branch, as WGSL wants them in uniform
  flow). Not yet: video.
- [x] **Drawing in the shell.** `game/src/rui.rs`: the runtime read on a thread, one mesh
  per run of quads with one material, uploaded only when it changes, positions through the
  HUD topology in the target's pixels. The viewmodel's camera draws them (layer 3) in its
  transparent pass after the arms, ordered by each run's material; an orthographic camera of
  their own (order 2) is made only when there is no viewmodel, since each camera costs a
  whole view a frame (about 1.3 ms in the guest, 2026-10-13). The last camera hosts the
  shell's own Bevy UI: a later camera on the same target resolves its MSAA over an earlier
  camera's UI pass. `vmshot` renders it offscreen (`game/src/bin/vmshot.rs`).

### Text

Every HUD element the scripts create (`compass_flat`, `survival_player_hud_v2`, the ability
panels) draws text, so text comes before the HUD tasks.

- [x] **Fonts.** `font` version 7 read (`FontAtlas`, `Face`, `Glyph` in `src/assets/rui.rs`)
  with its R8 texture (`texture::load` reads format 53); `Ui::fonts` is the engine's font
  table by slot. Checked by `tests/rui.rs` against the running game's table in the image:
  every slot's face and atlas match, and the four font names `ui.dll` passes to `+0xc8`
  (`DefaultRegularFont`, `DefaultBoldFont`, `TitleRegularFont`, `TitleBoldFont`) land on
  faces.
- [x] **Text styles.** The style buffer's font terms (`0x2fc900`, `style` in
  `src/rui/draw.rs`), from the disassembly.
- [x] **sprintf** (`+0x40`, `src/rui/printf.rs`): C99 `vsnprintf` over the variadic slots;
  its unit test pins the formats `ui.dll` uses. `%.0f` of an exact tie rounds to even, as
  the Universal CRT does since Windows 10 2004; older Windows rounded it up.
- [x] **Layout** (`+0x20`, `0x2f55e0`, `src/rui/text.rs`): ported from the decompile, its
  compares from the disassembly. Checked only against itself (line breaks and widths from
  the font's advances, in its unit test); nothing compares it with the game yet. Embedded
  elements are placed too (Recovered, "Embedded elements").
- [ ] **The text job** (`0x2f7db0`, job type 0). The glyphs are ported from the
  disassembly (`text_job`, `glyph_quad` in `src/rui/text.rs`; behaviour under Recovered,
  "Fonts and text"). The images in the text (a box per image through the emitter, a
  nine-slice one too) and the embedded elements (Recovered, "Embedded elements") are drawn
  before the glyphs. Not modelled: the parent's colour matrix in an embedded element's
  styles. Checked by probing `compass_flat` (`playerAngles 0,37,0`: one quad for "53", 946 to 975 by
  94 to 115 px at 1080p, its texture coordinates inside both glyphs' bounds) and by eye in
  `vmshot` (the tactical panel's "7 sec"); not against the game.
- [x] **The shader's text mode** (`game/src/rui.wgsl`, `glyph`, `shadow`, `mode == 0`), from
  the full disassembly. The shell uploads the font textures as R8 and each font's glyph
  bounds (binding 5), and batches text quads by font atlas. Runs in `vmshot`: the panel's
  countdown draws, light grey with a dark edge. Not compared with the game.
- [x] **`#TOKEN` text** (`+0x48`, `0x2fb6e0`): the localization files read at start
  (`src/assets/localize.rs`, `Ui::localize`), a token found as the engine's table finds it
  (Recovered, "Localized text"). Checked by its unit tests and the panel's countdown
  (`#HUD_ABILITY_RECHARGE_REMAINING_SEC`, "%s1 sec").
- [x] **`%token%` in a string** (the layout's substitution, `0x2f4c70`): the token table
  (Recovered, "Key names in text") built by `lurch::rui::tokens::Tokens` from the shell's
  binds (`Binds::game_binds`: the key codes, and the commands as the game's default config
  binds them) and `scripts/keys_keyboard.rson`; `{a|b}`, `[a|b]`, `&` and `@` as the
  substitution reads them. Its unit test pins `%use%`, `%&use%`, a held bind and an unbound
  name. Not modelled: the entries named after keys and the engine's aliases.

### The tactical's HUD and the zipline prompt

What the user asked for on 2026-10-12: the grapple's ability panel at the bottom left of
centre, and the use prompt on a zipline, both as the game draws them.

- [ ] **`tactical_ability_hud`** (`ui.dll+0x93e80`). `game/src/rui.rs` creates it in draw
  group 7 at `HUD_Z_BASE` with the arguments `InitOffhandRui` gives a `cooldown_type ammo`
  weapon (below), the legend's colours from its character item flavour (`Loadout::character`,
  the one whose `setFile` is the legend's settings), `chargeFrac` from the grapple's clip
  and `refillRate` from its regen. Drawn in `vmshot`: the slanted box, icon, border, the
  fill, the countdown and the key hint (`%offhand1%`, the `Q` keycap). The backdrop, the
  blurred scene behind the HUD masked into the panel's shape, draws since 2026-10-14
  (`game/src/blur.rs`; Recovered, "The special image source and the screen blur"). Still
  needed: a check against the game at 1080p.
  `readyFrac`, `dryfireFrac` and `useFrac` are 0: their tracks are not ported.
- [x] **The grapple's cooldown.** `game/src/cooldown.rs`: the grapple weapon's clip, run
  each command after movement (`run_cmds`), emptied by the script's drain when the hook
  catches and refilled by the engine's regen (Recovered, "The grapple's clip"). A grapple
  starts only with a full clip (`ammo_min_to_fire`); a press while grappling still detaches,
  as before. A respawn refills it. The crosshair's `ammoFrac` and the panel's `chargeFrac`
  read it (`RUI_TRACK_WEAPON_CLIP_AMMO_FRACTION`, taken to be the clip over its size).
  Checked by `tests/shell/cooldown.rs` (a catch with no travel costs 100, 10 s refill 100)
  and in `vmshot`; not against the game. The suit's grapple power is not what limits a
  grapple in Apex: `power_use_rate` is 0, so it stays at 100.
- [x] **The zipline prompt.** Engine code, not script (Recovered, "The use prompt
  (engine)"): `zipline_flyout` made each frame the use search finds a zipline (`flyout` in
  `game/src/rui.rs`, `Pilot::use_point`), in draw group 9 on the screen in pixels, with the
  HUD globals' view from the camera (`Globals::set_view`; the field of view's tangent is the
  shell's camera's). Its quads are moved onto the HUD's plane, which the shell draws every
  quad through. It hides with the HUD (F1). `ping` is bound to the middle mouse button as the
  game's default config binds it (`Action::Ping`, which does nothing in lurch), so "Ping"
  shows that button. `hint_display`, the prompt for every other usable, is not made: the
  test map has nothing else to use. Checked in `vmshot` beside "zipline down" against the
  user's screenshot (same parts and layout within a few pixels, at half its resolution);
  not yet at 1080p in the game.

### Crosshair and grapple

- [x] **Crosshairs from the weapon script.** `lurch::assets::script::crosshairs` reads
  `RUI_CrosshairData` (`DefaultArgs`, each `Crosshair_N` with its RUI and `Args`, bases
  merged); `game/src/rui.rs` creates the fists' and the grapple's and sets each argument from
  the value it is bound to (`weapon_value`, the engine's names at `0x1477700`; values lurch
  does not model are 0 or false). Which crosshairs show is inferred from the settings'
  names, not ported: a weapon in use shows `active_crosshair_count` from
  `rui_crosshair_index`; an offhand shows those from
  `ordnance_crosshair_always_on_start_index` on at all times. The engine's own choice is in
  the client's crosshair code (find it from the weapon settings `0x8e7869` parses).
- [x] **The grapple's crosshair.** `ui/crosshair_grapple` runs from `ui.dll` with lurch's
  in-range answer and times, and draws over the view in place of our ring (kept for runs
  without an install). Checked in `vmshot` frames: blue and pulsing in range, faint yellow
  out of range. `ammoFrac` is the grapple's clip fraction (`cooldown.rs`); `adsFrac` (the
  eighth argument) and the crosshair movement are 0.
  `ui/crosshair_grapple_charge` (shown while the tactical is used) is not drawn yet.
- [x] **The fists' crosshair**, `ui/crosshair_dot`, through the weapon script, in place of
  our white dot (kept for runs without an install).

### HUD

- [ ] **What the HUD is made of.** From the client scripts (`cl_main_hud.nut`,
  `client/rui/cl_hud.gnut`, `pilot/cl_pilot_health_hud.gnut`, `sh_minimap.gnut`, ...): each
  RUI the HUD creates, its draw group, sort key, and the arguments set or tracked.
- [ ] **Tracks.** `RuiTrack*` binds an argument to a game value (`RUI_TRACK_*`); port the
  ones the HUD uses for values lurch has (health, shields, grapple power, ability charge).
- [ ] **Compass** (`compass_flat`).
- [ ] **Health and shields.** The panel is `survival_player_hud_v2` (`SURVIVAL_HUD_PLAYER`,
  `sh_consts.gnut`), set up by `SURVIVAL_PopulatePlayerInfoRui` (`cl_gamemode_survival.nut`):
  `playerIcon` is the character's `galleryPortrait`, `shieldFrac` the armour tiers' capacities
  over 100 with tier 3's (100) as alpha. It runs and draws with no unported step. Its draws
  come in job order: backdrop, portrait, frame, team-colour edge, then the name's text and the
  bars. Only the Minecraft host (`rui::HostHud`) draws it, and only up to the name. Seen in
  `vmshot` (2026-10-17), not explained: with `useShields`, `playerShieldFrac` 0.5 to 1 and
  `shieldFrac` set as the script does, no shield bar draws.
- [ ] **Abilities**: tactical (the grapple's charge), ultimate, passive. The ultimate's panel is
  `ability_hud` (`ultimate_ability_hud` only with the playlist's `hud_ultimate_ready_video`).
  It draws at the bottom centre, and the tactical's left of it. With `InitOffhandRui`'s
  arguments both draw with no unported step (`ultimate_ability_hud` stops on job type 3).
- [ ] **Weapon and ammo panel** for the fists.
- [ ] **Minimap.** The test map has no overview image; decide what it shows.

### Menus: the pause menu and the settings menu

What the user asked for on 2026-10-13: Escape in a match opens the game's in-match menu
(`SystemMenu`) with its two buttons, Settings and Leave Match, and Settings opens the
game's whole settings menu (`MiscMenu`), 1:1: every tab, every setting, each backed by its
ConVar whether lurch does anything with it or not (Close Deathbox Menu on Damage, for
instance, is a ConVar and a working switch that changes nothing). Leave Match does nothing
for now (the user's choice; the game opens a confirm dialog that leaves). Everything the
menus show is listed under Recovered, "The pause and settings menus"; the tasks below are
in order.

R5Reloaded loads its menus from `platform/scripts/resource/ui/menus/` (`_menus.nut`,
`AddMenu`); the copy under `platform/resource/ui/menus/` is an older one nothing loads. The
templates a layout names with `InheritProperties`, and the child groups
(`childGroupAlways`), are in `platform/resource/basemodui_scheme.res`.

Groundwork:

- [x] **Inventory.** Every menu, tab, row, label token, ConVar, option list and the
  ConVar's image default: Recovered, "The pause and settings menus".
- [x] **`tools/convars.py`.** Lists the image's ConVars (name, default, live value,
  flags, range) by their vtable. Nine of the menu's ConVars are not in the image: R5Reloaded
  registers them (Recovered, "The ConVar files"). `gameCursor_velocity` is the image's
  `gameCursor_Velocity`.
- [x] **ConVars in the shell.** Done 2026-10-14: `game/src/convar.rs` (`ConVars`,
  `ConVarPlugin`). Its 85 ConVars are the inventory's, with the image's default, flags and
  range, or R5Reloaded's for nine. Values are set as the engine sets them, from a string, a
  float or an int. Files and saving follow the game (Recovered, "The ConVar files"). The user
  chose (2026-10-14) that lurch keeps its own `settings.cfg` and `profile.cfg` next to
  `lurch.cfg`. On the first run, with neither there, it reads the game's from Saved Games and
  writes them as its own; it never writes the game's. Scripted runs (`--script`, `vmshot`)
  read and write nothing. `tests/shell/convar.rs` checks the setters, the format, the save
  rule and the first run. On the guest, with the user's own files, every line both had came
  out byte for byte and in the same order (120 in `settings.cfg`, 57 in `profile.cfg`).
- [x] **The ConVars lurch acts on.** Done 2026-10-14 (`ConVars::apply`, `act` in
  `game/src/convar.rs`). The user decided: the saved files win over `lurch.cfg`, and only a
  flag on the command line itself (`Config::cli`) wins over them.
  - Applied: `mouse_sensitivity`, `m_invert_pitch` (`Look::invert`), `cl_fovScale`,
    `player_setting_autosprint` (1 on; modes 2 and 3 are not modelled and count as off).
  - `fps_max` through the engine's cap (`limiter::engine_cap`, Recovered, "The frame cap"),
    the monitor's refresh from Bevy.
  - The volumes: `sound_volume` as Miles' master volume, the other five ×100 as their
    controllers.
  - The key binds: the movement and ping commands of `settings.cfg`'s `bind_US_standard`
    lines replace lurch's, by the engine's key names (`binds.rs`, `KEY_NAMES`, `0x1309708`).
    lurch's own actions (respawn, spots, HUD) stay. The wheel's binds give `wheel_up` and
    `wheel_down`.
  - Taken again whenever the settings change.
  - Not modelled: held binds (`bind_held_US_standard`), `m_acceleration`, the zoom scalars,
    the voice's focus muting. The prompts' key hints take the binds at start only.
  - Checked by `tests/shell/convar.rs` (the command-line rule, binds, the cap); not in a
    window yet.

VGUI, the engine's panels the menus are made of:

- [x] **Layout files.** Done 2026-10-14 (Recovered, "KeyValues and the layout files").
  `src/assets/keyvalues.rs` parses KeyValues with the engine's tokenizer and evaluates
  conditions with its grammar and PC symbols; `src/assets/vgui.rs` loads `Scheme` (templates
  resolved) and `load` builds `Panel`s: own keys, then the template's missing ones, children
  from an EditablePanel's blocks, a CNestedPanel's `controlSettingsFile` and a button's seven
  `childGroup*` keys. The Mouse/Keyboard and Video tabs' content files are commented out in
  `settings.res`; their engine panels fill them (their own tasks).
  - Not modelled: `#base`/`#include` (only `ingame_mp.menu` has one), a value's int or float
    type, the `$CVAR_` symbols, a second block for a panel already made (replaces its
    settings here).
  - The scheme's two lookups (`InheritableProperties`, `ButtonChildGroups` by name) are read
    from how they are used, not decompiled.
- [x] **Layout.** Done 2026-10-14 (Recovered, "Layout and Solve"): `src/vgui.rs`, `layout`
  turns the panel tree into `Placed` panels (position, size, `zpos`, `visible`, `clip`, pin,
  then the absolute position and clip rectangle Solve gives). Checked by a unit test and by
  `menus_lay_out_as_vpanel_solves_them` (the pause menu's buttons 16 below the centre and 8
  apart; the Gameplay tab's rows from (160, 128), 2 apart).
  - Not modelled: `AutoResize` and its offsets (but a slider's; Recovered, "SliderControl"),
    title-safe insets, insets and minimum sizes, a scheme's own proportional panel, the
    default size of a panel without `wide`/`tall` (0 here; a new panel is 64 × 24,
    `0x4c2d50`, until `auto_wide_tocontents` sizes it). Every panel counts as proportional;
    where the menus are set so was not traced.
  - The steady state is solved: VPanel's Solve reads a sibling where the last frame left it,
    so the engine needs a frame or two to settle a pin to a later sibling.
- Panel kinds (split 2026-10-14; Recovered, "RuiPanel and RuiButton"):
  - [x] **RuiPanel**: the element `rui` names, `clipRui`, `ruiArgs` by their KeyValues type,
    and what paint sets each frame (resolution, topology, `actualRes`, `clipRect`). Done
    2026-10-14: `element`, `create_rui` and `paint_rui` in `src/vgui.rs`, the value types in
    `keyvalues::kind`; `menu_panels_make_their_elements_with_their_arguments` in
    `tests/rui.rs`. Not drawn yet: that waits for "Paint order" and the menus in the shell.
  - [x] **RuiButton**: as RuiPanel, plus `buttonText` from the label and the state arguments
    (`isDisabled`, `isFocused`, `isSelected`, `isLocked`, `isNew`) on it and on its Always
    group's RUI children; `behave_as_label`, `ruiDefaultHeight`, `ruiFont`. Done 2026-10-14
    as functions (`label_text`, `paint_button_text`, `STATE_ARGS`, `set_state`), checked by
    `buttons_take_their_label_and_states`. When the states change belongs to "Input". Not
    modelled: `ruiDefaultHeight` and `ruiFont`, which size a label-like button's text for
    `auto_wide_tocontents` (with "Label").
  - [x] **ImagePanel**: `image`, `drawColor`, `scaleImage`. Done 2026-10-14 as paints
    (`Paint::Fill`, `Paint::Image` from `paint_list` in `src/vgui.rs`): the menus' images are
    the textures `texture/NAME.rpak` in `common_early.rpak`. Not modelled: tiling, rotation,
    frames, `scaleAmount`, an image drawn at its own size (no menu image is), scheme colour
    names. Not drawn by the shell yet.
  - [x] **Label**: the scheme's fonts, `fgcolor_override`, `textAlignment`, `allcaps`,
    `auto_wide_tocontents`. Done 2026-10-16 through TextImage's RUI path (Recovered, "VGUI's
    text"): `src/vgui/label.rs`, drawn by `Menu::draw` as `drawtext_vgui`; a RuiButton with
    `behave_as_label` and `auto_wide_tocontents` sized as `0x939100` sizes it. Checked by
    `labels_draw_their_text_through_rui`, `colours_go_from_srgb_to_linear_as_the_engine_does`
    and `a_label_places_its_text_by_alignment_and_inset`. Not modelled: `wrap`, `centerwrap`,
    `^` colour codes, `associate`, the disabled look.
  - [x] **Paint order**: parents before children, children by `zpos`; what clips a paint.
    Done 2026-10-14: `paint_list`, checked by `panels_paint_by_zpos_then_file_order` and
    `menus_paint_in_zpos_order`. Assumed, not traced: a queued RUI (`0x71ff70`) draws where
    its panel paints, between the surface's own draws.
- Input (split 2026-10-14; Recovered, "Input"):
  - [x] **The panel under the cursor and the button states.** `panel_at`, `focused`,
    `unfocused`, `NEW_BUTTON` in `src/vgui.rs`, checked by
    `the_cursor_finds_the_topmost_panel_and_buttons_change_state`. Not modelled: `polyShape`
    hit tests (the settings tabs have one).
  - [ ] **In the shell**: a cursor while a menu is open, focus under it, clicks,
    `navUp`/`navDown` with the arrow keys, Escape as navigate back, the wheel over a scroll
    panel. No usercmd is made while a menu is open, as now while paused. In part, 2026-10-14:
    the cursor (shown while paused, put at the centre on opening), focus under it, a click on
    a button, Escape opening and closing (`cmd.rs`, `pause_input`; `menu_cursor` and `draw` in
    `rui.rs`). Open: the arrow keys and `navUp`/`navDown`, the wheel, a keyboard focus.
    Script steps `menu`, `cursor X Y` and `click` drive it for `vmshot`.
- [x] **Menu sounds** through Miles: `UI_Menu_Focus`, `UI_Menu_Accept`,
  `UI_Menu_Focus_Large`, `UI_Menu_SettingsTab_Select`, and what a menu plays on opening and
  closing (`_menus.nut`). Done 2026-10-16 (Recovered, "Menu sounds"; `Menu::sounds` in
  `src/vgui.rs`, played by `game/src/audio.rs`), checked by the menu tests. Neither menu
  plays a sound on opening or closing in the scripts read. Not modelled: the keyboard's
  sounds (a button's keys, `0x9484d0`, among them `UI_Menu_Back`), the tabs' shoulder keys
  (`UI_Menu_ApexTab_Select`, `_tabs.nut`), a slider's arrow keys.

The RUI the menus use (census of 2026-10-13: 10 of the 19 elements run and draw fully):

- [x] **`+0x50` upper case** (`0x2fb7b0`): `generic_button`, `tab_button_lobby`,
  `generic_tooltip`, `keybindlist_overlay` stop on it. Done 2026-10-14 (`upper`, `decode` in
  `src/rui/api.rs`; Recovered, "The callback table"). Census: 1002 of 1405 elements run and
  draw fully (872 before). Of the menus' elements only `tab_button_lobby` still stops (opcode
  11).
- [x] **The blur behind a menu**: the special image source (`0x2000`, `0x2f7c50`) that
  `screen_blur` and `tabs_background` draw (and the tactical panel's backdrop), and the
  blurred copy of the scene it reads. Done 2026-10-14 (Recovered, "The special image source
  and the screen blur"). Both elements run and draw fully in the census. A menu's own camera
  setup (which camera's frame the blur reads while a menu is open) is decided with the menus.
- [ ] **Transform opcodes 11 and 12**: `shoulder_navigation_shortcut_angle`, `basic_image`.

The pause menu:

- [ ] **`SystemMenu` on Escape** (`system.menu`, `system.res`, `menu_system.nut`). Drawn
  2026-10-14 (`Menu`, `system_menu` in `src/vgui.rs`, checked by
  `the_system_menu_shows_two_buttons_and_focuses_under_the_cursor`; the shell's draw in
  `rui.rs`): the blur, the cover, `#SETTINGS` and `#LEAVE_MATCH` on `generic_button`, the other
  buttons hidden, the hovered one focused. Seen in `vmshot` on the host (960 × 540): the cover
  halves the scene's brightness (sky 140, 178, 217 to 67, 87, 108); the unfocused buttons have
  a grey edge, the focused one an orange-red edge (white and orange the other way round
  until 2026-10-16, when the clock was frozen). A click on a button closes the menu, then
  Settings opens `MiscMenu` (2026-10-16); Leave Match does nothing. Open: the footer is not
  drawn (its buttons have no width until Label). Not compared with the game. Blur
  (`ScreenFrame`, `ui/screen_blur`), `Cover` (`vgui/HUD/white`, black at alpha 200, twice the
  screen, centred), `SystemPanel` with `Button0` `#SETTINGS` and `Button1` `#LEAVE_MATCH`
  (`ui/generic_button`, 376 × 60, the first 16 below the centre, then 8 apart), `Button2` to
  `Button6` hidden (an empty label hides a button), `DataCenter` empty in a match. The cursor
  moves to the screen's centre on opening. Escape again closes it (`OnSystemMenu_NavigateBack`);
  a button closes it before acting (`OnButton_Activate`). Settings opens `MiscMenu`;
  Leave Match does nothing.
- [ ] **Footer buttons** (`footer_buttons.res`, `_footer.nut`, `LeftRuiFooterButton`):
  the pause menu's `#B_BUTTON_BACK`, the settings menu's `#B_BUTTON_CLOSE`. Drawn 2026-10-16
  (`Menu::set_footer`): `UpdateFooterOptions` takes the top active panel's options (a tab's
  body over `SettingsPanel`, whose Close never shows on PC while a tab is up), with the
  mouse their mouse labels. Pause menu: Back, Dev Menu (`ShouldShowDevMenu` is true outside
  the lobby). Each tab: Back, Restore Defaults (`#FOOTER_CHOICE_HINT` has no mouse label).
  Open: a click on them (Back navigates back; Dev Menu, Restore Defaults and their dialogs
  are not made), the Audio and Video tabs' Apply once a setting changed.

The settings menu:

- [ ] **`MiscMenu`'s frame** (`misc.menu`, `menu_misc.nut`): blur, `Vignette`
  (`rui/menu/common/menu_vignette`), `Logo` (`ui/tab_logo`), `MatchmakingStatus`,
  `ToolTip`, `ButtonTooltip`, footer; `SettingsPanel` (`SettingsPanelFull`, `settings.res`).
  In part, 2026-10-16: `SystemMenu`'s Settings opens it (`OnButton_Activate` closes
  `SystemMenu`, then `OpenSettingsMenu` advances to `MiscMenu`), with `SettingsPanel` shown as
  `OnMiscMenu_Open` does (`misc_menu` in `src/vgui.rs`, checked by
  `the_settings_menu_shows_its_settings_panel`; `MenuOpen` names the menu on top in the
  shell). Escape closes it back to the game, as `CloseAllMenus` and its own navigate-back
  both end there in a match. Seen in `vmshot`: the blur, cover, vignette, logo, the tab
  strip's background and the details panel's empty box. Open: `MatchmakingStatus` and the
  unused footer buttons are not hidden as the scripts hide them; nothing of the menu acts on a
  click.
- [ ] **Tabs** (`tabs_settings.res`, `_tabs.nut`, `lobby_panel_settings.nut`): `#HUD`,
  `#MOUSE_KEYBOARD`, `#CONTROLLER`, `#VIDEO`, `#AUDIO` on `ui/tab_button_lobby` over
  `ui/tabs_background`, the shoulder hints (`ui/shoulder_navigation_shortcut_angle`), the keys
  that switch tabs, `UI_Menu_SettingsTab_Select`; the tab last shown opens again.
- [ ] **Details panel** (`ui/settings_details`): `selectionText`, `descText`, `detailImage`
  (the tab's `rui/menu/settings/settings_*` with nothing focused), `showCbInfo`.
- [ ] **Switch rows** (`SwitchButton`: `ui/settings_base_button`): `ChoiceButtonAlways`
  (two `ui/settings_choice_button`) or `MultiChoiceButtonAlways` (two
  `ui/settings_change_button` around `ui/settings_multichoice_value`), the `list` of label
  and value, the ConVar written on a change, `nonDefaultColor` off the default.
- Slider rows (split 2026-10-16):
  - [x] **SliderControl** (an engine class, Recovered "SliderControl"): its children from
    `SliderControl.res`, `conCommand`, `minValue`, `maxValue`, `stepSize`, the bar, the value
    label, the default's mark, the highlight, a press and a drag. Done 2026-10-16
    (`src/vgui/slider.rs`, `Menu::sync_sliders`, `press`, `drag`, `release`); checked by
    `sliders_follow_their_convar_and_the_cursor`. Not modelled: the arrow keys (`0x94f160`).
    `UI_Menu_Accept` on a change is done with "Menu sounds".
  - [ ] **The `TextEntry` beside a slider** (`SliderControlTextEntrySmall`,
    `showConVarAsFloat`): the Mouse/Keyboard tab's sensitivities and the ADS menu.
- [x] **Scrolling** (`ScrollFrame`, `ScrollBar` `ui/survival_scroll_bar`,
  `_scroll_panel.gnut`): the Gameplay tab's content is 1660 tall in an 836 frame. Done
  2026-10-16 (`SettingsMenu::scroll`, `wheel`, `drag`; Recovered, "Scroll panels"), checked by
  the settings test. Not modelled: scrolling to the focused row, which the script does only
  without the game cursor (`gameCursor_ModeActive` 0, or 2 with a gamepad).
- [ ] **Subheaders**: `SubheaderBackgroundWide` (`ui/menu/common/menu_header_bar_wide`),
  `SubheaderText`. The text draws (2026-10-16, Label). Open: the bar draws solid black here
  (its texture is opaque black with a light line along its bottom; how the surface blends a
  VGUI image is not recovered); compare with the game.
- [ ] **Gameplay tab** (`#HUD`; `hud_options.res`, `panel_hud_options.nut`).
- [ ] **Mouse/Keyboard tab** (`controls_pc.res`, `panel_controls_pc.nut`; the key bind list's
  recovered part: Recovered, "The key bind list"), with the ADS
  sensitivity menu (`controls_ads_pc.menu`, `ads_controls_pc.res`) and the key bind list:
  an engine panel (`CreateKeyBindingPanel`, string `0x141c888`; `listpanel_keybindlist`
  `0x149e798`; the action list `scripts/kb_act.lst`, `0x149ebd0`; `ui/keybindlist_overlay`,
  `ui/keybindlist_message`). A new bind changes the shell's binds.
  In part, 2026-10-16: the tab's body is made as the engine makes it (`SettingsMenu::load`;
  Recovered, "The engine's tab bodies"): the sensitivity slider, the ADS menu's button and the
  three switches draw and act. Not yet: the TextEntry beside the slider. Its `background_keybindings`
  (`ui/basic_image` with no `basicImage`) draws nothing here, as the image job skips an empty
  image name; compare with the game.
  The key bind list, later on 2026-10-16 (Recovered, "The key bind list"; `src/vgui/keybinds.rs`,
  `SettingsMenu::list_*`, the shell's `KeyTrap`): its sections and items from `kb_act.lst`,
  filled in from `settings.cfg`'s binds when the tab shows; a click on a key column captures
  the next key or button (Escape cancels), a right click clears it, Delete clears the item last
  captured into; the binds change as the engine's commands change them, the shell's binds and
  the key names in text follow, and hiding the tab saves `settings.cfg`. Checked by
  `the_list_fills_in_and_binds_as_the_panel_does`,
  `the_key_bind_list_shows_the_binds_and_captures_a_new_key` and in `vmshot` (a capture, then
  the key's keycap in its column). Not yet: the scroll bar's look, the bind messages
  (`KeyBindMessage`), restoring the defaults, the text clipped at the list's edges.
- [ ] **Controller tab** (`controls.res`, `panel_controls_gamepad.nut`), with
  `GamepadLayoutMenu` (`gamepadlayout.menu`) and the advanced look menu
  (`controls_advanced_look.menu`, `advanced_look_controls.res`). ConVars only: lurch reads
  no gamepad.
- [ ] **Video tab**: an engine panel, `VideoOptionsPanel` (`CreateVideoOptionsPanel`,
  string `0x141c958`; its rows' names and option labels at `0x14a1498` to `0x14a1c00`), laid
  out by `video.res`. Recover each row's options and ConVar from its code; apply and discard
  (`uiGlobal.videoSettingsChanged`, `DiscardVideoSettingsDialog`). In part, 2026-10-16: the
  body is made from `video.res` (Recovered, "The engine's tab bodies"); its rows draw with their
  titles and the sliders act, but the switches' choices, which the constructor fills, are
  empty.
- [ ] **Audio tab** (`audio.res`, `panel_sound.nut`), with discard
  (`uiGlobal.SoundSettingsChanged`, `DiscardAudioSettingsDialog`).
- [ ] **Saving on close** (`OnSettingsPanel_Hide`: `SaveSettingsConVars` for each tab) and
  the confirm dialog the discards use (`confirm_dialog.menu`).
- [ ] **Checked against the game**: each menu and tab beside a 1080p screenshot of
  R5Reloaded in the guest (the user starts it), rendered by `vmshot` with a script that
  presses Escape.

## Recovered

### The `ui` asset (version 39)

Registered with header size `0x68` and a native size of `0x78` (`0x2f467d`); the load
callback (`0x2f3cc0`) looks up two exports of the pak's DLL by name with GetProcAddress:
the element's name at `+0x68` and `name__hidden` at `+0x70`. The DLL comes from a table of
one module per loaded pak (`0x27d40c20`, stride `0xb8`, by pak index): `ui.rpak` loads
`paks/Win64/ui.dll`.

| Offset | Type | What |
|---|---|---|
| `+0x00` | pointer | name (`crosshair_grapple`) |
| `+0x08` | pointer | the data block's defaults, `+0x46` bytes |
| `+0x10` | pointer | transform code, up to the next block |
| `+0x18` | 2 × f32 | the size the element is laid out at (1920, 1080 for the HUD's) |
| `+0x20` | 2 × f32 | its reciprocal |
| `+0x28` | pointer | argument names; null in this build |
| `+0x30` | pointer | argument clusters, `0x12` bytes each (one in every element here) |
| `+0x38` | pointer | arguments, 8 bytes each |
| `+0x40` | u16 | argument count |
| `+0x42` | u16 | mapping count |
| `+0x44` | u16 | data block size |
| `+0x46` | u16 | defaults size |
| `+0x48` | u16 | style count |
| `+0x4a` | u16 | transforms the code makes, counting the three every element has |
| `+0x4c` | u16 | job count |
| `+0x4e` | u16 | cluster elements in the data block |
| `+0x50` | pointer | styles, `0x44` bytes each |
| `+0x58` | pointer | draw jobs |
| `+0x60` | pointer | mappings, 16 bytes each |

The blocks follow each other in the pak in the order name, defaults, code, styles, jobs,
clusters, arguments, mappings, mapping data (checked over all 1405 elements).

An argument: type byte, element byte, u16 offset of its value in the data block, u16 name
offset (into the absent name table), u16 check. A cluster: u16 first argument, u16 count,
byte multiplier, byte addend, then at `+0xa` the bytes per element. Lookup (`0x301840`):
over the name's bytes (signed), `h = (c + mul * h + add) ^ (h >> 20)`, starting from the
element byte; the slot is `first + (h & (count - 1))`; the entry must hold `h >> 4` in its
check, the asked type (type 1 also matches 12) and the element byte. Types seen: 3 bool,
4 int, 5 float, 8 colour and alpha, 9 game time, 12 image (`RuiSetImage`, `0x7a4930`; a
`RuiTrackImage` argument such as the panel's `hudIcon` has it). Creation (`0x720fc0`) points every type 1, 2, 12 and 13 argument at an
empty string.

A style (`0x44` bytes): u16 at `+0` (1 in `crosshair_grapple`), then data offsets of the
colour (`+2`, `+4`, `+6`) and alpha (`+8`); the type-1 job skips its draw when that alpha is
not above zero; the text job reads a font at `+0x2c` and sizes at `+0x36`, `+0x38`, `+0x40`.

A job starts with u16 type and u16 transform; sizes by type `0x16, 0x2e, 0x1c, 0x1c, 0x2e,
0xc` (`0x1300a70`); handlers by type at `0x1300bb0`: 0 text (`0x2f7db0`), 1 image
(`0x2f9570`), 2 (`0x2f9d50`), 3 (`0x2fa330`), 4 (`0x2fa7e0`), 5 a nested element
(`0x2fb000`). Over all elements: 8147 type 1, 2872 type 0, 324 type 2, 118 type 4, 80
type 5, 11 type 3. A type-1 job: `+4` u16 (passed on to the emitter), `+6` data offset of
the image index (−1 none, `0x2000` a special source through `0x2f7c50`), `+8` a second
image (a mask), `+0xa` to `+0x18` data offsets of the image's crop and placement, `+0x1a`
to `+0x26` those of a rotation and offset (sine and cosine by the engine's polynomial),
`+0x28` a bool, `+0x2a` u16 flags, `+0x2c` the style byte.

A mapping: u32 key count, u16 outputs per key, u16 cubic, pointer to the keys then the
outputs (twice as many, value and slope per key, when cubic). Evaluated by `0x2fc060`:
below the first key the first segment at `t = 0`; at or past the last, the last segment at
`t = 1`; else the segment holding `x` at `t = (x - k0) / (k1 - k0)`; a line weighs
`1 - t, t`, a cubic the Hermite basis with the slopes times the key gap. `0x2fc1b0` sums one
output in a fixed order.

### Instances and a frame

An instance (`0x2fc550`): the asset, its size and reciprocal (which `RuiSetResolution`
changes), the draw scratch at `+0x18`, last frame's draw box at `+0x20`, the time stamp
counter at creation at `+0x38` (it seeds the random numbers, `0x2fc5b0`), flags at `+0x48`
(destroy) and `+0x49` (hidden this frame), `+0x4a` (no safe area), the topology at `+0x58`,
then the data block at `+0x60`.

`Rui_Draw` (`0x2fc670`; R5Reloaded's SDK detours it, to honour `rui_drawEnable`) sets up
three transforms before the element's function runs: 0 the unit square at the element's
size (the job worker `0x2fd300` passes the asset's size, basis identity), 1 the same inset by
the safe area for the HUD draw groups (mask `0xe40`; ConVar `rui_safeAreaFrac`, image value
0, times 0.05), 2 the element's own aspect fitted into the rect. Transform code then adds
more. After the function, each job's transform extends the instance's draw box. The
function's arguments: the callback table, the draw group's globals, the instance, the data
block.

Draw groups (script constants, `0x77e284`): `RUI_DRAW_WORLD` 2, `RUI_DRAW_VIEW_MODEL` 3,
`RUI_DRAW_COCKPIT` 5, `RUI_DRAW_HUD` 6, `RUI_DRAW_POSTEFFECTS` 7.

The globals (`0x720100`, per draw group, `0x100` bytes): copied from a template at
`0xc05f400` (live values in the image: the clock at `+0x90`, `+0xa8` 1, a
colour at `+0xc8` and again at `+0xe0`, −60 at `+0xec`), then the time at `+0x98`, the
viewport at `+0x48` and `+0x4c`, and projection terms. HUD elements draw only while `+0xa8`
is set; `+0xac`, `+0xb0` and `+0xc4` are other flags they test, not yet named.

The globals' clock: `+0x90` is a u64, the performance counter in microseconds (`0x2f1c90`:
QueryPerformanceCounter times 1,000,000 over the frequency, which it reads once). A frame
function (`0x22ab50`) stores it in the template first thing each frame. The image holds
`0x4_32be_4adb` there (about 18,031 s), which was read as two flags until 2026-10-16.
Elements time their own transitions by it. `generic_button` keeps, for `isDisabled`,
`isFocused`, `isSelected` and the bool at `+0x38`, the value it last saw and the clock when
that value changed. Each state's weight is a smoothstep (`(3 - 2x) x²`) of the time since its
change over 0.15 s, rising when the state is on and falling when it is off. The last-seen
slots default to 1 for the three states (0 for `+0x38`), so a new instance starts as if each
state had just turned off. Its edge colour lerps from grey to orange-red (`+0xc0`, `+0xd0`)
by the focus weight. lurch's shell sets the clock from its real time since start, not from
boot.

The HUD's topology (`cl_mapspawn.gnut`, `BuildTopologyCreateData`): a plane from
`(xOffset, 0)` along `(scaledWidth, 0)` and `(0, scaledHeight)` in screen pixels, where the
virtual screen is 1920 × 1080 at 16:9 (1080 × aspect wide when wider), scaled to the screen's
height and centred.

### The callback table (`0x1300ad0`)

| Offset | RVA | What | Ported |
|---|---|---|---|
| `+0x00` | `0x2fb430` | hide and destroy (`+0x48`, `+0x49`) | yes |
| `+0x08` | `0x2fb440` | hide this frame (`+0x49`) | yes |
| `+0x10` | outside the image | report an error the code found (division by zero, with the source line) | yes: logged, frame hidden (assumed) |
| `+0x18` | `0x2fb450` | the frame's sizes, for the code to fill | yes |
| `+0x20` | `0x2f55e0` | lay out a text job | yes |
| `+0x28` | `0x2f6540` | box of transforms `[a, b)`, then move `[c, b)` by its corner | yes |
| `+0x30` | `0x2f31d0` | run the transform code to an offset | yes |
| `+0x38` | `0x2f3230` | the same with the unit size at one | yes |
| `+0x40` | `0x2fb460` | sprintf into the frame's string buffer (`0x2008` bytes) | yes |
| `+0x48` | `0x2fb6e0` | a localized text with arguments (`0x2fb4e0`), `***` on failure | yes |
| `+0x50` | `0x2fb7b0` | upper-case UTF-8 | yes |
| `+0x58` | `0x2fb900` | a number with the locale's separators | no |
| `+0x60` | `0x270130` | sRGB to linear | yes |
| `+0x68` | `0x2fbbc0` | nothing | yes |
| `+0x70` | `0x2fbca0` | a random number from the frame's seeds | no |
| `+0x78` | `0x2fbcf0` | project a point through the topology | no |
| `+0x80` | `0x2fbda0` | not identified | no |
| `+0x88` | `0x2fbdd0` | a nested instance's size | no |
| `+0x90` | `0x2fc420` | image index by name | yes |
| `+0x98` | `0x2fc000` | an image as a private-use character, for text | no |
| `+0xa0` | `0x2fc1b0` | mapping, one output | yes |
| `+0xa8`, `+0xb0`, `+0xb8` | `0x2fc220`, `0x2fc2c0`, `0x2fc380` | mappings, more outputs | yes |
| `+0xc0` | `0x2fbe20` | a nested instance's size in 1920 × 1080 units | no |
| `+0xc8` | `0x2f3430` | a font's slot by name | yes |
| `+0xd0` | `0x2fc430` | palette colour, linear | yes |
| `+0xd8` | `0x2fc4f0` | 1920 over the screen's width at 16:9 | no |

Upper case (`0x2fb7b0`): an empty string comes back as it is. Otherwise each code point is
decoded (`0x437dd0`: 2 bytes from a lead above `0xc1`, 3 bytes not a surrogate, 4 bytes below
`0x110000`, each continuation `10xxxxxx`; anything else is 0 and does not advance), cut to 16
bits, put through the CRT's `towupper` (`0x11c33e8`) and written back as 1 to 3 bytes at the
string buffer's end (frame `+0x1598` the bytes used, `+0x159c` the buffer). Before each code
point, a buffer past `0x2003` bytes gives back the string as it was. When nothing changed the
string comes back and the buffer is not taken; otherwise the copy, terminated. `towupper` in
the C locale maps only `a` to `z`; under another locale it uses the locale's tables
(`LCMapStringEx`). No `setlocale` name is in the image, so the C locale is taken, not checked.

The colour (`0x2fc430`): row `a + b` (negatives read as 0, past the end row 0) of
`datatable/colorpalette_table` (`common_early.rpak`; columns `colorName`, `default`,
`protanopia`, `deuteranopia`, `trianopia`, `qa`, 0 to 255) at the column the global at
`0x27ed8590` names (1, `default`), times 1/255, through `0x270130`, alpha 0. Row `0x4a` is
`AVAILABLE_COLOR` (40, 90, 220), `0x4b` `UNAVAILABLE_COLOR` (255, 255, 0).

`0x270130`, per lane: below 0.04045 times 0.0773994; else `a = (x + 0.055) * 0.9478673`,
a guess at `a^(-1/5)` from the bits (`0x4c2a6698 - hi32(0x33333333 * bits(a))`), three
steps `y += (1 - a * y^5) * 0.2 * y`, then `(y * a)^3`. Lane 3 is scaled by one.

### Images

`uimg` version 10 (header `0x40`, native `0x48`): f32 reciprocal width and height, u16 width
and height, u16 image count, u16 count of nine-slice images (they come first), pointers to
per-image frames (`0x20` bytes), sizes (u16 × 2), nine-slice borders (`0x20` bytes), and
names (u32 hash, flag bytes), then the texture's GUID at `+0x38`. The CPU data holds each
image's place in the texture (`u, v, w, h`, inset half a texel), which the engine uploads as
a buffer (`0x2f3970`). Registration (`0x2f39c0`) puts each image in one table by its hash: a
later atlas's image replaces an earlier one's. An image's hash is the halves of the asset
hash of its name folded together (`0x2fc420`); a name with no image falls back to
`missing`.

Texture formats in `ui.rpak`: BC7 sRGB (13) and BC1 sRGB (1) for the atlases, 53
(`DXGI_FORMAT_R8_UNORM`, through `0x130a760`) for the two font atlases.

### The UI shader

`code_private/ui` (material in `ui.rpak`): its shader set names an input layout and the
pixel shader `0xa84403b56d2e69e3` (SM5, 535 slots). The reflection survives: `g_styles`
(`UiShaderStyle_s`, 160 bytes: `color[3]`, `colorxfrm[3]`, `tint`, `blend`, `premul`, ...,
`desaturate` at 148), `g_imgBounds0`/`1` (each image's `mins`, `size` in its atlas: the
atlas CPU data), `g_fontBounds`, `g_videoBounds`, `fontTexture`, `materialTexture0`/`1`,
Bink planes. Disassemble it with `D3DDisassemble` from `d3dcompiler_47.dll` on the guest
(a PowerShell `Add-Type` wrapper is enough).

Vertex input: `POSITION` float3, `TEXCOORD1` float4 (the image's frame coordinates in
`xy`, the job's crop coordinates in `zw`), `TEXCOORD2` float2 (text), `TEXCOORD3` int4:
image index, mask image index (−1 none), style index, flags with the draw mode in bits 12
and up (0 text, 1 image, 3 video, else a procedural shape from the style's `+132`
values).

The image mode: four taps at ±¼ of the screen derivatives of the frame coordinates, each
wrapped (flag bits 16, 32: repeat in x, y) or clamped, through `g_imgBounds0[image]`
(`mins + uv * size`), averaged; alpha raised by one when flag 4 is set. A mask image
(texture 1, through `g_imgBounds1`) lerps between `color[1]` and `color[2]`, then adds to or
multiplies the image (flag 1). Edges: the crop coordinates against their own derivatives
give a soft edge (`0.7 × distance + 0.5`), each side switched off by flag bits 8 to 11
(all four set: none). Then for every mode: the colour times `color[0]`; rows of `colorxfrm`
dotted with it plus `(1 - premul × (1 - a))` times their fourth term; times `tint`; lerp to
luminance (0.2126, 0.7152, 0.0722) by `desaturate`; output premultiplied:
`rgb × alpha × (premul × (1 - a) + a)`, `a × blend` (so `blend` 0 is additive).

The style buffer (`0x2fc900`, per style record): `color[0..2]` from the record's `+2` to
`+0x18`; the colour matrix from saturation `+0x28`, brightness `+0x2a` (below zero scales,
above adds in the fourth column) and hue `+0x26` (rotated by `0x2f3d60` when not zero),
times the parent's matrix when the instance is nested (`+0x4a`); `tint` `+0x1a` to `+0x20`;
`blend` `+0x22`; `premul` `+0x24`; for a text style (`+0` = 0) the font's metrics from
`+0x2c` to `+0x3e`; for a shape style the shape's angles and radii from `+0x2c` to `+0x38`;
`desaturate` from ConVar `gfx_desaturate_force` in some draw groups.

A text style with a soft drop shadow draws a faint box around each pair of glyphs. Looked into
on 2026-10-14, seen in `zipline_flyout`'s labels. Style 1 there has shadow alpha `+0x2e` 1,
blur `+0x34` 10 and offset 0, so `t3 = 1 / (k × blur)` = 0.0433, with `k` = 2.31 texels per
pixel. The shadow reaches 0 only at `0.5 / t3 × s` texels from the glyph, 11.5 for a size
class scale `s` of 1. The font's distance field ends at 10 texels (`sample × 12.75 − 10`), so
the shadow bottoms out at `−10 × 0.0433 + 0.5` = 0.067 of black. That floor covers the
glyph's bounds and stops at their edge: over flat sky in `vmshot`, 217 becomes 210.

The game should draw it too. Three things match lurch's line by line: the shader's text and
shadow path, the style terms (`0x2fc900`), and the scales the emitter writes to `TEXCOORD2`
(`0x2f8fc6` to `0x2f9031`: each glyph's `+7` into the face's `+0x48` table). Not yet seen in
the game: the user's screenshot has the labels over a dark doorway.

### Fonts and text

A `font` asset (version 7, header `0x28`, native `0x30`; registered at `0x2f415e`, loaded by
`0x2f3870`) is one R8 texture of signed distances and the faces packed in it:

| Offset | Type | What |
|---|---|---|
| `+0x00` | u16 | face count |
| `+0x02` | u16 | glyph classes per row of the line-break table |
| `+0x04` | 2 × u16 | texture size, then its reciprocal at `+0x08` (2 × f32) |
| `+0x10` | pointer | faces, `0x60` bytes each |
| `+0x18` | pointer | line-break table: a bit per (previous class, class, space between) |
| `+0x20` | u64 | the texture's GUID |

A face: `+0x08` u16 its slot in the font table, `+0x0a` u16 a count of the table at `+0x48`
(not read), `+0x0c` u16 kerning pairs, `+0x0e` u16 blocks of 64 code points, `+0x14` u32
the first code point, `+0x18` u32 glyphs, `+0x1c`/`+0x20` an em in texture units across
and down, `+0x24`/`+0x28` two heights in ems (the layout's baseline is the size times
`+0x24`), `+0x30` u16 per block an index into `+0x40` (u64 a bit per code point present) and
`+0x38` (u16 the glyph of its lowest bit), `+0x50` glyphs (`0x20` bytes: f32 advance in ems,
u16 first kerning pair, u8 class, then six floats the text job reads), `+0x58` kerning
pairs (u32 the code point before, f32 the advance added in ems). A glyph's kerning pairs end
at the next glyph's first, so the glyph array has one more entry than its count.

Loading puts each face's address at its slot of the font table (`0xc05f2d0`, 32 slots) and
its atlas's number at `0xc05f3d0` (5 for none; both set at `0x2f46dd`), and copies the header
to the atlas's place at `0x270f4c30` (`0x30` bytes each). A name's slot (`0x2f3430`) is its
asset hash `h` (`0x46cd80`) folded as `(h ^ h >> 11) & 0x1f` in 16 bits. A glyph
(`0x2f4bc0`): the code point less the first, its block's bit, the glyph of the block's
lowest bit plus the bits below; a code point the face lacks reads as U+25A1, and one that
lacks that too is reported and reads as glyph −1.

A text style's terms in the style buffer (`0x2fc900`): with `k = width × em across / size`
(the atlas's texture width, the face's `+0x1c`, the style's `+0x36`), floats 30 and 31 are
`k × (+0x30) / width` and `k × (+0x32) / height` (the stored reciprocals), 32 is `+0x2e`, 33
`1 / max(FLT_MIN, k × (+0x34))`, 34 and 35 `k × (+0x3a)` and `k × (+0x3c)`, 36
`1 / max(FLT_MIN, (+0x3e) × k)`; the `max` is MAXSS, so a NaN passes through.

A text job (type 0, `0x16` bytes): `+6` to `+9` the style of each of its four fonts, then
data offsets of the string (`+0xa`), the width it is fitted to (`+0xc`), the wrap width
(`+0x10`), the alignment (`+0x12`, 0 left to 1 right) and the line spacing (`+0x14`). A
text style holds data offsets of its font slot (`+0x2c`, u16), size (`+0x36`), advance
scale (`+0x38`) and baseline shift (`+0x40`).

The layout (`0x2f55e0`), per style: the advance scale `size × (+0x38)`, the ascent
`size × face(+0x24) − (+0x40)`, the descent `size − ascent`. It walks the UTF-8 string a
code point at a time (a malformed sequence or a surrogate ends it), counting each:

- `` `n `` (n 0 to 3) switches to style n, raising the line's height to the style's ascent
  plus descent when that is more, and the job's tallest style with it; any other byte after
  the backtick ends the string.
- `%%` is a `%`; `%...%` is substituted (`0x2f4c70`) and walked in place, up to 8 deep: `$`
  an image by name (none: nothing), `<` and `>` U+F2000 and U+F2001, `@` a nested element
  by name, `{a|b}` and `[a|b]` one of two tokens, `&` a token's other form, anything else a
  localized token. A substitution with no closing `%` ends the string.
- A glyph advances by the style's advance scale times its advance, plus the kerning pair
  for the code point before it. A space only marks the next glyph as after a space. A
  newline ends the line. Any other glyph may mark a break, when the first style's atlas's
  table allows one between the previous glyph's class and its own; and when the advance
  passes the wrap width the line ends at the last break, the rest of the line (and its
  images) moving to the next.
- Code points U+F0000 to U+F1FFF are images (index = code point − U+F0000): an image is
  as tall as the style's size, its top at the ascent above the baseline, and advances by
  its width over its height times the advance scale.
  After U+F2000 an image stretches until U+F2001 instead, over every style the text used, a
  nine-slice image adding its borders.
- U+F2002 to U+F2101 are nested elements (registry at `0xd4152c0`, `0x20` each), up to 8.

A line's height adds the spacing; widths are measured to the end of a line's last glyph
that is not a space. At the end every line is shifted by `(widest − its width) ×
alignment`, and the job's scale is `fit / max(fit, widest)` (MAXSS); below one, its images
and nested elements are re-centred vertically. The results go into the draw scratch:

| Offset | What |
|---|---|
| `+0x20 + offset / 16 × 0xc` | per job: f32 scale, u8 first line, line count, first image, image count, tallest style, first nested element, their count |
| `+0x35a4` | 64 line breaks, `0xc` bytes: u32 characters before it, f32 the line's width, f32 the next line's width on the job's last break |
| `+0x38a4`, `+0x38a8`, `+0x3dac` | the counts of breaks, images and nested elements, which `Rui_Draw` zeroes each frame with the string buffer's use (`+0x1598`) |
| `+0x38ac` | 64 images, `0x14` bytes: u16 image, u16 style, f32 `x0, y0, x1, y1` |
| `+0x3db0` | 8 nested elements, `0x1c` bytes: u16 style, u8 registry index, f32 `x, y`, size, and the element's size over it |

The glyph bounds (`g_fontBounds`, one buffer per font asset, built at load by `0x2f3460`):
an entry per glyph of every face, at the face's `+0x2c` (the glyphs of the faces before it).
A face's `+0x48` table (`+0x0a` entries of two floats `s, p`) gives each glyph class (the
glyph's byte `+7`) a scale `S = (em across, em down) × s` (`+0x1c`, `+0x20`) and a margin
`P = S × p`. A glyph whose left and right (`+0x10`, `+0x18`, in ems) match is empty:
mins `(1, 1)`, maxs `(0, 0)`. Otherwise mins `(left · Sx + u) − Px`, `(Sy · top + v) − Py`
and maxs `(Sx · right + u) + Px`, `(bottom · Sy + v) + Py`, with the glyph's place in the
texture `u, v` at `+8`, `+0xc`.

The text job (`0x2f7db0`) draws, in order: the job's inline images (through the image
emitter `0x2f6880`, flags `0x1f00`), the elements nested in it, then the glyphs. Glyphs go
two to a quad: each quad's vertices carry both glyphs' texture coordinates (the shader's
`v0.xy`, `v0.zw`), their classes' scales `s` (`v1.x`, `v1.y`), both glyphs' bound indices
(`v2.x`, `v2.y`), the style and flags 0 (text mode); the shader tests each pixel against both
glyphs' bounds. A quad's sides fall halfway between neighbouring glyphs; its top and bottom
are the glyphs' extents plus the style's margins (the larger of half `+0x34` less the shadow
offset `+0x30`, `+0x32` and half `+0x3e`, plus `+0x3a` and `+0x3c`). Quads are padded for
anti-aliasing (`0x301320`) with a sign per side, starting `(0, 0, −1, 1)`: top and bottom
always, left and right only at a run's ends. Empty glyphs (spaces) only advance. Lines start
at `(box width − line width) × alignment`; the glyph scale is the job's fit scale times the
style's advance scale `+0x38`, the height lerps from the size to the fitted size by the job's
`+0xe`. The style is visible when the largest of its alphas (`+8`, and `+0x10`, `+0x18`
capped by `+0x3a`) is above zero.

sprintf (`0x2fb460`) is the Universal CRT's `vsnprintf` with standard behaviour (`0x2292c0`,
options `| 2`) into the scratch's string buffer at `+0x159c`; when the rest of the buffer is
one byte or less, or the result does not fit, or the format fails, it returns `***`
(`0x133ffd0`) and uses nothing.

### Transform opcodes 8, 9 and 13, and fit

Opcodes 8 (`0x2f2670`) and 9 (`0x2f27d0`) take 0x14-byte entries: parent A, A's point (two
data offsets), parent B, B's point, a point `d` and a vector `c` (data offsets). 8 makes an
upright box from A's point to B's, scaled per axis by `c`; 9 lays a box of the new size along
the line from A's point to B's (in the element's units), turned so its vector
`(c0 · w, c1 · h)` lies on that line. Both put the box's own point `d` on A's point. Opcode
13 (`0x2f30f0`, 6-byte entries: parent, first, end) carries transforms `[first, end)` made in
the unit square into the parent: through its basis over its size, then its origin.

`+0x28` (`0x2f6540`, fit `a, b, c`): the box transforms `[a, b)` cover (MINPS and MAXPS from
±FLT_MAX), transforms `[c, b)` moved by its corner, its size returned as `w, w, h, h`.

### The image job's mask

A type-1 job's mask image (`+8`) is mapped through the job's box (or, with the bool at
`+0x28`, the element's unit square), turned by the angle at `+0x1e` (in turns), scaled by
`+0x24`, `+0x26`, offset by `+0x1a`, `+0x1c` inside the box and `+0x20`, `+0x22` after, then
into the mask's frame (`+0x10` to `+0x1c` of its frame record); with `+0x28` that map is
inverted. Its flags shifted left by 2 join the job's. The shader reads the mask through the
same `zw` coordinates the crop uses otherwise. The sine is the engine's own over quarter
turns: round to the nearest quarter, polynomials in the remainder for sine and cosine, the
quadrant picks one and its sign (constants at `0x15bd220` and `0x15bdd80` on).

### The special image source and the screen blur

Image index `0x2000` is the special source: the scene behind the HUD, blurred
(`special_source` and the image job in `src/rui/draw.rs`, `game/src/blur.rs`).
`tactical_ability_hud`'s backdrop uses it as a mask. 51 elements use it, `screen_blur` and
`tabs_background` among them.

Where it is drawn (`0x2f7c50`): only while the flag `0xd40b2a0` is not zero, and in any draw
group but 1 to 6 (the u16 at the instance's `+0x10`, less one, above 5 unsigned, so group 0
passes too). The topology must be a plane lying flat in the screen: type below 2, its vectors
at `+0x10` (origin), `+0x20` (right) and `+0x30` (down), with origin z, right y and z, and
down x and z all zero (the fourth lanes too). Then it maps the job's box to the texture's
coordinates through the backbuffer's size `W`, `H` (the material system's `+0x1c8`):

- basis: `sizes[0].x · (right.x · inv_size.x) / W` across and `sizes[0].w · (down.y ·
  inv_size.y) / H` down, zero elsewhere. `sizes[0]` is the frame's first transform size
  (scratch `+0x3e90`); `inv_size` is the element's (`+0x20`).
- origin: `org.x / W`, `org.y / H`.

The flag is 1.0 after the screen blur ran this frame (`0x5cdf20`, store at `0x5ce03c`) while
ConVar `mat_screen_blur_enabled` (image default 1) is on; otherwise it is 0 (`0x5cdfd4`).

As the image (`0x2f96f0`): the job's crop is the box's unit square, no placement or repeat
(placement 0, span 1), the clip is `(0, 0, 1, 1)` of the box, the job's flags `| 4` (the
image's alpha forced to 1), no nine-slice. The rows above replace the frame's map.

As the mask (`0x2f990f`, `0x2f99c1`): the mask is image 0 of the special atlas, the job's
flags `| 8` (the mask's alpha forced to 1), and the crop rows are the map above. Either way a
false answer from `0x2f7c50` leaves the job undrawn.

The emitter knows the source by its entry (`0x270f4d20`) and draws it from a separate atlas
record (`0x270f58e0`, `0x2f68cf`). That atlas's one image is `_rt_SmallFB0`, which the UI
fetches by name each frame (`0x3ee3a0`). Its bounds come from the render target's size
(`0x2f4a76`): mins `1 / w`, `1 / h`, size `1 − 1 / w`, `1 − 1 / h`.

The screen blur (`0x756a00`), from the frame `W` by `H`, with `w = W / 4` and `h = H / 4`
(integer division):

1. `dev/downsample_backbuffer` draws `_rt_FullFrameFB` into `_rt_SmallFB0`, `w` by `h`. Its
   source rectangle is the whole frame.
2. `dev/blurfilter` draws `_rt_SmallFB0` into `_rt_SmallFB1` with `$blurDirection` 0 (across).
3. `dev/blurfilter` draws `_rt_SmallFB1` back into `_rt_SmallFB0` with `$blurDirection` 1
   (down).

The blur's `$minUVs` is `1 / (2w)`, `1 / (2h)` and its `$uvRangeWidth` is `1 − 1 / w` (through
a double), `1 − 1 / h`.

The materials are VMTs in `englishclient_mp_common.bsp.pak000_dir.vpk`: FindMaterial
(`0x3bf710`, hooked by the SDK at its entry) tries `material\NAME.rpak` and then
`materials/NAME.vmt`.

- `dev/downsample_backbuffer`: shader `Downsample`, `$useBackbuffer 1`, `$BASETEXTURE
  _rt_FullFrameFB`.
- `dev/blurfilter`: shader `BlurFilter`, `$kernel 4`, commented "Gaussian kernel 25 pixels
  wide, 13 texture fetches", `$basetexture _rt_SmallFB0`.

Their shaders load from `shaders\fxc\NAME.vcs` (`0x1362168`) in
`client_frontend.bsp.pak000_000.vpk`. Each `.vcs` holds one DXBC blob at `+0x3c`.

- `downsample_ps40`: four samples at texel offsets `(−1, −1)`, `(−1, 1)`, `(1, −1)`,
  `(1, 1)`, added in that order, times 0.25. Alpha is the luminance `(0.3, 0.59, 0.11)`.
- `blurfilter_ps40` (`CBufBlurFilter`: `c_offsetToNeighborPixel`, `c_numSideTapPacks`,
  `c_centerWeight`, `c_colorBias`, `c_colorScale`, `c_sideTapPacks[9]`;
  `CBufBlurFilterDynamic`: `c_minUVs`, `c_uvRangeWidthDouble`):
  - the centre sample times the centre weight;
  - then, per pack `(o1, w1, o2, w2)`, the taps at `uv ∓ o · step`, each mirrored into the
    range: `q = p − min`, then `min(|q|, 2·range − q) + min`;
  - the two first-offset taps are summed and times `w1`, then the two second-offset taps
    times `w2`, each added to the running sum;
  - out: `rgb · scale + bias`, alpha as summed.
- Both vertex shaders pass the texture coordinate through.

`BlurFilter`'s draw (`0x41c9b0`, `0x41cb70`) sets the constants:

- step: `(1 / source width, 0)` across, `(0, 1 / source height)` down;
- the kernel's pack count, centre weight and packs;
- bias 0 and scale 1 (`$enableClearColor` off);
- the dynamic range doubled.

Its parameters (`0xd4035d0`) are `$smallRT`, `$kernel`, `$blurDirection`,
`$enableClearColor`, `$clearColor`, `$minUVs` and `$uvRangeWidth`.

The kernels are built once (`0x41c720`):

- Kernel 0 is a fixed table (`0x1362f88`): centre 0.2013, six taps a side.
- Kernels 1 to 7 come from the widths at `0x1362fc0` (7, 9, 13, 25, 7, 25, 33). The width is
  scaled by `height / 1080` (rounded) where `0x1363048` is 0, which applies to kernels 5 to 7.
  It is capped at 18 (`0x41c8da`).

So `$kernel 4` is 18 texels wide, not the VMT's 25.

The generator (`0x459e60`) for a width `n`:

- weights `exp(−(i·k)² / 2)` for `i` from 0 to `n / 2`, with `k = 4 / ((n + 1) · 0.5)`;
- the weights scaled by `1 / (w0 + Σ 2·wi)`;
- then taps: the centre `(0, w0)`, and each pair `i`, `i + 1` from 1 on as one tap at
  `i + w(i+1) / (wi + w(i+1))` weighing `wi + w(i+1)`; a last odd weight is a tap of its own
  at `i`;
- at most 19 taps.

The table keeps `taps / 2` packs, the side taps two to a pack. An odd last tap's pack is
finished by a tap of weight 0 at the same offset. Width 18 gives the centre and five taps a
side (11 fetches a pass). Width 25 gives 13 fetches, which matches the VMT's comment.

Not checked: whether `_rt_FullFrameFB` and `_rt_SmallFB0` are sRGB, the screen rectangle
draw's exact texel mapping (lurch maps each target pixel's centre straight onto the source),
the samplers (lurch: bilinear, clamped). The shell runs the blur on the camera that draws the
HUD, between its opaque and transparent passes, so it holds the scene and the arms but not
the HUD.

### The use prompt (engine)

`0x681aa0` runs each frame for the local player: unless use prompts are off
(`disable_player_use_prompts`), the player is in a context action that hides them, or in a
vehicle, it runs the use search (`0x85e8c0` with the client flag) and passes what it finds to
`0x99ea50`, which fills the prompt state at `0x23356e0` through `0x99e470`. A target whose
virtuals `+0x4f0` or `+0x4e0` say it is a zipline makes state 5, with the use point the
search left at the player's `+0x19bc` (written at `0x85e5f2`, the point `find_zipline`
returns). Other targets show their hold or press prompt (`m_holdUsePrompt`,
`m_pressUsePrompt`) through `ui/hint_display`.

`0x99eab0` draws it each frame: an instance of `ui/zipline_flyout` (state 5) or
`ui/hint_display` made for the frame (`0xd410a00`) in draw group 9, on a plane in screen
pixels, with `usePos` (the use point), `hintTextSize` when set, and `timeSincePlayerUsed`.
`zipline_flyout` projects `usePos` through `+0x78` with the draw group's matrix (the
globals' first `0x40` bytes) and formats `#ZIPLINE_INTERACT` (`%&use%`), `#RIDE` and
`#PING_ABILITY` through `+0x48`, with images `white` and
`rui/borders/zipline_flyout_line_border`.

So the prompt is two things: a zipline-only element, and the generic `hint_display` for
every other usable. Only the first exists in lurch (the test map has nothing else usable).
More of it, recovered 2026-10-13:

- `0x681aa0` also skips the search while `+0xe6`'s byte is set or `+0x3a93` is clear, or
  `0x66ce30` says so, or the player's `+0x452` is 12; then it clears the state. A target
  goes through only when its `+0x9e8` mask meets the player's.
- `0x99e470` for a target other than the player: `hintTextSize` from the target's `+0x8d4`
  (taken to be `m_usePromptSize`: the server's datamap puts it beside `m_usableFOV`, as the
  client's `+0x8d0` is), `timeSincePlayerUsed` from now less the target's `+0x9d8` (not
  identified). Both start at −1; `hintTextSize` is set only when not below 0. For a zipline
  it then sets state 5 and the use point (`+0x19bc` to `+0x19c4`) and stops.
- `0x99eab0` builds the instance's topology itself: a plane (type 0) from `(0, 0, 0)` along
  `(w, 0, 0)` and `(0, h, 0)`, the screen's size from `0xd40b3b0`'s `+0x1a8`. It hands the
  draw list draw group 6's globals (`0x24708d38`).
- `zipline_flyout` (`ui.dll+0x12b040`, 1920 × 1080, data `0x74` bytes) hides itself while the
  globals' `+0xb0` is set (a menu is open: `0x71fa40` sets it from the open menu count). It
  writes the projected `usePos` (`+0x40`) at `+0x4c`, the three texts at `+0x58`, `+0x60`,
  `+0x68`, images `white` at `+0x54` and the line border at `+0x70`. Its jobs: four texts
  (`#ZIPLINE_INTERACT` over `#RIDE`, `%ping%` over `#PING_ABILITY`), a shape (job type 2,
  the dot, 8 × 8) and the line from the use point to the dot (an image job on the nine-slice
  `zipline_flyout_line_border`, its box made by opcode 10).

### The HUD's view matrix

The HUD draw groups (6 and 7) get fresh globals every frame (`0x768a45`): the template, the
time, then rows of a matrix built from the view's world-to-view rows `V0`, `V1`, `V2` (view
setup `+0x40`, `+0x50`, `+0x60`) and the screen's `w`, `h` (`+0x238`, `+0x23c`):
`V0 + (0, 0, 0, w/2)`, `(0, 0, 0, h/2) − V1`, `(0, 0, 0, d) − V2`, with
`d = −w/2 / max(T, 0.0001)` and `T` the view setup's `+0x190`; then `(w/2, h/2, d)` at
`+0x30` (the eye), something from `0x436b80` at `+0x3c`, the viewport at `+0x48`, and the
screen-to-clip matrix at `+0x50` (`0x4364e0`). A point through these rows and the line from
the eye to it meet the screen plane at `w/2 + d·x/z`, `h/2 − d·y/z` in view space, so `T` is
the tangent of half the horizontal field of view; this is read from the geometry, not from
the code that writes `+0x190`.

### Projecting a point (`+0x78`)

`0x2fbcf0`: the point (its fourth lane forced to 1) through the matrix's first three rows,
each `(p.z·m2 + m3) + (p.x·m0 + p.y·m1)`, then the topology's function (table `0x1345860` by
type: 0 and 1 a plane `0x2fffd0`, 2 `0x300220`, 3 `0x3004b0`) with the eye `m[12..15]`.
The plane's: with `N = right × down`, the line from the eye through the point crosses the
plane past the eye when `N·(org − eye)` (signed as `N·dir`) and `|N·dir|` are both above 0;
then the crossing in the plane's units, `−(C·down)` and `C·right` with
`C = (x × N) / |N|²`, scaled by the least of 1 and `2^12 / 2^e` per lane (an integer
subtraction on the exponent bits, `0x85000000 − (bits & 0x7f800000)`). Otherwise the same
coordinates of the point itself from the plane's centre, made 4096 long (`RSQRTPS`, no
refinement), or `(0, 4096)` when both coordinates' squares are at most `FLT_EPSILON`.

### Transform opcode 10

`0x2f2a00`, operands as opcode 9's: opcode 9's box along the line between the two points,
but the half of its basis turned across the line is scaled by `|q| / |v|` (a refined
`RSQRTPS` of `|v|² / |q|²`), so the box keeps its own height whatever the line's length.

### Key names in text (`%command%`)

A `%...%` the layout does not know as an image, nested element or span is looked up in the
draw list's token table (the draw context's `+8`, `0x2f4c70`): `%name%` gives the entry's
text, `%&name%` its other form; `{a|b}` and `[a|b]` pick one of two names by the table's
`+0`. Nothing found gives the table's default, `%@ICON_UNBOUND%`. The table is the input
mode's (0 keyboard and mouse, 1 and 2 pads; `0x6fd0b0`), handed to RUI by `0x71fa40` each
frame (`0xd410a78`) and rebuilt by `0x23f8b0` when the mode changes or a bind does:

- An entry per key name (`0x23f6a0`: codes `1..0x72`, `0x73..0x92`, `0xf3..0xf6`,
  `0x103..0x10e`, `0x133..0x134`), its text the key's display string.
- For each key of the mode (keyboard: codes 1 to `0x72`), for each of its two binds (press,
  hold), each `;`-separated command with its `+` dropped, 1 to 24 long: an entry named
  after the command, its text the key's display string (`0x23f550`) and its other form
  `INPUT_PRESS` (`%command%`) or `INPUT_HOLD` (`HOLD %command%`) localized, its `%...%`
  replaced by that text (`0x23f330`); a hold bind is flagged.
- A key's display string (`0x23f550`) is entry `code` of `scripts/keys_keyboard.rson` (an
  `rson` asset in `common_early.rpak`: type `0x1002`, a list of `0x73` strings), wrapped as
  `%<%%$rui/borders/key_border%` … `%>%` unless it holds `%@` (or, for the mouse's codes
  `0x6c` on, `%$`). The list gives `%@KEY_E%` for E (code `0xf`), `%$vgui/HUD/pc_mouse_mb%`
  for the middle button (`0x6e`).

R5Reloaded's default config binds `e` to `+use; +use_long`, so `%&use%` reads `%@KEY_E%`.

### Embedded elements (`%@name%`)

`%@name%` is an element drawn inside the text, from a registry (`0xd4152c0`, `0x20` each:
the asset, its name, its arguments, its size; found by name through `0x722970`). The UI
script fills it: `ui/_embedded_text.nut` calls `RuiCreateEmbedded(asset, name)` (native
`0x722a20`: the size is the element's own) and sets `displayText` for each key:
`KEY_E` is `ui/key_square_pc` with `E` (48 × 48), wider keys `key_rect_pc` (72 × 48) and
`key_wide_pc` (96 × 48), `ICON_UNBOUND` `key_square_icon_pc`.

The layout (`0x2f55e0`) turns code point U+F2002 + i into a record while it holds fewer than
8 (`+0x3db0`, `0x1c` bytes): u16 style, u8 registry index, f32 pen `x` and baseline `y`,
the box `(s/h · w, s/h · h)` for the style's size `s` and the entry's size `w`, `h` (a zero
read as 1), and the element's size over the entry's. The pen moves by the box's width. On a
wrap, a line's alignment and a fit below one, records move as images do (a fit centres them
by their height).

The text job (`0x2f7db0`), after its images, draws each record: a new instance of the
entry's asset (`+0x4a` set), the entry's arguments copied in, its `parentColor` (colour
and alpha) set from the record's style's colour, then `Rui_Draw` (`0x2fc670`) and the job
walk (`0x2fcff0`) into the same draw list, with the parent's colour matrix
(`0x28f2b0`) and a placement from the record's box through the job's transform.

### The tactical's panel (script)

`ClWeaponStatus_AddClient` creates it with `CreateCockpitPostFXRui` (draw group 7),
`HUD_Z_BASE`, tracking `bleedoutEndTime` and `reviveEndTime`. `InitOffhandRui` sets
`hintTime` (now), `isTitan` false, `isVisible` (`GetHudDefaultVisibility()`), `isReverseCharge`
false, `isPaused` (mod `survival_ammo_regen_paused`), `chargeFrac` 0, `useFrac` 0,
`chargeMaxFrac` 1, `minFireFrac` 1, `segments` 1, tracks `refillRate` (ammo regen rate),
`hudIcon` (the weapon's `hud_icon`, `rui/hud/tactical_icons/tactical_pathfinder`),
`readyFrac` (ready to fire), `dryfireFrac`; `chargeFracCaution` 0, `chargeFracAlert` 0,
`chargeFracAlertSpeed` 16, `chargeFracAlertScale` 1, `ammoMinToFire`, and `ultimateColor`,
`ultimateColorHighlight` from the legend's `hudUltimateColor` and `hudUltimateColorHighlight`
(its item flavour's settings), sRGB to linear, alpha 1. For `cooldown_type ammo`:
`minFireFrac` = min to fire / clip, `segments` = clip / per shot, `chargeFrac` tracks the clip
fraction and `useFrac` the `simple_timer` status effect. `hintText` is `%offhand1%` while
`hud_setting_showButtonHints` is on, and is refreshed every frame.

### Localized text

`+0x48` (`0x2fb6e0`, through `0x2fb4e0`): an empty token comes back as it is. Otherwise the
token is looked up in the localization table (`CLocalize`, vtable `0x1357e60`, its object
at `0xd40b388`; `+0x58` finds a text, `0x3a57c0` its index): a leading `#` is dropped and the
names compare ASCII letters without case (`0x466d30`). A token with no text is copied as it
is (`%s` through `0x2f33e0`) when it fits in the rest of the frame's string buffer less one.
A text is UTF-16: each unit from `0xd800` to `0xdfff` takes the next one with it unchecked,
`%s1` to `%s9` becomes the argument it names when that is one of the five passed (pointers in
`r8`, `r9` and three on the stack), and every other code point is written as UTF-8 only with
room for four bytes and the terminator. Whatever does not fit gives `***`. The text then uses
the buffer as sprintf's does.

The table is filled from the files `platform/resource/localization_files.txt` lists, with
`_english.txt` appended (UTF-16 with a byte order mark; `"lang" { "Tokens" { "NAME" "text" } }`,
an entry optionally followed by a condition such as `[$PC]` or `[!$NX&&!$PS4]`). The engine's
loader is not read: which of two entries of one name it keeps, its escapes and its conditions
are taken from the file format (`src/assets/localize.rs` says which).

### `tactical_ability_hud` (`ui.dll+0x93e80`)

Hidden unless the globals' `+0xa8` is set and `+0xac`, `+0xb0` are not, `isVisible` is set,
an unnamed bool at `+0x98` is not, `bleedoutEndTime` (`+0xb4`) is not above 0, and the time
is not before `reviveEndTime` (`+0xb8`). `chargeMaxFrac` 0 is reported as a division by zero
(`weapon_status.rui` line 1548) and hides it.

The charge is `chargeFrac` (`1 - chargeFrac` with `isReverseCharge`) over `chargeMaxFrac`,
capped at 1. While `chargeFrac` is above 0 and below `minFireFrac` and `readyFrac` is 0, with
a `refillRate` above 0, the hint below the box is `#HUD_ABILITY_RECHARGE_REMAINING_SEC` with
`%i` of `int((1 - chargeFrac / minFireFrac) × ammoMinToFire / refillRate)` ("7 sec");
otherwise it is empty. A `refillRate` of 0 also stops the fill's animation. The images: `hudIcon`,
`rui/hud/dpad/slanted_mask`, `slanted_border`, `white`, `rui/hud/ultimate_icons/ultimate_fill`,
`rui/hud/cooldown_gradient`, `rui/hud/status_effects/silenced`,
`rui/hud/tactical_icons/slanted_segments_mask`, `tactical_cooldown_segment`,
`tactical_charges_boosted_4ct`, `tactical_charges_boost_glow_mask`, `flare_tiny_yellow`.
`segments` above 1 draws segment marks. Ready, the box is black at 0.75 alpha and the icon
white; charging, the icon is at 0.15 and a white fill at 0.25 rises with the charge.

The argument offsets: `hintTime` `+0x7c`, `isTitan` `+0x84`, `isVisible` `+0x88`,
`isReverseCharge` `+0x8c`, `isPaused` `+0x94`, `chargeFracAlert` `+0xa0`, `…AlertSpeed` `+0xa4`,
`…AlertScale` `+0xa8`, `chargeFracCaution` `+0xac`, `hudIcon` `+0xc0`, `chargeFrac` `+0xc8`,
`useFrac` `+0xcc`, `ultimateColor` `+0xd0`, `ultimateColorHighlight` `+0xe0`, `chargeMaxFrac`
`+0xf0`, `minFireFrac` `+0xf4`, `segments` `+0xf8`, `readyFrac` `+0xfc`, `dryfireFrac` `+0x100`,
`refillRate` `+0x104`, `hintText` `+0x108`, `ammoMinToFire` `+0x110`.

### The grapple's clip

`mp_ability_grapple.txt`: `cooldown_type ammo`, `ammo_clip_size`, `ammo_min_to_fire` and
`ammo_per_shot` 300, `regen_ammo_refill_rate` 10, `regen_ammo_refill_start_delay` 0,
`fire_duration` 0.001; the mod `grapple_regen_stop` sets the refill rate to 0. Its attack
(`OnWeaponPrimaryAttack_ability_grapple`) uses no ammo; the engine fires a weapon only with
`ammo_min_to_fire` in the clip.

The script (`mp_ability_grapple.nut`): `CodeCallback_OnGrappleAttach` starts
`GrappleDecreaseAmmo`, which keeps the clip it found, empties it, adds `grapple_regen_stop`
and resets the regen. Then, at once and every 0.1 s while the grapple is active or the
player is faster than 500: the clip is emptied again and the step's distance in the plane
added to a path; it stops when the player newly rides a zipline or after 5 s. At its end it
adds the last step and runs `ReduceAmmoBasedOnDistance`: the cost is
`int(GraphCapped(path, 300, 4500, 0, 1) × 300) + 100` (playlist vars
`pathfinder_grapple_drain_min_dist`, `…_max_dist`, `…_base_cost`), the clip becomes the one it
found less the cost, from 0 to `ammo_min_to_fire`, the mod comes off and the regen is reset.
It also works out a distance with the height (`…_z_scalar` 0.5) and does not use it.

The engine's regen (a weapon's think, `0xa3f350`, calls `0xa5a520`; fields of the weapon,
its settings copied at `+0x1874`: clip `+0x1334`, `ammo_clip_size` `+0x1b58`, refill rate
`+0x1ba0`, start delay `+0x1ba4`, `regen_ammo_stockpile_max_fraction` `+0x1ba8`,
`regen_ammo_while_firing` `+0x1bb0`, `fire_duration` `+0x189c`, `cooldown_time` `+0x1e64`):

| Step | Where | What |
|---|---|---|
| Reset | `0xa5a680` | with a refill rate above 0, the regen timer `+0x1658` becomes −2 |
| Marked | `0xa5a520` | timer at −2: the last fire time `+0x1304` becomes now less `fire_duration`, the timer −1 |
| May regen | `0xa59ac0` | refill rate above 0, a last fire time, the clip short of its size, and (unless `regen_ammo_while_firing`) now past last fire + `fire_duration` + start delay + `cooldown_time`; and states lurch never has |
| Not | `0xa3f46e` | when it may not, the timer drops to −1 |
| Start | `0xa59d45` | when `+0x2aac` is not above 0, the timer and `+0x2aac` become now less the frame time |
| Refill | `0xa59dee` | `x = (now − timer) × rate`; at 1 or more the clip gains `int(x)`, up to its size, and the timer becomes `now − frac(x) / rate` |

A weapon with a stockpile fraction regenerates its stockpile instead (`0xa5a210`), starting
its timer whenever it is not above 0, start delay added. `+0x2aac` is written only at the
start step, so read as it is a later run would start from a timer of −1 and refill at once;
`cooldown.rs` starts a run whenever its timer is not above 0, as the stockpile regen does.
That is an inference, to be checked by timing a refill in the game.

### `crosshair_grapple` (`ui.dll+0x135a30`)

Out of range: `UNAVAILABLE_COLOR` at alpha 0.15. In range: `AVAILABLE_COLOR`, and once
past the time since it last went out of range, mappings 0, 1 and 2 over the time since it
came in range scale the 24 × 24 size and give the alpha and `+0x50`. Hidden when `ammoFrac`
is below 1 or the weapon is disabled. The alpha is multiplied by `1 - (+0x14)`. The image is
`rui/hud/crosshairs/grapple_indicator`, centred at `(0.5, 0.5)` plus the crosshair
movement.

### The pause and settings menus

What R5Reloaded's scripts and layouts build, read from the install
(`platform/scripts/resource/ui/menus/`, `platform/scripts/vscripts/ui/`), with each ConVar's
image value from `tools/convars.py` (default string, flags, range when it has one). A default
marked mod or `gamesdk.dll` is not in the image: R5Reloaded registers it ("The ConVar files"). Labels are localization tokens (`#…`); a row's label and description come from the
script's `SetupSettingsButton` or `SetupSettingsSlider`, its options and ConVar from the
layout. Rows are listed in the layout's order, which is the order on screen (each pinned
under the one before).

**Escape.** In a match, Escape reaches `UICodeCallback_ToggleInGameMenu` and opens
`SystemMenu` (`OpenSystemMenu`). `UpdateSystemPanel` fills the buttons in order: `#SETTINGS`
(opens `MiscMenu`), then the mode's extras (none in a plain match), then `#LEAVE_MATCH`
(`LeaveDialog`); `#RETURN_TO_LOBBY` instead in the firing range or training. The text at the
bottom is empty unless the playlist is one of R5Reloaded's named modes or `is_practice_map`.

**Tabs** (`InitSettingsPanel`), in order, with the details panel's default image:
`#HUD` (`settings_hud`), `#MOUSE_KEYBOARD` (PC only, `settings_pc`), `#CONTROLLER`
(`settings_gamepad`), `#VIDEO` (`settings_video`), `#AUDIO` (`settings_audio`); images under
`rui/menu/settings/`. Footer: `#B_BUTTON_CLOSE` / `#CLOSE`.

Gameplay (`#HUD`). Rows from `SwchAccessibility` on, and the accessibility header, show only
when `IsAccessibilityAvailable()` (native, not yet read); `SwitchChatMessages` is PC only.

| Row | Label | ConVar | Options (label value) | Default | Flags |
|---|---|---|---|---|---|
| `SwitchLootPromptStyle` | `#HUD_SETTING_LOOTPROMPTYSTYLE` | `hud_setting_showMedals` | `COMPACT` 0, `DEFAULT` 1 | 1 | `0x1000000` |
| `SwitchShotButtonHints` | `#HUD_SHOW_BUTTON_HINTS` | `hud_setting_showButtonHints` | `OFF` 0, `ON` 1 | 1 | `0x1000000` |
| `SwitchDamageIndicatorStyle` | `#HUD_SETTING_HITINDICATORSTYLE` | `hud_setting_damageIndicatorStyle` | `OFF` 0, `CROSSHAIR` 1, `CROSSHAIR_SHEILDS` 2 | 2 | `0x1000000` |
| `SwitchDamageTextStyle` | `#HUD_SETTING_DAMAGETEXTSTYLE` | `hud_setting_damageTextStyle` | `OFF` 0, `STACKING` 1, `FLOATING` 2, `BOTH` 3 | 1 | `0x1000000` |
| `SwitchPingOpacity` | `#HUD_SETTING_PINGOPACITY` | `hud_setting_pingAlpha` | `DEFAULT` 1.0, `FADED` 0.5 | 1.0 | `0x1000000` |
| `SwitchShowObituary` | `#HUD_SHOW_OBITUARY` | `hud_setting_showObituary` | `OFF` 0, `ON` 1 | 1 | `0x1000000` |
| `SwitchRotateMinimap` | `#HUD_ROTATE_MINIMAP` | `hud_setting_minimapRotate` | `OFF` 0, `ON` 1 | 0 | `0x1000000` |
| `SwitchWeaponAutoCycle` | `#SETTING_WEAPON_AUTOCYCLE` | `weapon_setting_autocycle_on_empty` | `OFF` 0, `ON` 1 | 1 | `0x41000200` |
| `SwitchAutoSprint` | `#SETTING_AUTOSPRINT` | `player_setting_autosprint` | `OFF` 0, `ON` 1 | 0 | `0x41000000` |
| `SwitchPilotDamageIndicators` | `#HUD_PILOT_DAMAGE_INDICATOR_STYLE` | `damage_indicator_style_pilot` | `INDICATOR_2D_ONLY` 0, `INDICATOR_3D_ONLY` 2, `INDICATOR_BOTH` 1 | 2 | `0x1000000` |
| `SwitchDamageClosesMenu` | `#HUD_DAMAGE_CLOSES_MENU` | `player_setting_damage_closes_deathbox_menu` | `OFF` 0, `ON` 1 | 1 | `0x1000000` |
| `SwitchShowHealthbars` | `#HUD_SHOW_HEALTHBARS` | `enable_healthbar` | `OFF` 0, `ON` 1 | 1 (mod, 0 to 1) | `0x80080` |
| `SwitchStreamerMode` | `#HUD_STREAMER_MODE` | `hud_setting_streamerMode` | `OFF` 0, `KILLER` 1, `ALL` 2 | 0 | `0x1000000` |
| `SwitchAnalytics` | `#HUD_PIN_OPT_IN` | `pin_opt_in` | `DISABLED` 0, `ENABLED` 1 | 1 | `0x1000000` |
| subheader | `#MENU_ACCESSIBILITY` | | | | |
| `SwchColorBlindMode` | `#COLORBLIND_MODE` | `colorblind_mode` | `OFF` 0, `PROTANOPIA` 1, `DEUTERANOPIA` 2, `TRITANOPIA` 3 | 0 (0 to 3) | `0x41000000` |
| `SwchSubtitles` | `#SUBTITLES` | `closecaption` | `OFF` 0, `ON` 1 | 0 | `0x1000000` |
| `SwchSubtitlesSize` | `#SUBTITLE_SIZE` | `cc_text_size` | `SUBTITLES_NORMAL` 0, `_LARGE` 1, `_HUGE` 2 | 0 | `0x1000000` |
| `SwchAccessibility` | `#MENU_CHAT_ACCESSIBILITY` | `hud_setting_accessibleChat` | `OFF` 0, `VISUAL` 1, `AUDIO` 2, `VISUAL_AUDIO` 3 | 0 | `0x1000000` |
| `SwchChatSpeechToText` | `#MENU_CHAT_SPEECH_TO_TEXT` | `speechtotext_enabled` | `OFF` 0, `ON` 1 | 0 | `0x1000000` |
| `SwitchChatMessages` | `#MENU_CHAT_TEXT_TO_SPEECH` | `hudchat_play_text_to_speech` | `OFF` 0, `ON` 1 | 0 | `0x1000000` |
| `SwitchEnableMotd` | `#HUD_ENABLE_MOTD` | `enable_motd` | `OFF` 0, `ON` 1 | 1 (mod, 0 to 1) | `0x80080` |
| `SwitchShowMotd` | `#HUD_SHOW_MOTD` | `open_motd_once_per_server` | `OFF` 0, `ON` 1 | 1 (mod, 0 to 1) | `0x80080` |
| `SwitchShowPos` | `#HUD_SHOW_POS` | `cl_showpos_archived` | `OFF` 0, `ON` 1 | 0 (mod, 0 to 1) | `0x80080` |
| `SwitchShowFPS` | `#HUD_SHOW_FPS` | `cl_showfps_archived` | `OFF` 0, `ON` 1 | 0 (mod, 0 to 1) | `0x80080` |
| `SwitchShowDevWarning` | `#HUD_SHOW_DEV_WARNING` | `show_dev_warning_dialogue` | `OFF` 0, `ON` 1 | 1 (mod, 0 to 1) | `0x80080` |
| `SwitchMovementOverlay` | `#HUD_SHOW_MOVE_OVERLAY` | `enable_movement_overlay` (also a command) | `OFF` 0, `ON` 1 | 0 (mod, 0 to 1) | `0x80080` |

Option labels are `#SETTING_<name>` unless written with their own `#`. The list's order is
the order the arrows step through (`INDICATOR_3D_ONLY` 2 before `INDICATOR_BOTH` 1).

Mouse/Keyboard. Sliders have a text entry beside them (`SliderControlTextEntrySmall`).

| Row | Label | ConVar | Options | Default | Flags |
|---|---|---|---|---|---|
| `SldMouseSensitivity` | `#MOUSE_SENSITIVITY` | `mouse_sensitivity` | slider 0.2 to 20, step 0.2 | 5 (0.1 to 20) | `0x80` |
| `SldMouseSensitivityZoomed` | hidden (`visible 0`) | `mouse_zoomed_sensitivity_scalar_0` | slider 0.2 to 10, step 0.2 | 1.0 (0.1 to 20) | `0x80` |
| `BtnLookSensitivityMenu` | `#MENU_MOUSE_SENSITIVITY_ZOOM` | opens `ControlsAdvancedLookMenuPC` | | | |
| `SwchMouseAcceleration` | `#MOUSE_ACCELERATION` | `m_acceleration` | `OFF` 0, `ON` 1 | 0 | `0x80` |
| `SwchMouseInvertY` | `#MOUSE_INVERT` | `m_invert_pitch` | `OFF` 0, `ON` 1 | 0 | `0x1000000` |
| `SwchLightingEffects` | `#LIGHTING_EFFECTS` | `chroma_enable` | `OFF` 0, `ON` 1 | 1 | `0x80` |
| `listpanel_keybindlist` | the key bind list (engine) | binds | | | |

The ADS menu (`ads_controls_pc.res`, title `#CONTROLS_ADVANCED_LOOK`): `SldMouseSensitivity`
again, `SldMouseSensitivityZoomed` (`#MOUSE_SENSITIVITY_ZOOM`,
`mouse_zoomed_sensitivity_scalar_0`), subheader `#MENU_PEROPTICADS`, `SwchCustomADSEnabled`
(`#PEROPTICADS_ENABLED`, `mouse_use_per_scope_sensitivity_scalars`, default 0, `0x80`), and
`SldSensitivity0` to `SldSensitivity7` (`#PEROPTICADS_0` to `_6` labelled in the script,
`mouse_zoomed_sensitivity_scalar_0` to `_7`, sliders 0.2 to 10 step 0.2, each default 1.0, 0.1
to 20, `0x80`). The per-optic sliders show only with the switch on.

Controller.

| Row | Label | ConVar | Options | Default | Flags |
|---|---|---|---|---|---|
| `BtnGamepadLayout` | `#GAMEPAD_SET_BINDS` | opens `GamepadLayoutMenu` | | | |
| `SwchStickLayout` | `#STICK_LAYOUT` | `gamepad_stick_layout` | `SETTING_DEFAULT` 0, `SOUTHPAW` 1, `LEGACY` 2, `LEGACY_SOUTHPAW` 3 | 0 | `0x1000000` |
| `SwchTapToUse` | `#TAPTOUSE` | `gamepad_use_type` | `HOLDTOUSE_TAPTORELOAD` 0, `TAPTOUSE_HOLDTORELOAD` 1, `TAPTOUSE_TAPTORELOAD` 2 | 2 | `0x1000000` |
| `SwchHoldToCrouch` | `#HOLDTOCROUCH` | `gamepad_togglecrouch_hold` | `HOLDTOCROUCH_OFF` 0, `_ON` 1 | 0 | `0x1000000` |
| `SwchToggleGamepadADS` | `#GAMEPAD_TOGGLE_ADS` | `gamepad_toggle_ads` | `GAMEPAD_TOGGLE_ADS_OFF` 0, `_ON` 1 | 0 | `0x1000000` |
| `SwchTriggerDeadzone` | `#GAMEPAD_TRIGGER_DEADZONES` | `gamepad_trigger_threshold` | `TRIGGERDEADZONE_NONE` 0, `_DEFAULT` 30, `_MODERATE` 64, `_HIGH` 128, `_MAX` 255 | 30 | `0x1000000` |
| `SldCursorVelocity` | `#GAMEPAD_CURSOR_VELOCITY` | `gameCursor_velocity` (the image's `gameCursor_Velocity`) | slider 1300 to 4300, step 100 | 1300.0 | `0x1000000` |
| subheader | `#MENU_LOOKMOVE` | | | | |
| `SwchLookSensitivity` | `#LOOK_SENSITIVITY` | `gamepad_aim_speed` | `SENSITIVITY_VERYLOW` 0 … `_INSANE` 7 (eight) | 2 (0 to 7) | `0x1000000` |
| `BtnLookSensitivityMenu` | `#MENU_SENSITIVITY` | opens the ADS look menu | | | |
| `SwchLookSensitivityADS` | hidden (`visible 0`) | `gamepad_aim_speed_ads_0` | `SENSITIVITY_SAME` −1, then as above | −1 (−1 to 7) | `0x1000000` |
| `SwchLookAiming` | `#LOOKSTICK_AIMING` | `gamepad_look_curve` | `LOOKSTICK_AIMING_0` 0 … `_4` 4 | 0 (0 to 4) | `0x1000000` |
| `SwchLookDeadzone` | `#LOOK_DRIFT_GUARD` | `gamepad_deadzone_index_look` | `NONE` 0, `SMALL` 1, `LARGE` 2 | 1 | `0x1000000` |
| `SwchMoveDeadzone` | `#MOVE_DRIFT_GUARD` | `gamepad_deadzone_index_move` | `SMALL` 1, `LARGE` 2 | 1 | `0x1000000` |
| `SwchLookInvert` | `#LOOK_INVERT` | `joy_inverty` | `OFF` 0, `INVERTED` 1 | 0 | `0x1000000` |
| `SwchVibration` | `#VIBRATION` | `joy_rumble` | `OFF` 0, `ON` 1 | 1 | `0x1000000` |
| `BtnControllerOpenAdvancedMenu` | `#OPEN_ADVANCED_LOOK_CONTROLS` | opens `ControlsAdvancedLookMenu` | | | |

With `gamepad_custom_enabled` on, the look rows are disabled and their
`#CONTROLS_ADVANCED_LOOK_ENABLED` labels show (`panel_controls_gamepad.nut`). The advanced
look menu (`advanced_look_controls.res`): `gamepad_custom_enabled` 0, `_deadzone_in` 0.15
(slider 0 to 0.5), `_deadzone_out` 0.02 (0.01 to 0.3), `_curve` 10.0 (0 to 30),
`_assist_on` 1, `_hip_yaw` 160.0, `_hip_pitch` 120.0 (0 to 500), `_hip_turn_yaw` 220.0,
`_hip_turn_pitch` 0.0 (0 to 250), `_hip_turn_time` 0.33, `_hip_turn_delay` 0.0 (0 to 1), and
the same six for `_ads_`: 110.0, 75.0, 30.0, 30.0, 1.0, 0.25; all `gamepad_custom_*`, flags
`0x1000000`.

Video. The engine panel sets its own rows (no ConVar in the layout) except four:

| Row | Label | ConVar | Options | Default | Flags |
|---|---|---|---|---|---|
| `SwchDisplayMode` | `#DISPLAY_MODE` | engine | `#GameUI_Fullscreen`, `#GameUI_Windowed`, `#GameUI_BorderlessWindow` | | |
| `SwchAspectRatio` | `#ASPECT_RATIO` | engine | | | |
| `SwchResolution` | `#RESOLUTION` | engine | `%i x %i%s` | | |
| `SldBrightness` | `#BRIGHTNESS` | engine | slider 0 to 1, step 0.05 | | |
| `SldFOV` | `#FOV` | engine; `cl_fovScale` has the slider's range (unconfirmed) | slider 1.0 to 1.7, step 0.0275 | 1.27216005 (1 to 1.7) | `0x41000200` |
| `SwchColorBlindMode` | hidden (`visible 0`; on the Gameplay tab) | | | | |
| `SwchSprintCameraSmoothing` | `#SPRINT_VIEW_SHAKE` | engine | | | |
| subheader | `#MENU_ADVANCED` | | | | |
| `SwchVSync` | `#VSYNC` | engine | `#DOUBLE_BUFFERED`, `#TRIPLE_BUFFERED`, `#ADAPTIVE_VSYNC`, `#ADAPTIVE_VSYNC_HALF`, `#DYNAMIC_VSYNC`, off | | |
| `SwchReflex` | `#REFLEX` | `settings_reflex` | Off 0, On 1, On + Boost 2 | 1 (`gamesdk.dll`) | `0x80000` |
| `SwchAntiLag` | `#ANTILAG` | `settings_antilag` | Off 0, On 1 | 1 (`gamesdk.dll`) | `0x80000` |
| `SldFpsMax` | `#FS_FPS_MAX` | `fps_max` | slider −1 to 300, step 1 | −1 | `0x80080` |
| `SldAdaptiveRes` | `#ADAPTIVE_RES` | engine | slider from 0, step 1 | | |
| `SwchAdaptiveSupersample` | `#ADAPTIVE_SUPERSAMPLE` | engine | | | |
| `SwchAntialiasing` | `#ANTIALIASING` | engine | | | |
| `SwchTextureDetail` | `#TEXTURE_QUALITY` | engine | | | |
| `SwchLODQuality` | `#LOD_QUALITY` | `r_lod_switch_scale` | `LOD_0` 0 … `LOD_9` 9 | 1 | `0x40000000` |
| `SwchFilteringMode` | `#MENU_TEXTURE_FILTERING` | engine | `#GameUI_Bilinear`, `#GameUI_Trilinear`, `#GameUI_Anisotropic2X` to `16X` | | |
| `SwchAmbientOcclusionQuality` | `#MENU_AMBIENT_OCCLUSION_QUALITY` | engine | | | |
| `SwchSunShadowCoverage` | `#MENU_SUN_SHADOW_COVERAGE` | engine | | | |
| `SwchSunShadowDetail` | `#MENU_SUN_SHADOW_DETAILS` | engine | | | |
| `SwchSpotShadowDetail` | `#MENU_SPOT_SHADOW_DETAILS` | engine | | | |
| `SwchVolumetricLighting` | `#VOLUMETRIC_LIGHTING` | engine | | | |
| `SwchDynamicSpotShadows` | `#MENU_DYNAMIC_SPOT_SHADOWS` | engine | | | |
| `SwchModelDetail` | `#MENU_MODEL_DETAIL` | engine | | | |
| `SwchEffectsDetail` | `#MENU_EFFECT_DETAIL` | engine | | | |
| `SwchImpactMarks` | `#MENU_IMPACT_MARKS` | engine | | | |
| `SwchRagdolls` | `#MENU_RAGDOLLS` | engine | | | |

The engine's option labels near the row names include `#gameui_low`, `#gameui_medium`,
`#gameui_high`, `#SETTING_MINIMAL`, `#SETTING_NORMAL`, `#ASPECT_21_9`, `#ASPECT_TALL`,
`#ASPECT_LEGACY`. Which row takes which, and each row's ConVar, are not read yet.
`LblTextureStreamWarning` and `LblFPSWarning` are hidden labels the panel shows.

Audio.

| Row | Label | ConVar | Options | Default | Flags |
|---|---|---|---|---|---|
| `SldMasterVolume` | `#MASTER_VOLUME` | `sound_volume` | slider 0 to 1, step 0.05 | 1 | `0x1000000` |
| `SwchAudioLanguage` | `#AUDIO_LANGUAGE` | `miles_language` | `#GAMEUI_LANGUAGE_ENGLISH` english, `_FRENCH`, `_GERMAN`, `_ITALIAN`, `_JAPANESE`, `_KOREAN`, `Chinese` mandarin, `_POLISH`, `_RUSSIAN`, `_SPANISH` | empty | `0x1000000` |
| subheader | `#MENU_VOICE_CHAT` | | | | |
| `SwchPushToTalk` | `#OPTIONS_MENU_VOICE_CHAT_MIC` | `TalkIsStream` | `PTT` 0, `OPENMIC` 1 | 0 | `0x80` |
| `SldOpenMicSensitivity` | `#OPEN_MIC_SENS` | `speex_quiet_threshold` | slider 0 to 10000, step 50 | 1300 | `0x80` |
| `SldVoiceChatVolume` | `#VOICE_CHAT_VOLUME` | `sound_volume_voice` | slider 0 to 1, step 0.05 | 1 | `0x80` |
| subheader | `#MENU_ADVANCED` | | | | |
| `SldSFXVolume` | `#MENU_SFX_VOLUME_CLASSIC` | `sound_volume_sfx` | slider 0 to 1, step 0.05 | 1 | `0x1000000` |
| `SldDialogueVolume` | `#MENU_DIALOGUE_VOLUME_CLASSIC` | `sound_volume_dialogue` | slider 0 to 1, step 0.05 | 1 | `0x1000000` |
| `SldMusicVolume` | `#MENU_MUSIC_VOLUME_CLASSIC` | `sound_volume_music_game` | slider 0 to 1, step 0.05 | 1 | `0x1000000` |
| `SldLobbyMusicVolume` | `#MENU_LOBBY_MUSIC_VOLUME` | `sound_volume_music_lobby` | slider 0 to 1, step 0.05 | 1 | `0x1000000` |
| `SwchSoundWithoutFocus` | `#SOUND_WITHOUT_FOCUS` | `sound_without_focus` | `OFF` 0, `ON` 1 | 0 | `0x1000000` |
| `SwchChatTextToSpeech` | `#MENU_CHAT_TEXT_TO_SPEECH` | `hudchat_play_text_to_speech` | `OFF` 0, `ON` 1 | 0 | `0x1000000` |
| `SwchChatSpeechToText` | `#MENU_CHAT_SPEECH_TO_TEXT` | `speechtotext_enabled` | `OFF` 0, `ON` 1 | 0 | `0x1000000` |
| `SwchSpeakerConfig` | `#WINDOWS_AUDIO_CONFIGURATION` | `miles_channels` | `ONE_CHANNEL` 1, `TWO_` 2, `FOUR_` 4, `SIX_` 6, `SEVEN_` 7, `EIGHT_` 8, `NINE_CHANNEL` 9 | 0 | `0x40000000` |

The two chat rows show only when `IsAccessibilityAvailable()`; `SwchAudioLanguage` is
enabled only when `IsAudioLanguageChangeAllowed()` (native, not yet read).

### The frame cap

`0x2970e0`, each frame, with the video config (material system `+0x180`):

- When `sv_cheats` is 0 and a flag (`0xb37b885`) is clear, an `fps_max` above 0 and below 30
  is set to 30 (`0x297192`).
- Windowed (`+0x14` bit 0, taken to be that) or with vsync off (`+0x17` 0): the cap is
  `fps_max` ("Frame rate limiter. -1 indicates the use of desktop refresh. 0 is unlocked.").
  - −1 becomes the desktop's refresh as a whole number (`0x1741e94`, 240 in the dump);
    59 and 60 become 59.94.
  - Then, for −1 only: with vsync off, at least 144; with vsync mode 3 or more, times 1.05.
- Otherwise the cap is `fps_max_vsync` (default 0), or the refresh when
  `fps_max_use_refresh` is set.
- A cap of 0 or less (NaN too) is 300: "0 is unlocked" does not hold in this build.
- At most 10000. What is done with the cap from `0x297274` on (the wait) is not read.

### The ConVar files

Where (`0xc93ed0`, `0xc93dd0`, `0xc93ca0`): the Saved Games known folder
(`4c5c32ff-bb9d-43b0-b5b4-2d72e54eaaa4`; failing that, CSIDL 5, Documents) plus
`/Respawn/Apex` (`/Respawn/Apex_fnf` with `-fnf` on the command line). Then by kind: 0
`/local/settings.cfg`, 1 `/profile/profile.cfg`, 2 `/local/videoconfig.txt`, 3 `/profile/savegames/`,
4 `previousgamestate.txt`, else `error.cfg`.

Reading (command `execPlayerConfig`, "Load player settings.", `0x22ffc0`): `settings.cfg`, then
`profile.cfg` (`0x22fea0`). A file of `0x3fff` bytes or more is not read. A file is run as
console commands (`0x20de80`): lines end at any byte below `0x20` but tab. A ConVar's line
sets it from a string.

Writing (`0x22fcb0`, both files, `settings.cfg` first):

- `settings.cfg`: the key binds (`0x241030`), then every ConVar with FCVAR_ARCHIVE (`0x80`).
- `profile.cfg`: every ConVar with FCVAR_ARCHIVE_PLAYERPROFILE (`0x1000000`).
- Each ConVar is a line `%s "%s"\n` (`0x212260`): name, then value. The value is its string, or
  `FCVAR_NEVER_AS_STRING` with that flag (`0x1000`). Commands are skipped.
- Order is by name, compared byte by byte with A to Z lowered (`0x466d30`).
- Binds: for each key, `bind_US_standard "KEY" "COMMAND" N` and `bind_held_US_standard` for its
  held binding (`%s "%s" "%s" %i\n`), with `\` and `"` escaped and CR and LF as `\r`, `\n`. N
  is a byte of the key's record (`+0x25`); the user's file has 1 on `MOUSE4`, `MWHEELUP`,
  `MWHEELDOWN` and `i`. Split-screen players' binds are `cmd%d %s "%s" "%s" %i`.
- The game's files on disk end lines with CR LF.

When: command `savePlayerConfig` ("Store player settings.", `0x230170`) sets a flag. The frame
code then writes both files once (`0x231d8e`, `0x2396c0`), when two more flags are set:
`0xb37c52f` (the binds were read, `0x23013b`) and `0xb37d969`. Scripts reach it through
`SetConVar*` (`0x8d48b0` and its neighbours). Each finds the ConVar, sets it from the value, and
queues `savePlayerConfig` only when the ConVar has `0x1000000`. So a script setting a
`settings.cfg` ConVar alone does not save. The settings menu's own script
(`lobby_panel_settings.nut`, `OnSettingsPanel_Hide`) only notes what changed and sends it to
telemetry. Other callers queue it too: the binds' natives (`SetAbilityBinding`, `0x7ab710`),
`0x5bc830`, `0x7894a0`.

Setting a ConVar:

- From a string (`0x470040`): first as a colour (`0x46ff90`, `sscanf` `%i %i %i %i`). Three or
  four numbers, each below 256 unsigned, alpha 255 when left out, become the int's bytes
  (r, g, b, a from the low byte); the float is that int.
  Otherwise `atof` (the CRT's, `0x11b2f0c`) as a float, an infinity or NaN taken as `FLT_MAX`,
  then clamped (`0x470300`).
  The clamp: below the minimum (`v <= min && min != v`) it is the minimum; otherwise above the
  maximum it is the maximum. When clamped, the string becomes the clamped value printed with
  `%f`. The int is the float truncated (`cvttss2si`).
  The string is kept as given, unless `0x1000`.
- From a float (`0x470340`): nothing when it equals the float. Otherwise it is clamped and the
  string printed with `%f`.
- From an int (`0x470420`): nothing when it equals the int. Otherwise the float is the int
  clamped, the int is truncated from it when clamped, and the string printed with `%d`.
- `ConVar::Create` (`0x470540`): the default is the string, read as above but not clamped.

The object: `+0x38` flags, `+0x48` parent (the one holding the value), `+0x50` default,
`+0x58` string, `+0x60` its size, `+0x68` float, `+0x6c` int, `+0x70`/`+0x74` has-min/min,
`+0x78`/`+0x7c` has-max/max. Its vtable's `+0x70`, `+0x78`, `+0x80` and `+0x98` are the three
setters and the clamp.

R5Reloaded's ConVars:

- `gamesdk.dll` registers `settings_reflex` and `settings_antilag`, default "1", flags
  `0x80000` (FCVAR_RELEASE), no range.
- Mods declare others in their `mod.vdf` ("ConVars": flags by name, "Values" default, min,
  max). The SDK's flag names map to the engine's bits: `ARCHIVE` `0x80`, `RELEASE`
  `0x80000`, `ARCHIVE_PLAYERPROFILE` `0x1000000`, and the rest of Source's (`gamesdk.dll`,
  `0x5a224` on).
- The enabled mods (`mods/mods.vdf`: Flowstate, kralstuff, movementgym, SDK.Core) declare
  seven of the menu's, all `ARCHIVE|RELEASE`, 0 to 1:
  - SDK.Core: `enable_motd` 1, `open_motd_once_per_server` 1, `cl_showpos_archived` 0,
    `cl_showfps_archived` 0, `show_dev_warning_dialogue` 1;
  - Flowstate: `enable_healthbar` 1, `enable_movement_overlay` 0.

Not settled: the game's files and the dump's flags disagree for twelve of the menu's ConVars.
The user's files of 2026-10-14 lack the seven mods' ConVars, and lack `fps_max` (`0x80080`),
`speex_quiet_threshold` and `TalkIsStream` (`0x80`) in `settings.cfg`. They also lack
`sound_volume` and `miles_language` (`0x1000000`) in `profile.cfg`. Yet they hold
`miles_channels`, whose flags are `0x40000000`. `IsFlagSet` (`0x46fe90`) is a plain test of the
parent's flags. So the flags must differ in the running game when it writes; check them with
the console's `find` once the game runs. lurch follows the dump.

### KeyValues and the layout files

The engine's KeyValues reader (`0x475dc0`) loads VGUI's `.res`, `.menu` and scheme files.

Tokens (`0x473350`): whitespace is bytes 9 to 13 and space; `//` at a token's start runs to the
end of the line (`0x47dd40`). A `"` starts a quoted token up to the next `"`, without escapes
for these files. `{` and `}` are tokens of their own. Any other token runs to whitespace, `"`,
`{`, `}` or NUL. Inside `[`...`]` whitespace does not end it, and a `]` after a `[` marks the
token a condition. So `#SETTINGS` unquoted is a value, and `564//-1568` is one token (the
scheme's `SettingsDetailsPanel` has such values). A token keeps at most 1023 bytes.

A file (`0x475dc0`): top-level keys in turn. `#include` and `#base` take the next token as a
file: an included file's keys are appended after the file's, a base is merged into them
(`0x475c60`, not read). Any other token names a
key; a condition may follow the name, then `{` and the block.

A block (`0x4765e0`): until `}`, a name, then an optional condition, then `{` and a nested
block, or a value. After a value one more token is read: a condition is evaluated, anything
else is put back. A false condition removes the key. A second condition after the first,
or an unquoted `}` as a value, ends the block with an error; the key stays, since a key is
added to its block (`0x474790`, at the end, so names repeat in file order) before its value is
read. Names are symbols compared without case (`0x59d3b0`), and a lookup takes the first.
A value is typed: `0x` and 16 hex digits is a 64-bit integer; else a float when `strtod` reads
further than `strtol` and to the end; else an integer when `strtol` (base 10) reads to the end
and is not out of range; else a string, as is an empty value.

Conditions (`0x475d60`, `0x4723d0`): a leading `[` and trailing `]` are dropped. The grammar:

- `0x4721b0`: terms joined by `&&` or `||`, of one precedence, left to right (so
  `$A || $B && $C` is `($A || $B) && $C`). After `|` or `&` the next byte must hold that
  byte's bits (`next & op == op`), with no space between.
- `0x4720d0`: a `!` node's value is its right side's negated; what stood before it is dropped.
- `0x471e70`: `(` an expression, then the next byte is taken as the `)` unchecked; `$` and
  letters, digits and `_` (under 127 bytes) is a symbol; digits are a number, true when not 0;
  a `!` leaves an empty node (false). Anything else does not parse.
- Evaluation (`0x472330`) takes both sides. A condition that does not parse is false.

Symbols, without `$` and case (`0x471d40`): `WIN32`, `WIN64`, `WINDOWS`, `PC` are true;
`GAMECONSOLE`, `DURANGO`, `PS4`, `DEMO` false. Then the table the video mode fills
(`0x406fa0`, through the KeyValues system's vtable `0x13aa1f0` `+0x40`): `WIN32WIDE` and
`WIN32HIDEF` 1; `WIN32LODEF`, `GAMECONSOLEWIDE`, `GAMECONSOLEHIDEF`, `GAMECONSOLELODEF`,
`ANAMORPHIC`, `DURANGO` 0; `WIDESCREEN_16_9` when width over height is above 1.77. Then
(`0x59d920`, `+0x48`): `GAMECONSOLESPLITSCREEN` and `GAMECONSOLEGUEST` false; a language
name (`ENGLISH`, `JAPANESE`, `GERMAN`, `FRENCH`, `SPANISH`, `ITALIAN`, `KOREAN`, `TCHINESE`,
`PORTUGUESE`, `SCHINESE`, `POLISH`, `RUSSIAN`, `TURKISH`) true when it is the game's
language (`english` by default); `CVAR_` names read a value not identified. Any other symbol
(`NX`, `NX_UI_PC`) is false.

The scheme (`0x524510`): the file's first key holds `BaseSettings`, `Colors`,
`InheritableProperties` and `ButtonChildGroups`. Each template in `InheritableProperties` is
resolved in order (`0x524790`): its `InheritProperties` key is removed, the named template is
resolved first, then its keys the template lacks are appended. A chain stops at 16, and where
it would repeat.

A panel:

- A file applies as LoadControlSettings does: each block of its first key goes to the panel of
  that name, or makes one (BuildGroup's ApplySettings, `0x48f500`, vtable `0x136e140` `+0x30`).
  Names match without case. A block named `menu` goes to a panel whose name starts with
  `menu`.
- A new panel (`0x48f760`) has the class of its block's own `ControlName`, read before any
  template, and the block's name.
- Its settings (`0x4ca3c0`): a copy of the block, then each key of the scheme template named by
  `InheritProperties` that the block lacks, appended; only top-level keys are compared, so a
  block's own `ruiArgs` hides the template's whole. The panel's ApplySettings (vtable `+0x2f8`)
  gets the copy.
- EditablePanel's ApplySettings (`0x49f980`), Frame's (`0x4a6cc0`) and CNestedPanel's
  (`0x979760`) apply Panel's (`0x4ca510`), then hand the settings to their build group: the
  settings' blocks become children. By RTTI, Frame, CNestedPanel, GridButtonListPanel and
  SliderControl derive from EditablePanel; Label, ImagePanel, RuiPanel, TextEntry, RichText
  and SectionedListPanel do not.
- CNestedPanel then loads its `controlSettingsFile`, when set, into its children. It warns
  when an ancestor has its name, but loads anyway.
- Button's ApplySettings (`0x499450`; RuiButton's `0x938ee0` calls BaseModHybridButton's
  `0x946bf0`, which calls it) reads `command`, `default`, `selected`, the sounds and click
  events, then the child groups in this order: `childGroupAlways`, `childGroupNormal`,
  `childGroupDisabled`, `childGroupFocused`, `childGroupSelected`, `childGroupLocked`,
  `childGroupNew` (`0x4998c0`). Each names a group of the scheme's `ButtonChildGroups`, whose
  blocks a new build group over the button makes into children. Each child is shown when the
  button's state holds that group's bit, and its vtable `+0x470` is called with whether the
  group is Always (what that does is not read).

### Layout and Solve

Panel's ApplySettings (`0x4ca510`) sets, in this order: `proportionalToParent`, position,
`navUp`, `navDown`, `navLeft`, `navRight`, `zpos`, size, `polyShape`,
`cursorPriority`, the title-safe insets, then `AutoResize` (`0x4c9d50`), `IgnoreScheme`,
`visible`, `enabled`, `mouseEnabled`, `tabPosition`, `tooltiptext`, `paintbackground`,
`paintborder`, `border`, `clip`, `activeInputExclusivePaint`, `xcounterscroll`/`ycounterscroll`,
`cursorVelocityModifier` (default 1.0), `fieldName`, `RoundedCorners`, the pin,
`pin_corner_to_cursor` (default 10), the colour overrides, `scriptID`, `classname`.

The reference for `r`, `c`, `f` and `%` is the screen (ISurface `+0x1a8`), or the parent's size
when `proportionalToParent` is 1.

- `xpos`, `ypos` (missing: the panel's current position): `r` or `R` is the reference less the
  value, `c` or `C` the value plus half the reference (integer halving), `%` is
  `(int)((float)atof(rest) / 100 * reference)`. Otherwise `atoi`. The value after `r`, `c` or
  none is scaled when the panel is proportional. Text after the number is ignored, so
  `564//-1568` is 564. Set before the size is read, so a size never moves a position.
- `wide`, `tall` (missing: the current size): `f` or `F` is the reference less the scaled
  value, `%` as above, otherwise the scaled `atoi`.
- `zpos`: an int, missing keeps the current one. `visible`: missing or 1 shows, 0 hides, any
  other value leaves it. `clip` (flag `+0x98` bit 16, read back by Panel `+0xb8`): missing or
  above 0 clips, 0 does not, below 0 leaves it.
- Positions and sizes are kept as 16-bit values in the VPanel (`+0x4c`, `+0x50`).

Scaling (scheme manager `0x1398350` `+0x78`, `0x523830`, then `+0x58`, `0x523600`, and `+0x68`,
`0x5236d0`): `(int)floorf((float)tall / 1080 * value)` in f32, the base being 1920 × 1080
(surface `+0xb8`, `0x530d60`). With `vgui_EnableFixedAspectScaling` 1 (the image's default)
`tall` is that of the largest 16:9 inside the screen (`0x5458f0`): when `1080 / h <= 1920 / w`
it is `(int)(1080 / (1920 / w))`, else `h`. So at 1920 × 1080 nothing changes, and a wider
screen scales by its height. A scheme with its own proportional panel scales by that panel's
height instead (not used here). IsProportional is `+0x98` bit 12 (`0x1fd900`); SetProportional
(`0x4cdfd0`) passes it to children, and SetParent (`0x4c49b0`) takes the parent's.

The pin (`0x4c95f0`): `pin_to_sibling` names a sibling. `pin_corner_to_sibling` (the panel's
corner) and `pin_to_sibling_corner` (the sibling's) are corner names, compared with case, or
else the value as an int. The corners in order (`0x1314f10`), with their place as fractions
of a panel's width and height (`0x1314f90`): `TOP_LEFT` (0, 0), `TOP_RIGHT` (1, 0),
`BOTTOM_LEFT` (0, 1), `BOTTOM_RIGHT` (1, 1), `TOP` (0.5, 0), `RIGHT` (1, 0.5), `BOTTOM`
(0.5, 1), `LEFT` (0, 0.5), `CENTER` (0.5, 0.5). The sibling is found by name (`0x4c4e20`)
when the pin is set and again when the panel is made ready for use (`0x4c3700`), and handed to
the VPanel (`+0x98` sibling, `+0xa0` own corner, `+0xa1` the sibling's; `0x52c210`).

VPanel's Solve (`0x52a570`), run for each panel and then its children:

- Unpinned, the position is `xpos`, `ypos`. Pinned, each axis is, in f32,
  `(int)((float)(int)(sibling size * sibling fraction + sibling position) - own size * own
  fraction)`, plus the offset as a 16-bit sum, where the sibling's position is its absolute
  one less the parent's. The offset is subtracted on x when the sibling's corner is
  `TOP_LEFT`, `BOTTOM_LEFT` or `LEFT`, and on y when it is `TOP_LEFT`, `TOP_RIGHT` or `TOP`:
  it moves the panel away from the sibling. So the settings menu's tab content, `ypos -128`
  pinned `TOP` to `TOP`, sits 128 below the top.
- The absolute position is that plus the parent's absolute position and inset, clamped to
  ±32767. A term `(float)(a - b) * c * (float)panel[+0x70]` is added on each axis (an
  animation; 0 here).
- The panel's rectangle is clamped into the parent's clip rectangle (less the parent's
  inset on the right and bottom). Its own clip rectangle is that, or the parent's when
  `clip` is off.

### RuiPanel and RuiButton

RuiPanel (vtable `0x1484080`; ApplySettings `0x938b10`, paint `0x938b00` at `+0x438`):

- ApplySettings: Panel's first, then the old instance is freed. `rui` names the element's
  asset (`0x720ea0`): it must start `ui/` (case ignored) and end `.rpak`, and is found by the
  name's GUID; otherwise the panel has no element. `clipRui`: missing or not 0 sets it.
- `ruiArgs`: the instance is made now (`0x937ef0`), then each key sets the argument of its
  name by the value's KeyValues type: a string tries a string argument (type 1), then four,
  three and two floats (types 8, 7, 6, parsed `%f %f %f %f`, `%f %f %f`, `%f %f`, set only when
  all parse); an integer tries int (4), then bool (3, not 0); a float tries float (5).
- The instance (`0x937ef0`, `0x720fc0`): made once, in draw group 0 with sort key 0 and no
  topology handle. Its resolution (`0x937e40`) is the panel's size times `1080 / tall`, `tall`
  being the surface's (`+0x1b0`), with the reciprocals.
- Paint: the topology is the panel's absolute position with its width across and its height
  down (two copies, `+0x290` and `+0x2c0`), set on the instance (`+0x58`); the resolution is
  set again. The argument named by the ConVar `ruiPanel_resArgName` ("actualRes") gets two
  floats from `0x201ce0` for the panel's size (not read). With `clipRui`, `clipRect` (four
  floats) gets the panel's clip rectangle less its position, over its size, then the clip
  rectangle's size times the element's own size (`+0x18`, `+0x1c`) over the panel's. Then the
  instance is queued for this paint (`0x71ff70`).

RuiButton (vtable `0x1485228`, its instance at `+0x6d0`; ApplySettings `0x938ee0`, paint
`0x938d80`):

- ApplySettings: the cursor priority 5, the instance from the settings as RuiPanel's
  (`0x938260`), then BaseModHybridButton's (`0x946bf0`, which runs Button's, `0x499450`).
  `behave_as_label` (default 0); with it, `ruiDefaultHeight` and `ruiFont` set the label's
  height and font.
- Paint: the button's text (`+0x780`) is localized (`0x7a1bf0`) into the `buttonText` string
  argument, empty when the text is; then as RuiPanel's paint.
- Button states (`0x949880`): a state's bit is set or cleared; that child group's children
  are shown or hidden; then every child of the Always group that has an instance gets the
  state's bool argument (`0x9388b0`), and RuiButton (`0x9390b0`) sets it on its own instance
  too. The arguments by state (`0x131a9d0`): 2 `isDisabled`, 3 `isFocused`, 4 `isSelected`,
  5 `isLocked`, 6 `isNew`; states 0 and 1 have none.
- A DialogListButton (the switch rows, `0x9456e0`) sets its value child's `buttonText` to the
  current choice's label and the ints `itemCount` and `selectedIndex`;
  with two choices, each choice button's `isSelected` and `buttonText`. Entering and leaving it
  (`0x94fc60`, `0x94fe00`) set `isFocused` on its value and Always children.

### ImagePanel and paint order

ImagePanel (vtable `0x137a100`; ApplySettings `0x4a9c00`, ApplySchemeSettings `0x4aa350`,
PaintBackground `0x4a94f0` at `+0x430`):

- ApplySettings reads `scaleImage`, `scaleAmount`, `tileImage`, `tileHorizontally` and
  `tileVertically` (both default to `tileImage`), `tileHorizontalOverride`,
  `tileVerticalOverride` (scaled by the screen's height over the surface's `+0x3b0` when
  proportional), `image` (SetImage `0x4a93e0` keeps the name), `frame`, `fillcolor`,
  `drawcolor` and `border`. A colour is `%d %d %d %d` with alpha 255 when left out; with fewer
  than three numbers it is a scheme colour's name (scheme `+0x28`).
- ApplySchemeSettings makes the image (scheme manager `+0x38`, GetImage `0x523a00`): a leading
  `material/` or `material\` is dropped (case kept), everything from `_gen.rpak` on is cut,
  and the image is kept by that name. Its texture is made with the surface's
  `CreateNewTextureID` (`+0x1a0`) and `DrawSetTextureFile` (`+0x140`, `0x52a150`). The menus'
  images are `txtr` assets named `texture/NAME.rpak` in `common_early.rpak` (found by GUID;
  how the surface builds that name is not traced). A name ending `.pic` is flagged.
- PaintBackground: the clip becomes the parent's clip rectangle (surface `+0x598`), restored
  after. A `fillcolor` with alpha fills the panel (`+0x68`, `+0x80`). The image takes the draw
  colour and position 0, 0; with `scaleImage` it is sized to the panel (or its own size times
  `scaleAmount` when that is above 0), painted, and its size put back. Without it, tiled or at
  its own size.

Paint order: VPanel's SetZPos (`0x52b6b0`) stores the `zpos` as 16 bits, then moves the panel
back past each sibling with a greater `zpos` and on past each with a smaller one, so children
end up in `zpos` order and, for equal ones, in the order they were made. A hidden panel paints
nothing, children included. A RuiPanel's paint queues its instance (`0x71ff70`) into a list
the surface draws; that it draws in place among the surface's own draws is assumed.

PaintTraverse (`0x4c4130`): a panel whose clip rectangle is empty (right not past left, or
bottom not past top) does not paint, and passes that on to its children, whose rectangles lie
inside it. Otherwise, in order: the border when enabled (`+0x98` bit 2) and its paint is
before the children's, PaintBackground (`+0x430`, bit 3), Paint (`+0x438`, bit 4), the
children (each only when visible), then the border after them and PostChildPaint (`+0x450`,
bit 5). The surface's alpha multiplier is the panel's alpha (`+0x204`) times the parent's
for the panel and its children.

### Input

The panel under the cursor (Panel's IsWithinTraverse `0x4c7fd0`, Panel vtable `+0x38`): none
unless the panel is visible (IPanel `+0xf8`) and takes mouse input (`+0x1b8`). Outside popups:
when the point is within the panel (`0x4c7f00`), its children from the last to the first
(skipping popups) are asked the same; the first that answers wins. Otherwise the panel
itself, unless `mouseEnabled` 0 set its flag (`+0x98` bit 15), which leaves its children
reachable. Within (`0x4c7f00`): with a poly shape, VPanel's IsWithin (`0x52b0e0`: the point
over the panel's absolute rectangle in 0 to 1 both ways, edges included, then inside every
edge of the shape); otherwise inside the panel's clip rectangle, left and top included, right
and bottom not.

Button states (SetButtonState `0x949880`, vtable `+0x9a8`; Recovered, "RuiPanel and RuiButton"):

- Made (`0x497820`): Always on.
- The cursor entering (`0x498450`, `+0x338`) repaints and takes the focus when the button does
  not have it, then sets Normal off, Focused on when enabled (off when not) and Disabled to
  not enabled. Taking the keyboard focus (`0x498650`, `+0x360`) sets the same three.
- The cursor leaving (`0x498500`, `+0x340`) disarms (`+0x8c8`), repaints, then, as the focus
  being lost (`0x497fc0`, `+0x368` and `+0x900`): Normal to enabled, Focused off, Disabled to
  not enabled.
- One button is the focused one (`0x0d4108c8`): taking the focus (`0x498040`, `+0x908`) gives
  the one before it `+0x900`, so moving onto another button returns the first to Normal.
- SetSelected (`0x497b30`, `+0x8f0`) keeps its own flag (`+0x320` bit 7) and sets Selected.

VGUI's fonts, for Label: the scheme's `Fonts` block names faces such as `Default` with a
`tall`, `weight`, `antialias` and flags, and `isproportional`. `sourcescheme.res`'s
`CustomFontFiles` loads `resource/*.vfont`, TrueType files with Valve's `VFONT1` trailer
(encrypted). `resource/fontfiletable.txt` (read at `0x5339b0`, section `FontFileTable`) remaps
face names by language; it is in neither the host's copy nor the guest's loose files.
`DefaultBoldFont`, which the subheaders use, is not defined in any scheme file read. The image
imports `AddFontMemResourceEx` and `ExtTextOutW` from GDI but no `CreateFont` or
`GetGlyphOutline`, so how the engine rasterizes glyphs is not known yet. None of this matters
for the menus: their text goes through RUI ("VGUI's text").

A RuiPanel is never the panel under the cursor itself: its constructor (`0x938950`) sets the
flag `mouseEnabled` 0 sets (`+0x98` bit 15), and Panel's ApplySettings only ever sets that
flag (`mouseEnabled` present and 0), never clears it. Its children can still be. RuiButton's
constructor leaves it unset.

The scripts' cursor (`GetCursorPosition`, `0x7a7d20`) is the game cursor (`0x22c75e0`), in
1920 × 1080 units whatever the screen: a mouse move puts it at the mouse (`0x6f0130`), each
axis `1920 / wide × x` and `1080 / tall × y` rounded to the nearest (`0x201ce0`), when it moved
by more than 0.15 units. Its bounds are `0x201cd0`: 1920 × 1080. `gameCursor_ModeActive` is 1 in
the image. `TEMP_CursorInElementBounds` (`_menu_utility.nut`) scales it back to the screen.

### Menu sounds

A menu's sound is a name to the surface's PlaySound (`0x5460a0`, surface vtable `+0x338`),
which hands it to the callback at surface `+0x10198` (set through `+0x610`; in the dump
`0x2823b0`, which calls the client's `0x5c2b20`). That warns on an empty name; otherwise it
queues the name (CSOM_AddEventToQueue, `0x95b480`) with position type 2, at the emitter
`0x255e292a` names, which is not read further, and runs it (`0x956660`, `0x954c60`).

A BaseModHybridButton (RuiButton among them) has ten sound slots, read by ApplySettings
(`0x946bf0`) from the keys in the table at `0x131aa60`, the block's or its template's, an
empty value included. Its constructor (`0x944b10`) sets each to `__invalid__`. Playing a slot
(`0x9499f0`): `__invalid__` plays the slot's default (`0x92d7b0`, slots 0 to 7 only), an
empty name nothing, any other name itself.

| Slot | Key | Default |
|---|---|---|
| 0 | `sound_back` | `UI_Menu_Back` |
| 1 | `sound_accept` | `UI_Menu_Accept` |
| 2 | `sound_invalid` | `Menu.Invalid` |
| 3 | `sound_countdown` | none |
| 4 | `sound_focus` | `UI_Menu_Focus` |
| 5 | `sound_click` | `Menu.Click` |
| 6 | `sound_deny` | `UI_Menu_Deny` |
| 7 | `sound_mutedeny` | `Menu.Mute.Deny` |
| 8, 9 | `sound_title_click1`, `sound_title_click2` | none |

- The cursor entering (`0x948f70`, vtable `+0x360`; read as the cursor entering because
  `+0x370` takes a mouse code, while "Input" below names `+0x338` for it and `+0x360` for the
  keyboard focus, so one reading is wrong): after Button's handler, when no panel
  holding the keyboard focus refuses (its `+0x238`, true for every panel here) and the
  button is enabled, slot 4 plays unless a flyout is open (`+0x484`, set only by the flyout
  code at `0x93bba1` and `0x93be5e`) or the button already holds the keyboard focus
  (`+0x230`).
- A mouse press (`0x945050`, `+0x370`): the right button opens a flyout and nothing else.
  Another button on an enabled button: on a DialogListButton (`style` 3, `+0x494`), the left
  button steps its choice by the side it is pressed on (`0x948d70` with 1 or 2), or plays slot
  6 when Locked; on any other button, unless `hasScriptSound` (`+0x470`), slot 6 when Locked
  (state bit 5), else slot 1.
- A DialogListButton's commands (`0x948300`, none while Locked): `DialogListSelect<n>` makes
  the choice and plays slot 1; `DialogListNext` and `DialogListPrev` play slot 1 when the
  choice moves and slot 2 when it lands where it started (`0x948d70`).
- The settings tabs' template `TabButtonSettings` has `sound_focus` `UI_Menu_Focus_Large` and
  `sound_accept` `UI_Menu_SettingsTab_Select`; the switches' `LeftButton` and `RightButton`
  (`ChoiceButtonAlways`) have `sound_accept` empty, so a pick sounds only through its list.

A SliderControl plays `UI_Menu_Focus` whenever the cursor enters it (`0x94f870`), and
`UI_Menu_Accept` when SetValue (`0x94dd90`) changes its value while not dragging (`+0x2d3`).
Its arrow keys (`0x94f160`) and tab switches by the shoulder keys (`_tabs.nut`'s
`tabLeftSound`, `tabRightSound`, both `UI_Menu_ApexTab_Select` unless a menu sets them) are
not modelled.

### VGUI's text

VGUI's text goes through RUI (found 2026-10-16): `rui_overrideVguiTextRendering` is 1 in the
image (flags `0x2`), and with it (and no `-tools` on the command line) TextImage's paint
(`0x4ff7b0`) does not draw glyphs from the surface's font pages; TextEntry's (`0x4f2d00`) does
the same for its lines. The font files (`.vfont`, `fontfiletable.txt`) feed only those pages.

The block a TextImage hands over (at `+0x340`, filled in its paint):

| Offset | What |
|---|---|
| `+0x340` | colour, four floats: the draw colour's bytes over 255, sRGB to linear (surface `+0x530`, `0x544700` through `0x270130`), alpha times the surface's alpha multiplier |
| `+0x350`, `+0x360`, `+0x370` | colours a `^RRGGBBAA` code switches to (`0x4ff4c0` writes `` `n `` for them) |
| `+0x380` | the clip: the TextImage's `+0x318` to `+0x324` (x, y, width, height) as floats |
| `+0x390` | position: the TextImage's `+0x310`, `+0x314` plus its own position |
| `+0x398` | height: `+0x330` times the 16:9 screen's height (surface `+0x1b0`) over 1080 in integers, at least `+0x334`; when 0, the font's tall (surface `+0x2c0`) |
| `+0x39c` | wrap width: the TextImage's width when wrapping, else `FLT_MAX` |
| `+0x3a4` | upper case (flag `0x20` of `+0x50`, Label's `allcaps`) |
| `+0x3a6` | face: `+0x32c` (Label's RUI font), else surface `+0x510`: `rui_defaultFontFace`'s |
| `+0x3a0`, `+0x3a8` | text alignment (0 here) and underline style (0) |

The text goes through `0x4ff4c0` (colour codes, `%+`, line breaks for wrapping) to surface
`+0x110` (`0x545860`), which turns it to UTF-8 and calls the immediate draw (pointer
`0xd410ab0`, `0x720570`) with `drawtext_vgui` (`drawtext_vgui_centered` when `+0x32a`). That
makes an instance of `ui/<name>.rpak` for the frame in the current draw context, whose size is
the viewport's pixels and whose topology a plane over it (`0x720100`), and sets `text`, `face`
(argument type 14, a u16), `pos`, `height`, `lineBreakWidth` (the element's width over the
instance's, times the wrap width), `color`, `rcpActualRes` (the context's reciprocal size),
`allCaps`, `clipRect`, `textAlign`, `underlineStyle` and `altColor1` to `altColor3`.

Label (vtable `0x137b380`; ApplySettings `0x4ad440`, ApplySchemeSettings `0x4acc20`,
PerformLayout `0x4ae010`, Paint `0x4abbc0`, content size `0x4ab3a0`, ComputeAlignment
`0x4ab9f0`) keeps a TextImage (vtable `0x138f1e0`, constructor `0x4fe4c0`, content size
`0x500540`):

- Made (`0x4aaa00`, `0x4ab2c0`): `west` alignment, no insets.
- ApplySettings: `labelText` (a `#TOKEN` localized), `ruiVisible`, `textAlignment` (by name,
  `north-west` 0 to `south-east` 8), `associate`, `dulltext`/`brighttext`, `font`, `xboxfont`,
  `fontHeight` (−1 none; sets the TextImage's `+0x330`), `centerwrap`, `auto_wide_tocontents`,
  `auto_tall_tocontents`, `wrap`, `noshortcutsyntax`, `textinsetx` (scaled with
  `use_proportional_insets`), `textinsety`, `allcaps`.
- ApplySchemeSettings: without a RUI font, `Label.RuiFont` (`DefaultRegularFont` in
  `basemodui_scheme.res`) through surface `+0x500` (`0x544470`: the face's slot, `0x2f5100`,
  else the default face's); without a height, `Label.RuiFontHeight` (26); the colours
  `Label.TextColor` (or Dull/Bright), `Label.DisabledFgColor1`/`2`, `Label.BgColor`; a
  `fgcolor_override` wins. Colours a scheme names come from its `Colors`.
- Content size (TextImage, `0x500540`): the text measured by surface `+0x2f8` (`0x543750`):
  `0x27b2b0` is the size in ems (`0x2f5190`) times the height, truncated. `0x2f5190` walks the
  UTF-8: each code point's advance plus its kerning after the one before, the widest line,
  the lines counted at each newline (one at the end not counted); `%%` a percent, `%...%`
  substituted as in the layout; `` ` `` with a digit skipped; an image its width over its
  height (a nine-slice span its left and right borders); an embedded element its width over
  its height; upper case through `towupper` (`0x11c331c`, `a` to `z` in the C locale).
- `auto_wide_tocontents`/`auto_tall_tocontents` (`0x4ae230`): the Label is sized to its content
  (`0x4ab3a0`): across, the inset plus the text; down, at least the inset plus the text.
- PerformLayout and Paint: the TextImage is the content's width, no wider than the panel less
  the inset; across by the alignment's column (west when the text is wider than the panel),
  down by its row; the inset added on the west, taken off on the east, and added down. The
  TextImage's position and clip are the panel's.
- RuiButton's content size with `behave_as_label` (`0x939100`): the label's text measured in
  its `ruiFont` (`HybridButton.RuiFont`'s face is for the button's own text) at the
  TextImage's height as is (surface `+0x300`, `0x543830`, not upper case), times the button's
  height over `ruiDefaultHeight` (`0x938ee0`).

`0x270130` (sRGB to linear, four lanes): below 0.04045 times 1 / 12.92, else
`((c + 0.055) / 1.055)^2.4` as the cube of the fourth power of a fifth root, which starts from
`0x4c2a6698` less the high half of the float's bits times `0x33333333` and takes three Newton
steps. The fourth lane's threshold is `FLT_MAX` and factor 1, so alpha passes through.

### Scroll panels

`_scroll_panel.gnut`, as a settings tab runs it (`ScrollPanel_InitPanel`,
`ScrollPanel_InitScrollBar`; active while the tab shows):

- The content panel's height becomes its `PanelBottom`'s `y` within it, when above 0; the
  content's and the panel's heights are taken then.
- A move by `value` (`UpdatePanelElements`): `listOffset` becomes `listOffset + value × 10`
  (`SCROLL_SPEED`), clamped to 0 to the content's height less the panel's, then not below 0;
  the content's `y` is minus that (`Hud_SetY` truncates). The scroll bar (`UpdateScrollBar`)
  shows, `handleOffset` the offset over the content's height and `handleFrac` the panel's
  height over the content's; hidden when the panel is as tall as the content or taller.
- The wheel: a notch is a move by 60 / 10, up negative (`MouseWheelUp`, `MouseWheelDown`).
- The handle: the left button going down with the cursor within the scroll bar's rectangle
  (edges included, the bar hidden or not) keeps the cursor's `y` and the offset
  (`ScrollHandlePress`); each frame while it is held, the offset goes back to the kept one and
  moves by the cursor's travel over the bar's height times the content's height (the travel in
  the cursor's 1080 units, the heights in pixels); its release ends it.
- Without the game cursor (`gameCursor_ModeActive` 0, or 2 with a gamepad), each frame scrolls
  the focused panel into view (`ScrollToFocusedPanel`).

### The engine's tab bodies

The Mouse/Keyboard and Video tabs' content is made by code (`InitControlsPCPanelForCode`,
`InitVideoPanelForCode` in the scripts): `CreateKeyBindingPanel` (native `0x7adec0`, maker
`0x786340`) and `CreateVideoOptionsPanel` (`0x7ade40`, `0x7864a0`; it wants a ScriptMenu
ancestor) each make a panel under the given parent with the given name (`ContentPanel`),
load the given file into it (`controls_pc.res`, `video.res` on PC), and make it ready. The
script then puts it at 0, 0, sizes it to its `PanelFrame` and hides it until the tab shows.

- KeyboardBindingsPanel (constructor `0x99f3a0`), a CNestedPanel: makes `listpanel_keybindlist`
  (a SectionedListPanel kind, vtable `0x147c758`) and `ruipanel_keybindlist` (a RuiPanel, which
  `controls_pc.res` then names a RuiButton without changing it), registers `ItemSelected`,
  reads `in_forceuser`, then applies `wide f0`, `tall f0` (`KeyBindingsPanelSettings.res`).
- VideoOptionsPanel (constructor `0x9a9d30`): finds its rows by name and fills their choices
  (labels at `0x14a1498` on; not ported), then applies `xpos 320`, `ypos 64`, `wide f0`, `tall
  f0` (`VideoOptionsPanelSettings.res`, ApplySettings `0x9abe20`).
- `f0` is the screen's size, as neither sets `proportionalToParent`; `PanelFrame` is `%100` of
  that wide (it is `proportionalToParent`) and 836 tall, so the panel ends as wide as the
  screen, clipped by the tab.

### The key bind list

`controls_pc.res`'s `listpanel_keybindlist` is made by KeyboardBindingsPanel's constructor
(`0x99f3a0`) as a VControlsListPanel (vtable `0x147c758`, over SectionedListPanel `0x138c548`);
`scripts/kb_act.lst` (in the guest's install; not in the host's copy until 2026-10-16) fills it
(`0x9a02c0`):

- Each line is two quoted strings, a binding and a description. A `"blank"` binding whose
  description does not start with `=` starts a section (AddSection `0x4e9550`: a CSectionHeader,
  a Label, with the description as its text) with three columns (AddColumnToSection
  `0x4e9c60`, the header localized): `Action` (`Action`, width 286, flags `0x44`), `Key1`
  (`#GameUI_Key1`, 128, `0x2c`) and `Key2` (`#GameUI_Key2`, 128, `0x2c`), the widths scaled when
  the panel is proportional. A `blank` with `=...` adds nothing. Any other line adds an item
  (`0x4ea010`): KeyValues `Item` with `Action` the description, `Binding` the command and the two
  key columns empty; each item's two key columns are marked editable (`+0x38b`, `+0x390`).
- ApplySettings (`0x4e9310`): `linespacing` (20 when missing or 0, scaled), `single_click_edit`
  (`+0x380`), `fontHeight` (`+0x384`, a float), `linePadding` (`+0x27c`, truncated).
- ApplySchemeSettings (`0x9264f0`): background `SectionedListPanel.BgColor`, border
  `ButtonDepressedBorder`, `JoystickDeadZone`, `MaxScrollVel`, font `Default`.
- A header (CSectionHeader vtable `0x138bc78`; ApplySchemeSettings `0x4e62b0`): text
  `SectionedListPanel.HeaderTextColor`, background `SectionedListPanelHeader.BgColor`, the
  divider `SectionedListPanel.DividerColor`; Paint (`0x4e63b0`) draws the Label, then the divider:
  a filled rectangle from (1, height − 2) to (width − 2, height − 1).
- An item (CItemButton vtable `0x138ce58`; ApplySchemeSettings `0x4e7270`): colours
  `SectionedListPanel.TextColor`, `BrightTextColor`, `SelectedTextColor`,
  `OutOfFocusSelectedTextColor`, `SelectedBgColor`, `OutOfFocusSelectedBgColor`,
  `MouseOverBgColor`, `ColumnBgColor`, `BgColor`. PerformLayout (`0x4e6b00`) makes a TextImage per
  column and places the columns left to right, each its width (or its text's), the first at
  6 with its width less 8, a centred one (flag 8) centred, a right-aligned one (`0x10`) at the
  far side, the others its width less 2; a `%name%` text for flag `0x20`. PaintBackground
  (`0x4e74a0`) with `single_click_edit`: each column (unless flag `0x40`) filled with
  `MouseOverBgColor` when it is under the cursor, else `ColumnBgColor`, the columns
  `linePadding` apart; otherwise the whole row by its state.
- LayoutPanels (`0x4e8b50`): a header is the first section header's font tall plus 7 (20
  without one); the rows start at `y` 5 (less the scroll bar's value while it shows), `x` 5,
  as wide as the list less 10 (and less the scroll bar's width while it shows). Per section,
  its header when headers show (`+0x374`), the header's height down; then each item of the
  section `linespacing` tall, `linespacing` plus `linePadding` down; then 8 more. The total is
  the content's height (plus the scroll value).
- KeyBindings_FillInCurrent (native `0x7ac530`, then `0x9a1370`): every item's key columns
  emptied (`0x9a0d50`), then for each button code 0 to `0x134` except `0x73` to `0x134` (the
  gamepad's; the keyboard's and mouse's are below), the bound command's item (`0x9a0870`, the
  first whose `Binding` matches with case folded) gets the key's name (input system `+0xc8`)
  while it has an empty key column (`0x9a0aa0`; the column below).
- The key names (ButtonCodeToString, input system `+0xc8`, `0x39f250`): for codes 1 to `0x72`
  the table at `0x1309708` (`0`..`9`, `a`..`z`, the keypad, punctuation, `ENTER` to `PAUSE`,
  the modifiers, the arrows, `F1`..`F12`, the toggles, `MOUSE1`..`MOUSE5`, `MWHEELUP`,
  `MWHEELDOWN`); StringToButtonCode (`+0xd0`, `0x39f2f0`) compares with case folded. A key's
  bind record is `0x20676ee0 + code × 0x28`: the binding at `+0`, the column it was bound into
  at `+0x25` (the N `settings.cfg` writes).
- The token table also has an entry per key name (`0x23f6a0`), its text the key's display
  string, so a key column's `%w%` draws the key's keycap.
- The fill (`0x9a1370`) puts each key into the column its record names (`0x9a09d0`), whenever
  the item has an empty column, through AddBinding (`0x9a0aa0`) without binding anything.
- AddBinding (`0x9a0aa0`): the key is first taken off every item that shows it but those
  binding the same command (`0x9a1040`); an item left with a Key2 that is another key moves it
  to Key1, bound again. When applying, the column's old key is unbound (`0x9a17e0`: `cmd1
  unbind "KEY"`, `cmd1 unbind_held "KEY"`) and the new one bound (`0x9a1470`: `cmd1 bind "KEY"
  "BINDING" "COLUMN"`, then `cmd1 bind_held "KEY" "HELD"` when the binding is a pair's tapped
  half, else `cmd1 unbind_held`). Every item binding the same command then shows the key in
  the column and not in its other. (It also calls `0x9a1040` with the column's name, `Key1` or
  `Key2`, as the key, which matches nothing.)
- Pairs: `KeyBindings_AddTappedHeldPair` in `panel_controls_pc.nut` (`weaponSelectOrdnance`
  with `+strafe`, `+scriptCommand4` with `+scriptCommand2`), compared byte for byte.
- A mouse press on an item (CItemButton `0x4e7ac0`): the item is selected; with
  `single_click_edit`, on a column (`0x4e7bc0`: within the item's clip rectangle down, within
  a column across, the columns `linePadding` apart; none for flag `0x40`) the left button sends
  the panel Enter, the right button `0x6d`, through the list (`0x4ea930`) to
  KeyboardBindingsPanel (`0x99f9c0`): Enter is `ChangeKey`, `0x6d` is `ClearKey`; then
  `UI_Menu_Accept` on an editable key column (`+0x38b`, bits 1 and 2 of `+0x390`).
- `ChangeKey`, not while capturing: the clicked column less one (Key1 or Key2; nothing for the
  action, or without `single_click_edit` Key1 when empty, else Key2 when empty, else the one
  `LastBoundColumn` is not) is the column to capture into (`+0x3f8`); the item becomes the one
  of interest (`+0x920`), StartCaptureMode (`0x926620`) starts the engine's key trap (`+0x608`),
  and `ruipanel_keybindlist` shows with `isVisible` and `keyName` the item's action. `ClearKey`:
  the column's key unbound and taken off every item.
- OnThink (`0x9a24f0`) while capturing: a key the trap caught (`+0x610`) goes to `0x9a22c0`.
  A gamepad's code is passed over (the trap starts again). Otherwise the capture ends:
  Escape, 0 or −1 play `UI_Menu_Back`; any other key goes into the column (Key1 instead of
  Key2 when Key1 is empty or that same key) through AddBinding, `LastBoundColumn` is set,
  `UI_Menu_Accept` plays; the overlay hides. The scripts' `UICodeCallback_KeyBindSet` and
  `UICodeCallback_KeyBindOverwritten` are called when they exist (not modelled).
- Delete (`0x9a2560`, not while capturing): each key column of the item of interest is cleared
  as `ClearKey` clears one; clearing Key1 moves Key2 up first, so only the first key goes.
- The wheel over the list (`0x4ea7c0`): the scroll bar's value less 60 a notch up. The
  scripts' own wheel handler scrolls the tab too, whatever is under the cursor.
- PaintBackground with `single_click_edit` (`0x4e74a0`): each column without flag `0x40`
  filled, `MouseOverBgColor` under the cursor while not capturing (`UI_Menu_Focus` when that
  column is a new editable one for the item, `+0x38c`), else `ColumnBgColor`. The columns'
  text (PerformLayout `0x4e6b00`): the first at 6, a centred one in its width, the widths
  added without the padding; `%%%s%%` for flag `0x20`; colour BrightTextColor for flag 4, else
  TextColor, Selected or OutOfFocusSelected on the selected item.
- The panel's `OnControlsPCPanel_Show` fills the list in; `OnControlsPCPanel_Hide` runs
  `SavePlayerSettings`.
- Not decompiled yet: the header font's tall at run time (`0x543280` reads the font manager's
  records, on the heap), what the list's `fontHeight` sets, the item text's vertical place, the
  scroll bar, and how `drawtext_vgui` clips its glyphs to `clipRect`.

### SliderControl

BaseModUI's SliderControl (vtable `0x148d608`; constructor `0x94d9b0`, ApplySettings
`0x94e570` at `+0x2f8`, ApplySchemeSettings `0x94ee50` at `+0x2f0`, PerformLayout `0x94f040`,
paint `0x950180`), an EditablePanel.

- A new panel (`0x4c2d50`) is at 0, 0 and 64 × 24, unscaled, with no pin corner (10) and no
  `autoResize`; RuiPanel's (`0x938950`) and SliderControl's constructors keep that size.
- The constructor makes the slider proportional, sets the range 0 to 1, the step 1, the value
  0.5 and the inset 0, and loads `Resource/UI/BaseModUI/SliderControl.res` (the install's
  `resource/ui/basemodui/slidercontrol.res`) into its children: `BtnDropButton`
  (`ui/settings_base_button`, `autoResize` 1, `pinCorner` 0), `LblSliderText`
  (`ui/settings_slider_label`, pinned right to right of `BtnDropButton`), `PrgValue`
  (`ui/settings_slider`) and `PnlDefaultMark` (`ui/settings_slider_default`), all RuiPanels.
- ApplySettings loads that file again, then Panel's ApplySettings, then the slider's own
  blocks through its build group (so `SldOpenMicSensitivity`'s `PrgValue` block, with
  `ui/settings_voice_slider`, wins). It finds the four children by name, reads `inverseFill` and
  `isValueClampedToStepSize` (each true when 1), `stepSize` (1), `minValue` (0), `maxValue`
  (100), putting the value back through SetValue, forced, after each bound; then
  `playlistVarName`, `gamemodeIdx`, `conCommand` (its ConVar; when it exists, SetValue of its
  float, forced), `conCommandDefault`, `showLabel` (`LblSliderText` shown when not 0, its
  `buttonText` emptied when it is a RuiPanel, its enabled state the slider's). Then the slider
  takes `BtnDropButton`'s height, its `y` moved up by half the difference (truncated), and the
  layout below runs.
- Layout (`0x94e240`, also at the end of `conCommand`'s setter and of PerformLayout):
  `PrgValue` at `x` = the slider's width + the inset − its width, `y` = half the slider's
  height − half its own (each halved as integers). `PnlDefaultMark` at
  `(int)(bar width × f + bar x − (int)(mark width / 2))` in f32, `y` 0, where `f` is the
  default of `conCommandDefault`'s ConVar, else of `conCommand`'s, else the maximum, as
  `(v − min) / (max − min)` clamped to 0 to 1 (a NaN to 0), then `1 − f` with `inverseFill`.
  Both are shown when the slider is enabled. Then the label's text.
- ApplySchemeSettings: the colours `SliderControl.MarkColor`, `MarkFocusColor`,
  `ForegroundColor`, `ForegroundFocusColor`, `BackgroundColor`, `BackgroundFocusColor` (for
  the mark and the bar), the inset `SliderControl.InsetX` (`atoi`, then scaled: −68 in
  `basemodui_scheme.res`'s `BaseSettings`) and `SliderControl.CursorPriority`.
- PerformLayout: EditablePanel's (`0x49ebc0`), which places each child with a pin corner or
  `autoResize` (below); dragging enabled (`+0x4b8`) without its helper; the layout above.
- AutoResize (`0x4c9d50`, from Panel's ApplySettings): `AutoResize` (0) and `PinCorner`
  (10). From the panel's position and size and its parent's size at that moment: the pinned
  offset is the panel's position, the unpinned one its far edge less the parent's size, per
  axis by the corner (0 top left, 1 top right, 2 bottom left, 3 bottom right);
  `PinnedCornerOffsetX`/`Y` and `UnpinnedCornerOffsetX`/`Y` replace them (scaled when
  proportional). EditablePanel's PerformLayout then puts such a child at its pinned offsets,
  with its far edge the parent's size plus the unpinned offset on each axis it resizes (1
  right, 2 down, 3 both), else its own size. `BtnDropButton`'s offsets are taken while it and
  the slider are both 64 wide, so it spans the slider.
- SetValue (`0x94dd90`): with `isValueClampedToStepSize` and a step, the value becomes
  `(float)(int)((double)(v / step) + 0.5) × step`; then `v < max ? v : max`, and the minimum
  unless that is above it, where `max <= v` gives the maximum. Not forced, the same value
  returns at once, and a new one plays `UI_Menu_Accept` unless the slider is being dragged.
  The value is kept (a playlist variable is set instead when `playlistVarName` names one),
  then `PrgValue`'s `progress` is `(value − min) / (max − min)` (`1 −` that with `inverseFill`),
  the ConVar's SetValue takes the value as a float, the label is set, and a change is
  signalled.
- The label (`0x94ff80`), by `showLabel`: 1 `%d` of the ConVar's int, 2 `%.1f` of its float,
  3 `%d%%` of `(int)(float × 100)` in f32; without a ConVar, the number handed in (the
  fraction from SetValue, the value from the layout). Any other: empty. It becomes
  `LblSliderText`'s `buttonText`.
- Paint: when the ConVar's float differs from the value, SetValue of it, forced; so a ConVar
  out of the range is put back into it while the slider paints.
- The cursor entering (`0x94fc10`) highlights (`0x94fc60`): `BtnDropButton` navigated to
  (it takes the focus, `0x4d3460`; with a gamepad active, RequestFocus instead) with
  `isFocused` set on its instance; the mark's and the bar's focus colours; `isFocused` on
  `PrgValue`'s instance. Leaving (`0x94fc30`) undoes it (`0x94fe00`). Losing the focus
  (`0x94f9b0`) does too, unless dragging. Taking it (`0x94f870`) plays `UI_Menu_Focus`.
- The mouse: a press on the slider reaches its drag start (`+0x670`, `0x94f600`) through
  Panel's press (`0x4d2db0`), as dragging is enabled; it takes the cursor's moves from then on
  (`0x17386c8`, which `0x4c5290` sends to `+0x678`, `0x94f6f0`). Both set the value from the
  cursor: when dragging already, or with the cursor within `PrgValue`'s absolute rectangle
  (edges included) and the bar wider than 0, `f` = `(cursor x − bar x) / bar width` clamped to
  0 to 1 (`1 − f` with `inverseFill`), SetValue of `(max − min) × f + min`, not forced; then
  dragging. The release (`+0x680`, `0x94f710`) ends the drag, and when the cursor is off the
  bar, the slider loses the focus.
- The arrow keys (`0x94f160`): left and right step the value by `stepSize` (the other way with
  `inverseFill`), up and down navigate, each playing `Menu.Focus`; not ported.
- SetEnabled (`0x94dd00`): `BtnDropButton` and `LblSliderText` take it, the bar and the mark
  show only when enabled.

