// swift-tools-version: 6.0
// The native macOS app. Build it with `scripts/macos.sh run` (or `app` for a
// release build): the script first compiles the shared Rust protocol core and
// generates its Swift bindings into Generated/.
import PackageDescription

let package = Package(
    name: "SoundConnect",
    platforms: [.macOS(.v14)],
    targets: [
        .binaryTarget(name: "SoundConnectCoreFFI", path: "Generated/SoundConnectCoreFFI.xcframework"),
        .target(name: "SoundConnectCore", dependencies: ["SoundConnectCoreFFI"], path: "Generated/SoundConnectCore"),
        .executableTarget(
            name: "SoundConnect",
            dependencies: ["SoundConnectCore"],
            linkerSettings: [.linkedFramework("IOBluetooth")]
        ),
    ],
    swiftLanguageModes: [.v5]
)
