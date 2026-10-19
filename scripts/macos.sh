#!/usr/bin/env bash
# macOS only lets a program use Bluetooth when it runs as an app bundle that
# explains why it needs it. This wraps our binaries in a minimal bundle.
#
#   scripts/macos.sh run            Build a debug copy of the app and open it
#   scripts/macos.sh app            Build "Sound Connect.app" into dist/ and open it
#   scripts/macos.sh probe [args]   Build the probe example and print its output
set -euo pipefail

cd "$(dirname "$0")/.."
command -v cargo >/dev/null || source "$HOME/.cargo/env"

bundle() { # <binary> <bundle path> <bundle id> <display name>
  local binary=$1 app=$2 id=$3 name=$4
  rm -rf "$app"
  mkdir -p "$app/Contents/MacOS"
  cp "$binary" "$app/Contents/MacOS/"
  cat > "$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key><string>$id</string>
  <key>CFBundleName</key><string>$name</string>
  <key>CFBundleExecutable</key><string>$(basename "$binary")</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>LSUIElement</key><true/>
  <key>NSBluetoothAlwaysUsageDescription</key><string>Sound Connect uses Bluetooth to read and change your headphone settings.</string>
</dict>
</plist>
EOF
  codesign --force --sign - "$app" >/dev/null 2>&1
}

case "${1:-run}" in
  run)
    cargo build -p sound-connect
    app="target/debug/Sound Connect.app"
    bundle target/debug/sound-connect "$app" io.github.aecca.sound-connect "Sound Connect"
    pkill -f "$app/Contents/MacOS/sound-connect" || true
    open "$app"
    echo "Opened $app"
    ;;
  app)
    cargo build --release -p sound-connect
    app="dist/Sound Connect.app"
    bundle target/release/sound-connect "$app" io.github.aecca.sound-connect "Sound Connect"
    open "$app"
    echo "Opened $app"
    ;;
  probe)
    shift
    cargo build -p sc-bluetooth --example probe
    app="target/debug/Probe.app"
    bundle target/debug/examples/probe "$app" io.github.aecca.sound-connect.probe "Sound Connect Probe"
    out=$(mktemp)
    open -W --stdout "$out" --stderr "$out" "$app" --args "$@"
    cat "$out"
    rm -f "$out"
    ;;
  *)
    sed -n '5,7p' "$0"
    exit 1
    ;;
esac
