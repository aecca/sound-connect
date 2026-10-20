// Sound Connect: control Sony and Bose headphones from the macOS menu bar.

import AppKit
import ServiceManagement

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    private let model = HeadsetModel()
    private let updater = Updater()
    private var link: HeadsetLink?
    private var menu: StatusMenu?

    func applicationDidFinishLaunching(_ notification: Notification) {
        if let folder = Snapshots.folder {
            let sample = Snapshots.sampleModel()
            let menu = StatusMenu(model: sample, updater: updater, calls: Calls(model: sample))
            self.menu = menu
            Snapshots.take(menu: menu, model: sample, into: folder)
            return
        }
        LoginItem.enableOnFirstLaunch()
        link = HeadsetLink(model: model)
        menu = StatusMenu(model: model, updater: updater, calls: Calls(model: model))
        updater.start()
    }

    /// Opening the app again, e.g. from Spotlight, shows its menu: macOS
    /// hides the icon when the menu bar is too full to fit it.
    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        menu?.show()
        return false
    }
}

/// Opening at login, like most menu bar utilities. It's on after the first
/// launch; "Open at Login" in the menu turns it off.
enum LoginItem {
    private static let offeredKey = "OpenAtLoginOffered"

    static func enableOnFirstLaunch() {
        guard !UserDefaults.standard.bool(forKey: offeredKey) else { return }
        UserDefaults.standard.set(true, forKey: offeredKey)
        set(true)
    }

    static func set(_ on: Bool) {
        do {
            if on { try SMAppService.mainApp.register() } else { try SMAppService.mainApp.unregister() }
        } catch {
            NSLog("Sound Connect: couldn't change Open at Login: \(error)")
        }
    }
}

// Debug output goes to a file when launched with `open --stdout`; flush it per line.
setvbuf(stdout, nil, _IOLBF, 0)

let app = NSApplication.shared
// Top-level code runs on the main thread.
let delegate = MainActor.assumeIsolated { AppDelegate() }
app.delegate = delegate
// Live in the menu bar only: no Dock icon, no app menu.
app.setActivationPolicy(.accessory)
app.run()
