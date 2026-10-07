# Map collision for R5Reloaded maps

HAVM's `maps/*.mesh` are retail maps. None of them is a map the oracle runs, so they
cannot verify collision against recordings. R5Reloaded maps are extracted from its own
install. Done so far: `mp_rr_desertlands_64k_x_64k` (Season 3 World's Edge), world
geometry only.

1. On the guest, R5Reloaded is at `C:\Program Files\R5Reloaded\R5R Library\LIVE`. Unpack
   the server pak with the SDK's own tool (HAVM's LZHAM path fails on these with status 7):

       bin\revpk.exe -unpack vpk\englishserver_<map>.bsp.pak000_dir.vpk C:\xfer\lurch\<map>\

2. Copy `maps/<map>.bsp*` to `/home/fz/HAVM/maptools/maps/<map>/r2/maps/` (scp, about 2 MB/s).
3. revpk names lumps in lowercase hex (`.000e.bsp_lump`). HAVM's `raw_lump` looks for
   uppercase (`.000E.bsp_lump`) and otherwise falls back to the .bsp header, which yields
   garbage rather than an error. Uppercase the four hex digits; keep `.bsp_lump` lowercase.
4. From HAVM, with its maptools venv:

       tools/mapdump.py mesh /home/fz/HAVM/maptools/maps/<map> maps/<map>.mesh

   `maps/` here is gitignored; `World::open` reads the result.

Not in the mesh yet: static props (buildings, rocks, curbs). Their collision is in the
models inside the Season 3 rpaks, which HAVM's `props` command does not read.

The engine leaves a hull about 0.014 units closer to triangle leaves than the 1/32 skin
`src/world.rs` uses; 85% of street traces at Fragment still match bit for bit.
