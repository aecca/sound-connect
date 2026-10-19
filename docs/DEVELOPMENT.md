# How Sound Connect is built

Sound Connect talks to Sony headphones over a Bluetooth serial channel (RFCOMM) using Sony's binary protocol, the same one their phone app uses. One shared core speaks the protocol; each platform has its own app around it.

## Layout

| Path | What it is |
| --- | --- |
| `crates/protocol` | The protocol, with no I/O: message framing, typed commands, model profiles, and a `Session` that handles acknowledgements and retries. Tested against messages captured from a real WH-1000XM5. |
| `crates/ffi` | Swift bindings for the protocol (UniFFI), used by the Mac app. |
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
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The protocol tests use real messages. When adding support for a model or setting, capture what the headphones send (`SOUND_CONNECT_DEBUG=1` on the Mac app) and add it as a test.

## Releasing

Push a version tag; GitHub Actions builds the Mac app and the Linux packages and publishes them:

```sh
git tag -a v0.2.0 -m "Sound Connect 0.2.0"
git push origin v0.2.0
```

Update `version` in `Cargo.toml` first. `install.sh` always installs the latest release.

## Where the protocol comes from

The byte layouts come from [Gadgetbridge](https://codeberg.org/Freeyourgadget/Gadgetbridge) and [Sony Device Center](https://github.com/marconvcm/sony-device-center), and are checked against real headphones where possible. Comments in `crates/protocol` say which layouts haven't been tried on hardware yet.
