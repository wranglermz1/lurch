#!/bin/sh
# Dump the running game image into dumps/current.bin.
#
# Reuses HAVM's accumulating dumper: a page already held is never read again, so
# the first pass costs the whole image and later passes cost only the holes. On a
# build change the old image is archived under dumps/archive/ instead of being
# overwritten. --no-parse keeps it from touching HAVM's own offset files.
set -e
HAVM=${HAVM:-/home/fz/HAVM/HAVM}
PROC=${PROC:-r5apex.exe}
HAVM_DOMAIN=${HAVM_DOMAIN:-analysis1}
HOST_CPUS=${HOST_CPUS:-12-17}
OUT=$(cd "$(dirname "$0")/.." && pwd)/dumps
export HAVM_DOMAIN
[ -x "$HAVM/bin/redump" ] || { echo "no $HAVM/bin/redump -- build it there first" >&2; exit 1; }
cd "$HAVM"
exec taskset -c "$HOST_CPUS" ./bin/redump --proc "$PROC" \
    --dump "$OUT/current.bin" --dir "$OUT/archive" --no-parse "$@"
