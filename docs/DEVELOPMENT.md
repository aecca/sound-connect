# How Sound Connect is built

Sound Connect talks to headphones over a Bluetooth serial channel (RFCOMM) using each brand's own protocol, the same one their phone app uses. One shared core has a driver per brand; each platform has its own app around it, and neither the apps nor the Bluetooth code know which brand they're talking to.

## Layout

| Path | What it is |
| --- | --- |
| `crates/core` | The protocols, with no I/O. `device.rs` is the brand-neutral model apps use (`Device`, `Update`, `Setting`); `driver.rs` the `Driver` contract; `registry.rs` picks a driver by the headset's control service, and the service whose channel to open. |
| `crates/core/src/drivers/sony` | Sony: framing, commands, per-model profiles and the ACK-based session. Tested against messages captured from a real WH-1000XM5. |
| `crates/core/src/drivers/bose` | Bose QuietComfort 35 and 35 II, over BMAP. |
| `crates/ffi` | Swift bindings for the drivers (UniFFI), used by the Mac app. |
| `macos/` | The native Mac app (Swift, AppKit menu, IOBluetooth). |
| `crates/bluetooth` | Bluetooth link for the Rust app: BlueZ on Linux, IOBluetooth on macOS. |
| `crates/app` | The Linux app (Rust, GPUI): a tray icon and a small panel. |
| `install.sh` | The one-line installer. |
| `scripts/` | Build, package and test helpers. |
| `assets/icon.svg` | The app icon. |

## Mac

Needs Xcode and Rust ([rustup.rs](https://rustup.rs)).

```sh
scripts/macos.sh run       # build a debug copy of the app and open it
scripts/macos.sh package   # universal release, dist/SoundConnect-macos.zip
scripts/macos.sh icon      # rebuild macos/AppIcon.icns after editing assets/icon.svg
scripts/macos.sh snapshots # retake the README pictures (real UI, sample data)
```

macOS only lets apps use Bluetooth from an app bundle, so the script always wraps the binary in one. Launch with `SOUND_CONNECT_DEBUG=1` (for example `open --env SOUND_CONNECT_DEBUG=1 --stdout log.txt "target/debug/Sound Connect.app"`) to log what the headphones report.

## Linux

Needs Rust and a few headers; `scripts/linux-build-deps.sh` installs them on Ubuntu or Debian.

```sh
cargo run -p sound-connect          # run the app
scripts/package-linux.sh            # release tarball in dist/
scripts/package-linux.sh docker     # the same, from any machine with Docker
```

To build, test and lint for Linux from a Mac, see the comment at the top of `scripts/linux.Dockerfile`.

## Tests

```sh
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The protocol tests use real messages. When adding support for a model or setting, capture what the headphones send (`SOUND_CONNECT_DEBUG=1` on the Mac app) and add it as a test.

## Adding a brand

1. Add `crates/core/src/drivers/<brand>/` with a type implementing `Driver`: it turns bytes into `Update`s and `Setting`s into bytes, and describes the headset as a `Device`.
2. Add its control service UUID to `SERVICES` and a branch to `driver_for` in `registry.rs`. If its protocol runs on another service's channel (like Bose on Serial Port), add a branch to `channel_service` too.
3. Add tests built from real messages.

The Bluetooth code, the Mac menu and the Linux panel pick it up without changes: they show whatever the `Device` says the headset has.

## Releasing

Push a version tag; GitHub Actions builds the Mac app and the Linux packages and publishes them:

```sh
git tag -a v0.2.0 -m "Sound Connect 0.2.0"
git push origin v0.2.0
```

Update `version` in `Cargo.toml` first. `install.sh` always installs the latest release.

Installed apps find the new version on their own. Once a day they ask where GitHub's latest-release link redirects (`.../releases/tag/v0.2.0`) and, if it's newer, offer "Update to 0.2.0" in the menu or panel. Clicking it runs that release's `install.sh`, which quits the app, replaces it and opens the new one; its output goes to `~/Library/Logs/Sound Connect/update.log` on a Mac and `~/.local/state/sound-connect/update.log` on Linux. The code is `macos/Sources/SoundConnect/Updater.swift` and `crates/app/src/update.rs`; the Linux app uses `curl`, which the installer needs anyway, so it carries no HTTP or TLS code.

## Where the protocols come from

Sony's byte layouts come from [Gadgetbridge](https://codeberg.org/Freeyourgadget/Gadgetbridge) and [Sony Device Center](https://github.com/marconvcm/sony-device-center); Bose's from [bosectl](https://github.com/alabalag1/bosectl) and [based-connect](https://github.com/Denton-L/based-connect). They're checked against real headphones where possible, and comments in `crates/core` say which layouts haven't been tried on hardware yet.
