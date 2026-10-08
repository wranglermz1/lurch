#!/usr/bin/env python3
"""Print the animation tables the oracle builds at startup: activities, animation events and
activity modifiers, each with the number the code uses.

  tools/tables.py dumps/current.bin [activities|events|modifiers]

Activities and events are registered by one straight-line function each: a string, a number,
a call, over and over. The function is found by the first string it registers (ACT_RESET,
AE_EMPTY), and its registration calls are read by their few instruction forms. Modifiers are a
pointer array, found by its first entries.
"""
import mmap, re, struct, sys

def sections(m):
    nt = struct.unpack_from('<I', m, 0x3c)[0]
    n, optsz = struct.unpack_from('<HH', m, nt + 6)[0], struct.unpack_from('<H', m, nt + 20)[0]
    out = {}
    for i in range(n):
        o = nt + 24 + optsz + 40 * i
        out[m[o:o + 8].rstrip(b'\0').decode('latin1')] = struct.unpack_from('<II', m, o + 8)[::-1]
    return out

def cstr(m, a):
    return m[a:a + 96].split(b'\0')[0].decode('latin1')

def function_of(m, pdata, rva):
    va, size = pdata
    for o in range(va, va + size, 12):
        b, e, _ = struct.unpack_from('<III', m, o)
        if b <= rva < e:
            return b, e
    raise SystemExit(f'no function at {rva:#x}')

def lea_rcx_refs(m, text, target):
    va, size = text
    out = []
    for x in re.finditer(rb'\x48\x8d\x0d', m[va:va + size]):
        at = va + x.start()
        if at + 7 + struct.unpack_from('<i', m, at + 3)[0] == target:
            out.append(at)
    return out

def registrations(m, begin, end):
    """(number, name) for each registration call between begin and end."""
    edx = r8 = rcx = None
    out, i = [], begin
    s8 = lambda a: struct.unpack_from('<b', m, a)[0]
    while i < end:
        b = m[i:i + 4]
        if b[:3] == b'\x48\x8d\x0d':
            rcx = i + 7 + struct.unpack_from('<i', m, i + 3)[0]; i += 7
        elif b[:1] == b'\xba':
            edx = struct.unpack_from('<I', m, i + 1)[0]; i += 5
        elif b[:2] == b'\x33\xd2':
            edx = 0; i += 2
        elif b[:3] == b'\x45\x33\xc0':
            r8 = 0; i += 3
        elif b[:2] == b'\x41\xb8':
            r8 = struct.unpack_from('<I', m, i + 2)[0]; i += 6
        elif b[:3] == b'\x44\x8b\xc2':
            r8 = edx; i += 3
        elif b[:3] == b'\x41\x8d\x50':
            edx = r8 + s8(i + 3); i += 4
        elif b[:3] == b'\x44\x8d\x42':
            r8 = edx + s8(i + 3); i += 4
        elif b[:1] in (b'\xe8', b'\xe9'):
            out.append((edx, cstr(m, rcx))); i += 5
        elif b[:3] in (b'\x48\x83\xec', b'\x48\x83\xc4'):
            i += 4
        elif b[:1] == b'\xc3':
            break
        else:
            raise SystemExit(f'unexpected instruction at {i:#x}: {b.hex()}')
    return out

def table(m, sec, first):
    hit = m.find(b'\0' + first + b'\0', sec['.rdata'][0])
    refs = lea_rcx_refs(m, sec['.text'], hit + 1)
    begin, end = function_of(m, sec['.pdata'], refs[0])
    return registrations(m, begin, end)

def modifiers(m, sec):
    rd = sec['.rdata']
    names = [m.find(b'\0' + s + b'\0', rd[0]) + 1 for s in (b'ads', b'crouch', b'movingforward')]
    for x in re.finditer(rb'(?s).{8}', m[rd[0]:rd[0] + rd[1]]):
        at = rd[0] + x.start()
        q = [struct.unpack_from('<Q', m, at + 8 * k)[0] for k in range(3)]
        base = q[0] - names[0]
        if base & 0xffff == 0 and all(q[k] - names[k] == base for k in range(3)):
            out = []
            while (p := struct.unpack_from('<Q', m, at + 8 * len(out))[0] - base) and rd[0] <= p < rd[0] + rd[1]:
                out.append((len(out), cstr(m, p)))
            return out
    raise SystemExit('modifier names not found')

def main(path, which):
    f = open(path, 'rb')
    m = mmap.mmap(f.fileno(), 0, prot=mmap.PROT_READ)
    sec = sections(m)
    get = {
        'activities': ('activity', lambda: table(m, sec, b'ACT_RESET')),
        'events': ('event', lambda: table(m, sec, b'AE_EMPTY')),
        'modifiers': ('modifier', lambda: modifiers(m, sec)),
    }
    for w in which or get:
        label, rows = get[w]
        for n, name in rows():
            print(f'{label} {n} {name}')

if __name__ == '__main__':
    main(sys.argv[1], sys.argv[2:])
