#!/usr/bin/env bash
# Builds the Linux release, dist/sound-connect-linux-<arch>.tar.gz.
#
#   scripts/package-linux.sh          On Linux, with the build dependencies installed
#   scripts/package-linux.sh docker   Anywhere with Docker, for this machine's architecture
#
# Releases are built on Ubuntu 22.04 so they run on its glibc and anything
# newer: Ubuntu 22.04+, Debian 12+, Arch and Omarchy.
set -euo pipefail
cd "$(dirname "$0")/.."

if [ "${1:-}" = docker ]; then
  docker build -t sound-connect-package -f scripts/package-linux.Dockerfile scripts
  exec docker run --rm -v "$PWD":/src -v sound-connect-cargo:/root/.cargo/registry \
    -v sound-connect-package-target:/target sound-connect-package
fi

command -v cargo >/dev/null || source "$HOME/.cargo/env"
arch="$(uname -m)"
[ "$arch" = arm64 ] && arch=aarch64
target="${CARGO_TARGET_DIR:-target}"

cargo build --release -p sound-connect

root="$(mktemp -d)/sound-connect"
install -Dm755 "$target/release/sound-connect" "$root/bin/sound-connect"
strip "$root/bin/sound-connect"
install -Dm644 assets/icon.svg "$root/share/icons/hicolor/scalable/apps/sound-connect.svg"
install -Dm644 /dev/stdin "$root/share/applications/sound-connect.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Sound Connect
Comment=A light, fast app to control your Sony and Bose headphones
Exec=sound-connect
Icon=sound-connect
Categories=AudioVideo;Audio;Settings;
StartupNotify=false
EOF
cp LICENSE "$root/"

mkdir -p dist
tar -czf "dist/sound-connect-linux-$arch.tar.gz" -C "$(dirname "$root")" sound-connect
echo "Built dist/sound-connect-linux-$arch.tar.gz"
