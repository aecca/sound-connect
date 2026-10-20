import AppKit
import SoundConnectCore

/// Development aid for the README pictures: launched with
/// SOUND_CONNECT_SNAPSHOTS=<folder>, the app shows its real menu and
/// equalizer window with sample data, in light and dark mode, saves a
/// picture of each into the folder and quits. No Bluetooth involved.
@MainActor
enum Snapshots {
    static var folder: URL? {
        ProcessInfo.processInfo.environment["SOUND_CONNECT_SNAPSHOTS"].map { URL(fileURLWithPath: $0) }
    }

    /// A WH-1000XM5 in Ambient Sound, as it reports itself.
    static func sampleModel() -> HeadsetModel {
        let model = HeadsetModel()
        model.connection = .connected(name: "WH-1000XM5")
        model.device = serviceUuids().lazy.compactMap { headsetFor(name: "WH-1000XM5", service: $0) }.first?.device()
        model.battery = Battery(level: 80, charging: false)
        model.codec = "AAC"
        model.firmware = "2.4.1"
        model.mode = .ambient
        model.ambientLevel = 14
        model.focusOnVoice = true
        model.features = [.speakToChat: false, .dsee: true]
        model.equalizer = Equalizer(preset: .manual, bands: .five(clearBass: 3, bands: [2, 1, 0, 1, 3]))
        return model
    }

    static func take(menu: StatusMenu, model: HeadsetModel, into folder: URL) {
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        // An inactive app draws its menus dimmed: come forward first.
        NSApp.activate()
        after(1.5) { shoot([(.aqua, "light"), (.darkAqua, "dark")], menu: menu, model: model, folder: folder) }
    }

    private static func shoot(
        _ appearances: [(NSAppearance.Name, String)], menu: StatusMenu, model: HeadsetModel, folder: URL
    ) {
        guard let (appearance, suffix) = appearances.first else { exit(0) }
        NSApp.appearance = NSAppearance(named: appearance)
        EqualizerWindow.show(model: model)

        after(1) {
            let equalizer = NSApp.windows.first { $0.title == "Equalizer" }
            save(equalizer?.windowNumber, to: folder.appendingPathComponent("equalizer-\(suffix).png"))
            equalizer?.orderOut(nil)

            menu.popUp(at: NSPoint(x: 120, y: 900), appearance: NSAppearance(named: appearance))
            after(1) {
                save(ownMenuWindow(), to: folder.appendingPathComponent("menu-\(suffix).png"))
                menu.close()
                after(0.5) { shoot(Array(appearances.dropFirst()), menu: menu, model: model, folder: folder) }
            }
        }
    }

    /// Runs a block later, also while a menu is open.
    private static func after(_ seconds: TimeInterval, _ block: @escaping @MainActor () -> Void) {
        let timer = Timer(timeInterval: seconds, repeats: false) { _ in MainActor.assumeIsolated { block() } }
        RunLoop.main.add(timer, forMode: .common)
    }

    /// This app's open menu: its panel, a window at the pop-up menu level.
    private static func ownMenuWindow() -> Int? {
        let windows = CGWindowListCopyWindowInfo(.optionOnScreenOnly, kCGNullWindowID) as? [[String: Any]] ?? []
        return windows.first {
            ($0[kCGWindowOwnerPID as String] as? Int32) == getpid()
                && ($0[kCGWindowLayer as String] as? Int) == Int(CGWindowLevelForKey(.popUpMenuWindow))
        }?[kCGWindowNumber as String] as? Int
    }

    /// Saves a picture of one of this app's own windows, shadow included. An
    /// app may capture its own windows without the Screen Recording
    /// permission; the API is reached at runtime because the SDK hides it.
    private static func save(_ windowNumber: Int?, to url: URL) {
        typealias CreateImage = @convention(c) (CGRect, UInt32, UInt32, UInt32) -> Unmanaged<CGImage>?
        guard let windowNumber,
              let handle = dlopen("/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics", RTLD_NOW),
              let symbol = dlsym(handle, "CGWindowListCreateImage")
        else { return NSLog("Snapshots: nothing to capture for \(url.lastPathComponent)") }
        let createImage = unsafeBitCast(symbol, to: CreateImage.self)
        let includingWindow: UInt32 = 1 << 3
        guard let image = createImage(.null, includingWindow, UInt32(windowNumber), 0)?.takeRetainedValue() else {
            return NSLog("Snapshots: couldn't capture \(url.lastPathComponent)")
        }
        try? NSBitmapImageRep(cgImage: image).representation(using: .png, properties: [:])?.write(to: url)
    }
}
