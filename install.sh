#!/usr/bin/env bash
# Installs Sound Connect on macOS, Ubuntu/Debian and Arch (including Omarchy):
#
#   curl -fsSL https://raw.githubusercontent.com/aecca/sound-connect/main/install.sh | bash
#
# It downloads the latest release. Options, passed as `bash -s -- <option>`:
#   --from-source   Build from the source code instead
#   --uninstall     Remove Sound Connect
#
# Environment: SOUND_CONNECT_VERSION picks a release tag (default: latest);
# SOUND_CONNECT_ARTIFACTS installs from a local folder of release files.
set -euo pipefail

REPO="aecca/sound-connect"
VERSION="${SOUND_CONNECT_VERSION:-latest}"
MODE="release"
case "${1:-}" in
  --from-source) MODE="source" ;;
  --uninstall) MODE="uninstall" ;;
  "") ;;
  *) echo "Unknown option: $1" >&2; exit 1 ;;
esac

say() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33mNote:\033[0m %s\n' "$*"; }
fail() { printf '\033[1;31mError:\033[0m %s\n' "$*" >&2; exit 1; }

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# Gets a release file into $2, from GitHub or from SOUND_CONNECT_ARTIFACTS.
fetch() {
  if [ -n "${SOUND_CONNECT_ARTIFACTS:-}" ]; then
    cp "$SOUND_CONNECT_ARTIFACTS/$1" "$2"
    return
  fi
  local url="https://github.com/$REPO/releases/latest/download/$1"
  if [ "$VERSION" != latest ]; then
    url="https://github.com/$REPO/releases/download/$VERSION/$1"
  fi
  curl -fL --progress-bar "$url" -o "$2" ||
    fail "Could not download $1. If there is no release yet, run the installer with --from-source."
}

# The source code: this checkout when run from one, else a fresh clone.
source_dir() {
  local here=""
  if [ -n "${BASH_SOURCE[0]:-}" ] && [ -f "${BASH_SOURCE[0]}" ]; then
    here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  fi
  if [ -n "$here" ] && [ -f "$here/Cargo.toml" ] && [ -d "$here/crates/core" ]; then
    echo "$here"
  else
    git clone --quiet --depth 1 "https://github.com/$REPO.git" "$TMP/src"
    echo "$TMP/src"
  fi
}

ensure_rust() {
  if ! command -v cargo >/dev/null && [ -f "$HOME/.cargo/env" ]; then
    source "$HOME/.cargo/env"
  fi
  if ! command -v cargo >/dev/null; then
    say "Installing Rust (rustup.rs)"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
    source "$HOME/.cargo/env"
  fi
}

# ---------------------------------------------------------------- macOS

install_macos() {
  local apps="/Applications"
  [ -w "$apps" ] || apps="$HOME/Applications"
  local app="$apps/Sound Connect.app"

  if [ "$MODE" = uninstall ]; then
    pkill -x SoundConnect 2>/dev/null || true
    rm -rf "$app"
    say "Removed $app"
    return
  fi

  if [ "$MODE" = source ]; then
    [ -d /Applications/Xcode.app ] || fail "Building from source on a Mac needs Xcode, from the App Store."
    ensure_rust
    local src
    src="$(source_dir)"
    say "Building Sound Connect (this takes a few minutes)"
    "$src/scripts/macos.sh" package
    cp "$src/dist/SoundConnect-macos.zip" "$TMP/app.zip"
  else
    say "Getting Sound Connect for Mac"
    fetch SoundConnect-macos.zip "$TMP/app.zip"
  fi

  ditto -x -k "$TMP/app.zip" "$TMP/app"
  mkdir -p "$apps"
  # -a: macOS's pkill skips its own ancestors otherwise, and when the app
  # updates itself, it's the one that started this script.
  pkill -a -x SoundConnect 2>/dev/null || true
  rm -rf "$app"
  mv "$TMP/app/Sound Connect.app" "$app"
  # curl downloads aren't quarantined, but clear it in case.
  xattr -dr com.apple.quarantine "$app" 2>/dev/null || true

  say "Installed $app"
  open "$app"
  echo "Look for the headphones icon in your menu bar. The first time, allow Bluetooth when asked."
}

# ---------------------------------------------------------------- Linux

DISTRO="other"
detect_distro() {
  local ids=""
  if [ -r /etc/os-release ]; then
    ids="$(. /etc/os-release && echo " ${ID:-} ${ID_LIKE:-} ")"
  fi
  case "$ids" in
    *" arch "*) DISTRO="arch" ;;
    *" ubuntu "* | *" debian "*) DISTRO="debian" ;;
  esac
}

