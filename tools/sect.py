#!/usr/bin/env python3
"""Is a dump complete and in the clear?

Per section: how many pages carry data, and the Shannon entropy of a sample of
them. Code sits near 6.2; a packed or still-encrypted executable section runs
above 7.3. High entropy in .rsrc is compressed art and means nothing.

  tools/sect.py dumps/current.bin
"""
import collections, math, struct, sys

def sections(m):
    nt = struct.unpack_from('<I', m, 0x3c)[0]
    assert m[nt:nt + 4] == b'PE\0\0', 'not a PE image dump'
    n, optsz = struct.unpack_from('<H', m, nt + 6)[0], struct.unpack_from('<H', m, nt + 20)[0]
    for i in range(n):
        o = nt + 24 + optsz + 40 * i
        name = m[o:o + 8].rstrip(b'\0').decode('latin1')
        vsize, va = struct.unpack_from('<II', m, o + 8)
        yield name, va, vsize, bool(struct.unpack_from('<I', m, o + 36)[0] & 0x20000000)

def entropy(b):
    c = collections.Counter(b)
    return -sum((v / len(b)) * math.log2(v / len(b)) for v in c.values())

def main(path):
    f = open(path, 'rb')
    head = f.read(0x1000)
    print(f'{"section":<10} {"va":>10} {"vsize":>10} {"pages":>14} {"ent":>6} {"max":>6}  flag')
    for name, va, vsize, exe in sections(head):
        npages, live, ents = (vsize + 4095) // 4096, 0, []
        for i in range(npages):
            f.seek(va + i * 4096)
            page = f.read(4096)
            if not any(page):
                continue
            live += 1
            if live % 64 == 1:
                ents.append(entropy(page))
        ents = ents or [0.0]
        flag = 'CIPHER?' if exe and max(ents) >= 7.3 else ''
        print(f'{name:<10} {va:>#10x} {vsize:>#10x} {live:>6}/{npages:<7} '
              f'{sum(ents) / len(ents):>6.2f} {max(ents):>6.2f}  {flag}')

if __name__ == '__main__':
    main(sys.argv[1] if len(sys.argv) > 1 else 'dumps/current.bin')
