#!/bin/sh
# dec.sh without a Ghidra install: Ghidra's own C++ decompiler, built from source.
#
#   tools/decomp.sh OUT.c 0x8221b0 0x81a080 ...
#
# For a session that can clone GitHub but not download Ghidra releases. The first run
# sparse-clones Ghidra (decompiler and x86 specs only, about 20 MB) into $GHIDRA_SRC,
# builds decomp_opt and the x86-64 SLEIGH spec (needs g++, bison, flex, binutils-dev),
# and caches them. Output matches dec.sh; read it through tools/annotate.py.
set -e
ROOT=$(cd "$(dirname "$0")/.." && pwd)
SRC=${GHIDRA_SRC:-$ROOT/ghidra-cache/src}
CACHE=${CACHE:-$ROOT/ghidra-cache}
CPP=$SRC/Ghidra/Features/Decompiler/src/decompile/cpp
LANG=$SRC/Ghidra/Processors/x86/data/languages
OUT=$(realpath -m "$1"); shift
mkdir -p "$CACHE"
if [ ! -d "$SRC/.git" ]; then
    git clone -q --depth 1 --filter=blob:none --sparse https://github.com/NationalSecurityAgency/ghidra "$SRC"
    git -C "$SRC" sparse-checkout set Ghidra/Features/Decompiler/src/decompile Ghidra/Processors/x86/data
fi
if [ ! -x "$CPP/decomp_opt" ]; then
    mkdir -p "$CPP/com_opt" "$CPP/sla_opt"
    make -C "$CPP" -j"$(nproc)" decomp_opt sleigh_opt >"$CACHE/build.log" 2>&1 || { tail -20 "$CACHE/build.log" >&2; exit 1; }
fi
[ -f "$LANG/x86-64.sla" ] || (cd "$LANG" && "$CPP/sleigh_opt" x86-64.slaspec x86-64.sla >/dev/null 2>&1)
[ -f "$CACHE/slice.bin" ] || head -c $((0x1710000)) "$ROOT/dumps/current.bin" > "$CACHE/slice.bin"
{
    echo "load file binary-x86:LE:64:default:windows $CACHE/slice.bin"
    echo "option readonly on"
    echo "readonly [ram, 0x0, $((0x1710000))]"
    for a in "$@"; do
        echo "load addr $a"
        echo "echo // ==== $a"
        echo "decompile"
        echo "print C"
    done
    echo quit
} | SLEIGHHOME=$SRC "$CPP/decomp_opt" 2>&1 | grep -v '^\[decomp\]>\|^Decompil\|^Function func_\|successfully loaded\|now propagate\|marked range' >"$OUT"
grep -c '^// ====' "$OUT"
