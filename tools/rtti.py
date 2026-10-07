#!/usr/bin/env python3
"""Find a class's vtable in a dump by its RTTI name.

The R5Reloaded build keeps RTTI, so a class is addressable by name instead of by
byte pattern. Chain: the mangled name sits 16 bytes into a TypeDescriptor, a
RTTICompleteObjectLocator points at that descriptor by RVA, and the qword before
a vtable points at that locator. Retail strips all of this.

  tools/rtti.py dumps/current.bin                     list movement class names
  tools/rtti.py dumps/current.bin CGameMovement ...   resolve vtables

The image base is recovered from the locator pointers themselves, since a dump
holds live VAs and the header's ImageBase is only a preference.
"""
import collections, mmap, re, struct, sys

MOVEMENT = r'move|player|traversal|wallrun|climb|slide|physics|usercmd|jump|mantle'
VTABLE_CAP = 64

def sections(m):
    nt = struct.unpack_from('<I', m, 0x3c)[0]
    n, optsz = struct.unpack_from('<H', m, nt + 6)[0], struct.unpack_from('<H', m, nt + 20)[0]
    out = {}
    for i in range(n):
        o = nt + 24 + optsz + 40 * i
        name = m[o:o + 8].rstrip(b'\0').decode('latin1')
        vsize, va = struct.unpack_from('<II', m, o + 8)
        out[name] = (va, vsize)
    return out

def find_base(m, rd, col_rva):
    """A vtable slot holds base+col_rva. Recover base from whichever candidate wins."""
    lo, hi = rd
    votes = collections.Counter()
    for off in range(lo, lo + hi - 8, 8):
        q = struct.unpack_from('<Q', m, off)[0]
        cand = q - col_rva
        if cand > 0 and cand % 0x10000 == 0 and 0x7f0000000000 <= cand < 0x800000000000:
            votes[cand] += 1
    return votes.most_common(1)[0][0] if votes else 0

def locators(m, rd, name):
    """Every COL for this class name, by RVA."""
    for hit in re.finditer(re.escape(('.?AV' + name + '@@').encode()), m):
        td = hit.start() - 16
        for c in re.finditer(re.escape(struct.pack('<I', td)), m[rd[0]:rd[0] + rd[1]]):
            col = rd[0] + c.start() - 12
            if col < rd[0]:
                continue
            sig, _off, _cd, ptd, _pcd = struct.unpack_from('<IIIII', m, col)
            if sig == 1 and ptd == td:
                yield col

def vtables(m, rd, text, base, col):
    tlo, thi = base + text[0], base + text[0] + text[1]
    for v in re.finditer(re.escape(struct.pack('<Q', base + col)), m[rd[0]:rd[0] + rd[1]]):
        vt = rd[0] + v.start() + 8
        slots = []
        while len(slots) < VTABLE_CAP:
            q = struct.unpack_from('<Q', m, vt + 8 * len(slots))[0]
            if not tlo <= q < thi:
                break
            slots.append(q - base)
        if slots:
            yield vt, slots

def main(path, names):
    f = open(path, 'rb')
    m = mmap.mmap(f.fileno(), 0, prot=mmap.PROT_READ)
    sec = sections(m)
    rd, text = sec['.rdata'], sec['.text']
    if not names:
        found = sorted({h.group(1).decode() for h in re.finditer(rb'\.\?AV([A-Za-z0-9_]{3,50})@@', m)})
        hits = [n for n in found if re.search(MOVEMENT, n, re.I)]
        print(f'{len(found)} RTTI names, {len(hits)} movement-related:')
        for n in hits:
            print(' ', n)
        return
    base = 0
    for name in names:
        cols = list(locators(m, rd, name))
        if not cols:
            print(f'{name}: no RTTI locator'); continue
        base = base or find_base(m, rd, cols[0])
        if not base:
            print('could not recover image base'); return
        for col in cols:
            for vt, slots in vtables(m, rd, text, base, col):
                cap = ' (hit cap, unterminated)' if len(slots) == VTABLE_CAP else ''
                print(f'{name:<28} base={base:#x} col={col:#x} vtable={vt:#x} '
                      f'vfuncs={len(slots)}{cap}')
                for i, s in enumerate(slots):
                    print(f'  [{i:2}] {s:#010x}')

if __name__ == '__main__':
    main(sys.argv[1] if len(sys.argv) > 1 else 'dumps/current.bin', sys.argv[2:])