as_root() {
  if [ "$(id -u)" -eq 0 ]; then "$@"; else sudo "$@"; fi
}

# Installs whichever of the given packages are missing, and nothing else.
# On a typical desktop they're all there already and this does nothing.
install_missing() {
  local missing=() package
  case "$DISTRO" in
    debian)
      for package in "$@"; do
        dpkg-query -W -f '${Status}' "$package" 2>/dev/null | grep -q "ok installed" || missing+=("$package")
      done
      [ ${#missing[@]} -gt 0 ] || return 0
      say "Installing ${missing[*]}"
      as_root apt-get update -qq
      as_root apt-get install -y -qq --no-install-recommends "${missing[@]}"
      ;;
    arch)
      for package in "$@"; do
        pacman -Qq "$package" >/dev/null 2>&1 || missing+=("$package")
      done
      [ ${#missing[@]} -gt 0 ] || return 0
      say "Installing ${missing[*]}"
      as_root pacman -S --needed --noconfirm "${missing[@]}" ||
        fail "Could not install ${missing[*]}. Update your system with 'sudo pacman -Syu' and try again."
      ;;
  esac
}

has_vulkan_driver() {
  compgen -G "/usr/share/vulkan/icd.d/*.json" >/dev/null || compgen -G "/etc/vulkan/icd.d/*.json" >/dev/null
}

# Only what the app loads: D-Bus and BlueZ for Bluetooth, xcb and
# xkbcommon for X11 and keyboard input, Wayland, and Vulkan to draw.
install_runtime_packages() {
  case "$DISTRO" in
    debian)
      install_missing libdbus-1-3 libxcb1 libxkbcommon0 libxkbcommon-x11-0 libwayland-client0 libvulkan1 bluez
      has_vulkan_driver || install_missing mesa-vulkan-drivers
      ;;
    arch)
      install_missing dbus libxcb libxkbcommon libxkbcommon-x11 wayland vulkan-icd-loader bluez
      has_vulkan_driver || warn "No Vulkan driver found. Install the one for your graphics card (e.g. vulkan-intel, vulkan-radeon)."
      ;;
    *)
      warn "Unknown distribution. Sound Connect needs D-Bus, BlueZ, xkbcommon, Wayland or X11, and Vulkan."
      ;;
  esac
}

install_build_packages() {
  case "$DISTRO" in
    debian) install_missing git curl ca-certificates build-essential pkg-config \
      libdbus-1-dev libxcb1-dev libxkbcommon-dev libxkbcommon-x11-dev libfontconfig-dev ;;
    arch) install_missing git base-devel pkgconf dbus libxcb libxkbcommon libxkbcommon-x11 fontconfig ;;
    *) warn "Unknown distribution. Building needs a C compiler, pkg-config and the D-Bus, xcb, xkbcommon and fontconfig headers." ;;
  esac
}

linux_arch() {
  case "$(uname -m)" in
    x86_64 | amd64) echo x86_64 ;;
    aarch64 | arm64) echo aarch64 ;;
    *) fail "Sound Connect isn't built for $(uname -m) yet." ;;
  esac
}

BIN="$HOME/.local/bin/sound-connect"
SHARE="${XDG_DATA_HOME:-$HOME/.local/share}"
CONFIG="${XDG_CONFIG_HOME:-$HOME/.config}"
AUTOSTART_MARK="# Sound Connect"

