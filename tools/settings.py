#!/usr/bin/env python3
"""Name the player-settings offset globals in a dump.

Code reads a settings field as *(settings + *(u32 *)global). The registry in .data
holds triples (type<<32 | field offset, name ptr, global ptr), but the global in a
triple is not the one for that name. A global's name is the record whose field
offset equals the offset the global holds live. Verified on hull_min/hull_max
(SetupMove takes their difference) and speed/sprintspeed (picked by the sprint flag).
Most records sit in LO..HI; the rest (the grapple, zipline and cockpit sway fields
among them) from REG_LO up. Uppercase names there are other registries' records.

  tools/settings.py dumps/current.bin               every global, its offset, its name
  tools/settings.py dumps/current.bin 0x2385c68 ... just these globals
"""
import collections, mmap, struct, sys

LO, HI = 0x2380000, 0x2390000
REG_LO = 0x2300000
TYPES = {0: 'bool', 1: 'int', 2: 'float', 4: 'vec3', 5: 'string'}

def main(path, want):
    f = open(path, 'rb')
    m = mmap.mmap(f.fileno(), 0, prot=mmap.PROT_READ)
    q = lambda a: struct.unpack_from('<Q', m, a)[0]
    votes = collections.Counter()
    for a in range(LO, HI, 8):
        v = q(a)
        if v >> 40 == 0x7f:
            votes.update(range((v - HI + 0xffff) & ~0xffff, v - LO + 1, 0x10000))
    base = votes.most_common(1)[0][0]

    def cstr(va):
        a = va - base
        if not 0 < a < len(m) - 80:
            return None
        s = m[a:a + 80].split(b'\0')[0]
        return s.decode() if s and all(32 <= c < 127 for c in s) else None

    names, globs = collections.defaultdict(list), set()
    for a in range(REG_LO, HI, 8):
        meta, name, g = q(a), q(a + 8), q(a + 16) - base
        if REG_LO <= g < HI and meta >> 32 < 16 and (n := cstr(name)) and not n[0].isupper():
            names[meta & 0xffffffff].append(f'{n}:{TYPES.get(meta >> 32, meta >> 32)}')
            globs.add(g)
    for g in sorted(want or globs):
        off = struct.unpack_from('<I', m, g)[0]
        print(f'{g:#x} {off:#x} {"/".join(names.get(off, ["?"]))}')

if __name__ == '__main__':
    main(sys.argv[1], [int(x, 0) for x in sys.argv[2:]])
