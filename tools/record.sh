#!/bin/sh
# Record one scripted scenario: drain the ring while drive plays the keys.
#
#   tools/record.sh NAME STEP...      writes traces/NAME.ltr; steps as for capture/drive
#   tools/record.sh walk -:0.5 forward:2 -:1.5
set -e
NAME=$1; shift
ROOT=$(cd "$(dirname "$0")/.." && pwd)
HOST_CPUS=${HOST_CPUS:-12-17}
taskset -c "$HOST_CPUS" "$ROOT/capture/drain" "$ROOT/traces/$NAME.ltr" --map canyonlands &
DRAIN=$!
sleep 0.5
taskset -c "$HOST_CPUS" "$ROOT/capture/drive" "$@" || true
sleep 0.3
kill -INT $DRAIN
wait $DRAIN
