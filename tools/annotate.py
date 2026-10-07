#!/usr/bin/env python3
"""Make tools/dec.sh output readable: name what the oracle's globals are.

  tools/annotate.py dumps/current.bin IN.c > OUT.c

Rewrites, in place in the text:
  *(float *)(_DAT_X + 0x68)    ConVar float value  -> cv_<name>
  *(int *)(_DAT_X + 0x6c)      ConVar int value    -> cvi_<name>
  _DAT_X  (settings offset)    -> S_<name>          (names from tools/settings.py)
  _DAT_X  (float in .rdata)    -> its value
  0xNNNNNN that points at a C string in .rdata -> /*"string"*/ appended
"""
import mmap, re, struct, sys, os

sys.path.insert(0, os.path.dirname(__file__))
from settings import LO, HI

def main(dump, src):
    f = open(dump, 'rb')
    m = mmap.mmap(f.fileno(), 0, prot=mmap.PROT_READ)
    q = lambda a: struct.unpack_from('<Q', m, a)[0]
    u = lambda a: struct.unpack_from('<I', m, a)[0]
    base = 0x7ff7b6a90000
    nt = u(0x3c)
    sizes = q(nt + 24 + 56) & 0xffffffff

    def cstr(a, n=96):
        if not 0 < a < sizes - n:
            return None
        s = m[a:a + n].split(b'\0')[0]
        return s.decode() if len(s) >= 3 and all(32 <= c < 127 for c in s) else None

    def convar(g):
        p = q(g) - base
        if not 0 < p < sizes:
            return None
        n = q(p + 0x18) - base
        return cstr(n) if 0 < n < sizes else None

    import subprocess
    names = {}
    out = subprocess.run([sys.executable, os.path.join(os.path.dirname(__file__), 'settings.py'), dump],
                         capture_output=True, text=True).stdout
    for line in out.splitlines():
        g, off, n = line.split(maxsplit=2)
        if n != '?':
            names[int(g, 16)] = n.split(':')[0].split('/')[0]

    text = open(src).read()

    def cv(mo):
        g = int(mo.group(2), 16)
        n = convar(g)
        if not n:
            return mo.group(0)
        return ('cv_' if mo.group(3) == '68' else 'cvi_') + n

    text = re.sub(r'\*\((float|int|uint|undefined4) \*\)\(_?DAT_([0-9a-f]{8}) \+ 0x(68|6c)\)', cv, text)
    text = re.sub(r'\*\((?:float|int|uint) \*\)\(lRam0*([0-9a-f]{7,8}) \+ 0x(68|6c)\)',
                  lambda mo: (('cv_' if mo.group(2) == '68' else 'cvi_') + (convar(int(mo.group(1), 16)) or mo.group(1))), text)

    def dat(mo):
        a = int(mo.group(1), 16)
        if a in names:
            return 'S_' + names[a]
        if 0x1296000 <= a < 0x1710000:
            f = struct.unpack_from('<f', m, a)[0]
            d = struct.unpack_from('<d', m, a)[0]
            if (f == 0 and d != 0 and 1e-9 < abs(d) < 1e12) or not (f == 0 or 1e-9 < abs(f) < 1e9):
                return repr(d)
            return repr(f)
        return mo.group(0)

    text = re.sub(r'\(ulonglong\)_DAT_([0-9a-f]{8})', lambda mo: dat(mo), text)
    text = re.sub(r'_DAT_([0-9a-f]{8})(?![0-9a-f])', dat, text)

    def s(mo):
        a = int(mo.group(1), 16)
        st = cstr(a) if 0x1296000 <= a < 0x1710000 else None
        return mo.group(0) + (f'/*"{st}"*/' if st else '')

    text = re.sub(r'(?<![\w+])0x([0-9a-f]{7})\b', s, text)
    sys.stdout.write(text)

if __name__ == '__main__':
    main(sys.argv[1], sys.argv[2])
