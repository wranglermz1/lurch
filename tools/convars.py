#!/usr/bin/env python3
"""List the ConVars registered in the oracle image, or the named ones.

  tools/convars.py dumps/current.bin [NAME...]

One line each: name, default, the live value in the dump, flags, min and max when set.
A ConVar is found by its vtable (0x136be00, the one sv_gravity's object holds); the object
holds its name at +0x18, flags at +0x38, the default string at +0x50, the live string at
+0x58 (heap, not in the dump), float +0x68, int +0x6c, then has-min, min, has-max, max at +0x70 to +0x7c.
"""
import mmap, struct, sys

BASE = 0x7ff7b6a90000
VTABLE = 0x136be00


def main(dump, names):
    f = open(dump, 'rb')
    m = mmap.mmap(f.fileno(), 0, prot=mmap.PROT_READ)
    q = lambda a: struct.unpack_from('<Q', m, a)[0]

    def cstr(p):
        a = p - BASE
        return m[a:a + 512].split(b'\0')[0].decode('latin-1') if 0 < a < len(m) else None

    vt = struct.pack('<Q', BASE + VTABLE)
    rows = {}
    at = 0
    while (at := m.find(vt, at)) >= 0:
        o, at = at, at + 8
        if o % 8:
            continue
        name = cstr(q(o + 0x18))
        if not name:
            continue
        flags = struct.unpack_from('<i', m, o + 0x38)[0]
        has_min, lo, has_max, hi = struct.unpack_from('<?3xf?3xf', m, o + 0x70)
        fv, iv = struct.unpack_from('<fi', m, o + 0x68)
        rng = (f' min {lo:g}' if has_min else '') + (f' max {hi:g}' if has_max else '')
        rows[name] = f'{name} "{cstr(q(o + 0x50))}" live {fv:g}/{iv} flags {flags:#x}{rng}  @{o:#x}'
    for n in names or sorted(rows, key=str.lower):
        print(rows.get(n, f'{n} not in the image'))


if __name__ == '__main__':
    main(sys.argv[1], sys.argv[2:])
