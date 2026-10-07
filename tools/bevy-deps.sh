#!/bin/sh
# tools/bevy-deps.sh: system packages to build and screenshot game/ in a cloud session.
set -e
test -d /root/.ccr || { echo "bevy-deps.sh: cloud container only; on the workstation see docs/game-shell-plan.md section 9"; exit 1; }
PKGS="g++ pkg-config libx11-dev libwayland-dev libxkbcommon-dev libxkbcommon-x11-0 libxcursor1 libxrandr2 libxi6 libx11-xcb1 libvulkan1 mesa-vulkan-drivers xvfb xauth"
dpkg -s $PKGS >/dev/null 2>&1 && exit 0
apt-get update
DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends $PKGS
