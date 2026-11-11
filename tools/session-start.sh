#!/bin/sh
# SessionStart hook: unpack the oracle image if a compressed one is there, and say what this
# session cannot do yet.
# Its output reaches Claude, so it stays silent when all is well.
cd "$(dirname "$0")/.." || exit 0

unpack() {
    if command -v zstd >/dev/null 2>&1; then
        zstd -dqf --long=27 dumps/current.bin.zst -o dumps/current.bin.part
        return
    fi
    python3 -c 'import zstandard' 2>/dev/null \
        || python3 -m pip install -q zstandard >/dev/null 2>&1 \
        || python3 -m pip install -q --break-system-packages zstandard >/dev/null 2>&1
    python3 - <<'EOF'
import zstandard
with open('dumps/current.bin.zst', 'rb') as f, open('dumps/current.bin.part', 'wb') as o:
    zstandard.ZstdDecompressor(max_window_size=1 << 27).copy_stream(f, o)
EOF
}

if [ ! -f dumps/current.bin ] && [ ! -f dumps/current.bin.zst ]; then
    echo "lurch: no oracle image at dumps/current.bin, so tools/dec.sh cannot decompile (docs/setup.md)"
elif [ ! -f dumps/current.bin ]; then
    if unpack 2>/dev/null; then
        mv dumps/current.bin.part dumps/current.bin
    else
        rm -f dumps/current.bin.part
        echo "lurch: could not unpack dumps/current.bin.zst (no zstd, and no Python zstandard)"
    fi
fi

GHIDRA=${GHIDRA:-/opt/ghidra}
if [ ! -x "$GHIDRA/support/analyzeHeadless" ]; then
    echo "lurch: no Ghidra at $GHIDRA, so tools/dec.sh cannot decompile; tools/cloud-setup.sh installs it (docs/setup.md)"
fi
exit 0