# Hyprland (Omarchy) doesn't run XDG autostart entries: it has its own list.
hyprland_config() {
  if [ -f "$CONFIG/hypr/autostart.conf" ]; then
    echo "$CONFIG/hypr/autostart.conf"
  elif [ -f "$CONFIG/hypr/hyprland.conf" ]; then
    echo "$CONFIG/hypr/hyprland.conf"
  fi
}

uninstall_linux() {
  pkill -x sound-connect 2>/dev/null || true
  rm -f "$BIN" "$SHARE/applications/sound-connect.desktop" "$CONFIG/autostart/sound-connect.desktop" \
    "$SHARE/icons/hicolor/scalable/apps/sound-connect.svg"
  local conf
  conf="$(hyprland_config)"
  if [ -n "$conf" ]; then
    sed -i "/^$AUTOSTART_MARK\$/,+1d" "$conf"
  fi
  say "Removed Sound Connect"
}

install_linux() {
  detect_distro
  if [ "$MODE" = uninstall ]; then
    uninstall_linux
    return
  fi

  local tarball="$TMP/sound-connect.tar.gz"
  if [ "$MODE" = source ]; then
    install_build_packages
    ensure_rust
    local src
    src="$(source_dir)"
    say "Building Sound Connect (this takes a while the first time)"
    (cd "$src" && scripts/package-linux.sh)
    cp "$src/dist/sound-connect-linux-$(linux_arch).tar.gz" "$tarball"
  else
    say "Getting Sound Connect for Linux ($(linux_arch))"
    fetch "sound-connect-linux-$(linux_arch).tar.gz" "$tarball"
  fi
  install_runtime_packages

  tar -xzf "$tarball" -C "$TMP"
  local root="$TMP/sound-connect"
  mkdir -p "$(dirname "$BIN")" "$SHARE/applications" "$SHARE/icons/hicolor/scalable/apps" "$CONFIG/autostart"
  install -m755 "$root/bin/sound-connect" "$BIN"
  install -m644 "$root/share/icons/hicolor/scalable/apps/sound-connect.svg" "$SHARE/icons/hicolor/scalable/apps/"
  sed "s|^Exec=.*|Exec=$BIN|" "$root/share/applications/sound-connect.desktop" \
    > "$SHARE/applications/sound-connect.desktop"
  # Start at login: XDG autostart (GNOME, KDE, ...) and Hyprland.
  cp "$SHARE/applications/sound-connect.desktop" "$CONFIG/autostart/sound-connect.desktop"
  local conf
  conf="$(hyprland_config)"
  if [ -n "$conf" ] && ! grep -qx "$AUTOSTART_MARK" "$conf"; then
    # Start on a new line, without leaving a blank one behind on uninstall.
    [ -z "$(tail -c 1 "$conf")" ] || echo >> "$conf"
    printf '%s\nexec-once = %s\n' "$AUTOSTART_MARK" "$BIN" >> "$conf"
  fi

  "$BIN" --version >/dev/null || fail "The installed app doesn't start; see the error above."
  say "Installed $BIN"
  case ":$PATH:" in
    *":$(dirname "$BIN"):"*) ;;
    *) warn "To use the sound-connect command in a terminal or a keyboard shortcut, log out and back in once." ;;
  esac

  if command -v systemctl >/dev/null && ! systemctl is-active --quiet bluetooth 2>/dev/null; then
    warn "Bluetooth isn't running. Turn it on with: sudo systemctl enable --now bluetooth"
  fi
  if [ -n "${WAYLAND_DISPLAY:-}" ] || [ -n "${DISPLAY:-}" ]; then
    pkill -x sound-connect 2>/dev/null || true
    setsid "$BIN" > /dev/null 2>&1 < /dev/null &
    echo "Look for the headphones icon in your system tray."
  fi
  case "${XDG_CURRENT_DESKTOP:-}" in
    *GNOME*) warn "GNOME shows tray icons only with the AppIndicator extension." ;;
  esac
}

case "$(uname -s)" in
  Darwin) install_macos ;;
  Linux) install_linux ;;
  *) fail "Sound Connect runs on macOS and Linux." ;;
esac
