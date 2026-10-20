#!/usr/bin/env bash
# Builds and runs Sound Connect on macOS. macOS only lets a program use
# Bluetooth when it runs as an app bundle that explains why it needs it, so
# every binary gets wrapped in a minimal bundle.
#
#   scripts/macos.sh run            Build a debug copy of the native app and open it
#   scripts/macos.sh app            Build "Sound Connect.app" into dist/ and open it
#   scripts/macos.sh package        Build the release: a universal app zipped in dist/
#   scripts/macos.sh icon           Rebuild macos/AppIcon.icns from assets/icon.svg
#   scripts/macos.sh snapshots      Retake the README pictures in docs/images
#   scripts/macos.sh gpui           Build the GPUI app (the Linux UI) and open it
#   scripts/macos.sh probe [args]   Build the probe example and print its output
set -euo pipefail

cd "$(dirname "$0")/.."
command -v cargo >/dev/null || source "$HOME/.cargo/env"

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"

bundle() { # <binary> <bundle path> <bundle id> <display name>
  local binary=$1 app=$2 id=$3 name=$4
  rm -rf "$app"
  mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
  cp "$binary" "$app/Contents/MacOS/"
  cp macos/AppIcon.icns "$app/Contents/Resources/"
  cat > "$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key><string>$id</string>
  <key>CFBundleName</key><string>$name</string>
  <key>CFBundleExecutable</key><string>$(basename "$binary")</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>LSMinimumSystemVersion</key><string>14.0</string>
  <key>LSUIElement</key><true/>
  <key>NSBluetoothAlwaysUsageDescription</key><string>Sound Connect uses Bluetooth to read and change your headphone settings.</string>
  <key>CFBundleURLTypes</key>
  <array>
    <dict>
      <key>CFBundleURLName</key><string>$id</string>
      <key>CFBundleURLSchemes</key><array><string>soundconnect</string></array>
    </dict>
  </array>
</dict>
</plist>
EOF
  codesign --force --sign - "$app" >/dev/null 2>&1
}

# Compiles the shared Rust protocol core and generates its Swift bindings
# into macos/Generated, where the Swift package expects them. With
# `universal`, the library covers both Apple Silicon and Intel Macs.
core() {
  local out=macos/Generated lib=target/release/libsound_connect_core.a
  cargo build --release -p sc-ffi
  if [ "${1:-}" = universal ]; then
    rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
    cargo build --release -p sc-ffi --target aarch64-apple-darwin
    cargo build --release -p sc-ffi --target x86_64-apple-darwin
    lib=target/universal/libsound_connect_core.a
    mkdir -p target/universal
    lipo -create -output "$lib" \
      target/aarch64-apple-darwin/release/libsound_connect_core.a \
      target/x86_64-apple-darwin/release/libsound_connect_core.a
  fi
  rm -rf "$out"
  mkdir -p "$out/SoundConnectCore" "$out/include" "$out/bindings"
  cargo run -q --release -p sc-ffi --bin uniffi-bindgen -- generate \
    --library target/release/libsound_connect_core.dylib --language swift --out-dir "$out/bindings"
  mv "$out/bindings/sound_connect_core.swift" "$out/SoundConnectCore/"
  mv "$out/bindings/sound_connect_coreFFI.h" "$out/include/"
  mv "$out/bindings/sound_connect_coreFFI.modulemap" "$out/include/module.modulemap"
  rm -rf "$out/bindings"
  xcodebuild -create-xcframework -library "$lib" \
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
  package)
    core universal
    # One build per architecture, joined with lipo: SwiftPM can't link a
    # static xcframework when building both at once.
    bins=()
    for arch in arm64 x86_64; do
      swift build --package-path macos --disable-sandbox -c release --arch "$arch"
      bins+=("$(swift build --package-path macos --disable-sandbox -c release --arch "$arch" --show-bin-path)/SoundConnect")
    done
    bin=target/universal/SoundConnect
    lipo -create -output "$bin" "${bins[@]}"
    rm -rf dist/package
    bundle "$bin" "dist/package/Sound Connect.app" io.github.aecca.sound-connect "Sound Connect"
    rm -f dist/SoundConnect-macos.zip
    ditto -c -k --norsrc --noextattr --keepParent "dist/package/Sound Connect.app" dist/SoundConnect-macos.zip
    echo "Built dist/SoundConnect-macos.zip ($VERSION; $(lipo -archs "$bin"))"
    ;;
  icon)
    iconset="$(mktemp -d)/AppIcon.iconset"
    mkdir -p "$iconset"
    for size in 16 32 128 256 512; do
      swift scripts/render-icon.swift assets/icon.svg "$iconset/icon_${size}x${size}.png" "$size"
      swift scripts/render-icon.swift assets/icon.svg "$iconset/icon_${size}x${size}@2x.png" "$((size * 2))"
    done
    iconutil -c icns "$iconset" -o macos/AppIcon.icns
    echo "Built macos/AppIcon.icns"
    ;;
  snapshots)
    core
    swift build --package-path macos --disable-sandbox
    SOUND_CONNECT_SNAPSHOTS="$PWD/docs/images" \
      "$(swift build --package-path macos --disable-sandbox --show-bin-path)/SoundConnect"
    echo "Saved pictures in docs/images"
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
    sed -n '6,12p' "$0"
    exit 1
    ;;
esac
