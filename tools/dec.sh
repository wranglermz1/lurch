#!/bin/sh
# Decompile functions of the oracle image by RVA.
#
#   tools/dec.sh OUT.c 0x8221b0 0x81a080 ...
#
# Ghidra headless over .text+.rdata of dumps/current.bin, loaded raw at base 0 so
# addresses read as RVAs. No auto-analysis: each RVA becomes a function on demand,
# which keeps a run under a minute. Globals in .data show up as DAT_ addresses.
set -e
GHIDRA=${GHIDRA:-/opt/ghidra}
HOST_CPUS=${HOST_CPUS:-12-17}
taskset -c "$HOST_CPUS" true 2>/dev/null || HOST_CPUS=0-$(($(nproc) - 1))
ROOT=$(cd "$(dirname "$0")/.." && pwd)
CACHE=${CACHE:-$ROOT/ghidra-cache}
OUT=$(realpath -m "$1"); shift
mkdir -p "$CACHE"
if [ -d "$CACHE/proj.rep" ]; then
    SRC="-process slice.bin -readOnly"
else
    head -c $((0x1710000)) "$ROOT/dumps/current.bin" > "$CACHE/slice.bin"
    SRC="-import $CACHE/slice.bin -loader BinaryLoader -loader-baseAddr 0 -processor x86:LE:64:default -cspec windows"
fi
MAXMEM=6G taskset -c "$HOST_CPUS" "$GHIDRA/support/analyzeHeadless" "$CACHE" proj $SRC \
    -noanalysis -scriptPath "$ROOT/tools/ghidra" -postScript Dec.java "$OUT" "$@" >"$CACHE/last.log" 2>&1 \
    || { tail -20 "$CACHE/last.log" >&2; exit 1; }
grep -c '^// ====' "$OUT"
