// Sound Connect: control Sony headphones from the macOS menu bar.

import AppKit

final class AppDelegate: NSObject, NSApplicationDelegate {
    private let model = HeadsetModel()
    private var link: HeadsetLink?
    private var menu: StatusMenu?

    func applicationDidFinishLaunching(_ notification: Notification) {
        link = HeadsetLink(model: model)
        menu = StatusMenu(model: model)
    }
}

// Debug output goes to a file when launched with `open --stdout`; flush it per line.
setvbuf(stdout, nil, _IOLBF, 0)

let app = NSApplication.shared
let delegate = AppDelegate()
app.delegate = delegate
// Live in the menu bar only: no Dock icon, no app menu.
app.setActivationPolicy(.accessory)
app.run()
