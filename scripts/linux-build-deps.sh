#!/usr/bin/env bash
# Installs what building Sound Connect needs on Ubuntu/Debian, and no more:
# a C toolchain, pkg-config, and the D-Bus (BlueR), xcb, xkbcommon and
# fontconfig (GPUI) headers.
set -euo pipefail
sudo=""
[ "$(id -u)" -eq 0 ] || sudo=sudo
$sudo apt-get update -qq
$sudo apt-get install -y -qq --no-install-recommends \
  ca-certificates curl git build-essential pkg-config \
  libdbus-1-dev libxcb1-dev libxkbcommon-dev libxkbcommon-x11-dev libfontconfig-dev
