#!/usr/bin/env python3
"""Lay a PE file out as it sits in memory, so its addresses read as RVAs.

  tools/peimage.py INSTALL/paks/Win64/ui.dll OUT.img

Each section is copied to its virtual address; the gaps are zero. tools/dec.sh then
decompiles the result the way it decompiles dumps/current.bin:

  IMAGE=OUT.img SLICE=$(stat -c%s OUT.img) CACHE=ghidra-cache/ui tools/dec.sh OUT.c RVA...
"""
import struct, sys

def main(src, out):
    d = open(src, 'rb').read()
    nt = struct.unpack_from('<I', d, 0x3c)[0]
    assert d[nt:nt + 4] == b'PE\0\0', 'not a PE file'
    nsec, optsz = struct.unpack_from('<H', d, nt + 6)[0], struct.unpack_from('<H', d, nt + 20)[0]
    size_of_image, size_of_headers = struct.unpack_from('<II', d, nt + 24 + 56)
    img = bytearray(size_of_image)
    img[:size_of_headers] = d[:size_of_headers]
    for i in range(nsec):
        o = nt + 24 + optsz + 40 * i
        vsz, va, rsz, ra = struct.unpack_from('<IIII', d, o + 8)
        n = min(vsz or rsz, rsz)
        img[va:va + n] = d[ra:ra + n]
    open(out, 'wb').write(img)

if __name__ == '__main__':
    main(*sys.argv[1:3])
