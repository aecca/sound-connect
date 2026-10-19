#!/usr/bin/env bash
# Builds and runs Sound Connect on macOS. macOS only lets a program use
# Bluetooth when it runs as an app bundle that explains why it needs it, so
# every binary gets wrapped in a minimal bundle.
#
#   scripts/macos.sh run            Build a debug copy of the native app and open it
#   scripts/macos.sh app            Build "Sound Connect.app" into dist/ and open it
#   scripts/macos.sh gpui           Build the GPUI app (the Linux UI) and open it
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
  <key>LSMinimumSystemVersion</key><string>14.0</string>
  <key>LSUIElement</key><true/>
  <key>NSBluetoothAlwaysUsageDescription</key><string>Sound Connect uses Bluetooth to read and change your headphone settings.</string>
</dict>
</plist>
EOF
  codesign --force --sign - "$app" >/dev/null 2>&1
}

# Compiles the shared Rust protocol core and generates its Swift bindings
# into macos/Generated, where the Swift package expects them.
core() {
  local out=macos/Generated
  cargo build --release -p sc-ffi
  rm -rf "$out"
  mkdir -p "$out/SoundConnectCore" "$out/include" "$out/bindings"
  cargo run -q --release -p sc-ffi --bin uniffi-bindgen -- generate \
    --library target/release/libsound_connect_core.dylib --language swift --out-dir "$out/bindings"
  mv "$out/bindings/sound_connect_core.swift" "$out/SoundConnectCore/"
  mv "$out/bindings/sound_connect_coreFFI.h" "$out/include/"
  mv "$out/bindings/sound_connect_coreFFI.modulemap" "$out/include/module.modulemap"
  rm -rf "$out/bindings"
  xcodebuild -create-xcframework -library target/release/libsound_connect_core.a \
    -headers "$out/include" -output "$out/SoundConnectCoreFFI.xcframework" >/dev/null
}

native() { # <debug|release> <bundle path>
  core
  swift build --package-path macos --disable-sandbox -c "$1"
  local bin
  bin="$(swift build --package-path macos --disable-sandbox -c "$1" --show-bin-path)/SoundConnect"
  bundle "$bin" "$2" io.github.aecca.sound-connect "Sound Connect"
}

relaunch() { # <bundle path>
  pkill -f "$1/Contents/MacOS/" || true
  open "$1"
  echo "Opened $1"
}

case "${1:-run}" in
  run)
    native debug "target/debug/Sound Connect.app"
    relaunch "target/debug/Sound Connect.app"
    ;;
  app)
    native release "dist/Sound Connect.app"
    relaunch "dist/Sound Connect.app"
    ;;
  gpui)
    cargo build -p sound-connect
    bundle target/debug/sound-connect "target/debug/Sound Connect (GPUI).app" \
      io.github.aecca.sound-connect.gpui "Sound Connect (GPUI)"
    relaunch "target/debug/Sound Connect (GPUI).app"
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
    sed -n '6,9p' "$0"
    exit 1
    ;;
esac
