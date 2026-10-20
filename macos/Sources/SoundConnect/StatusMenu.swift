import AppKit
import Observation
import ServiceManagement
import SoundConnectCore
import SwiftUI

/// The menu bar icon and the panel it opens, which works like a menu: it
/// closes on a click elsewhere or Escape, and doesn't take focus from the
/// app in front. What's in it is `MenuView`.
@MainActor
final class StatusMenu: NSObject {
    private let model: HeadsetModel
    private let popUp: ModePopUp
    private let options = MenuOptions()
    private let statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
    private let panel = MenuPanel()
    /// Watch for a click elsewhere or Escape while the panel is open.
    private var monitors: [Any] = []

    init(
        model: HeadsetModel, updater: Updater, calls: Calls, shortcut: ModeShortcut, batteryAlerts: BatteryAlerts,
        popUp: ModePopUp
    ) {
        self.model = model
        self.popUp = popUp
        super.init()
        let actions = MenuActions(
            installUpdate: { [weak self] in
                self?.close()
                updater.install()
            },
            editEqualizer: { [weak self] in
                self?.close()
                EqualizerWindow.show(model: model)
            },
            powerOff: { [weak self] in
                self?.close()
                self?.confirmPowerOff()
            },
            connect: { name in model.connect(name) },
            openAtLogin: { LoginItem.set($0) },
            quit: { NSApp.terminate(nil) })
        let view = MenuView(
            model: model, updater: updater, calls: calls, shortcut: shortcut, batteryAlerts: batteryAlerts,
            popUp: popUp, options: options, actions: actions)
        panel.setContent(view) { [weak self] size in self?.resize(to: size) }

        statusItem.button?.target = self
        statusItem.button?.action = #selector(togglePanel)
        statusItem.button?.sendAction(on: [.leftMouseDown, .rightMouseDown])
        observe()
    }

    /// Shows the panel at a point on screen, e.g. for snapshots, whether or
    /// not the menu bar icon is visible. It follows the system's light or
    /// dark mode unless given an appearance.
    func popUp(at point: NSPoint, appearance: NSAppearance?) {
        panel.appearance = appearance
        open(at: point)
    }

    /// Opens the panel under its icon or, when macOS hides the icon because
    /// the menu bar is full, where the icon would be: just right of the camera
    /// notch, or at the top right of screens without one.
    func show() {
        open(at: nil)
    }

    func close() {
        guard panel.isVisible else { return }
        panel.orderOut(nil)
        statusItem.button?.highlight(false)
        monitors.forEach(NSEvent.removeMonitor)
        monitors = []
        popUp.menuIsOpen = false
    }

    @objc private func togglePanel() {
        if panel.isVisible { close() } else { open(at: nil) }
    }

    private func open(at point: NSPoint?) {
        options.opensAtLogin = SMAppService.mainApp.status == .enabled
        let size = panel.contentSize
        let topLeft = point ?? anchor(width: size.width)
        panel.setFrame(
            NSRect(x: topLeft.x, y: topLeft.y - size.height, width: size.width, height: size.height), display: true)
        panel.makeKeyAndOrderFront(nil)
        statusItem.button?.highlight(true)
        popUp.menuIsOpen = true
        monitors = [
            NSEvent.addGlobalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown]) { [weak self] _ in
                MainActor.assumeIsolated { self?.close() }
            },
            NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
                guard event.keyCode == 53 else { return event }  // Escape
                MainActor.assumeIsolated { self?.close() }
                return nil
            },
        ].compactMap { $0 }
    }

    /// The panel's top left corner: under the icon, kept on screen.
    private func anchor(width: CGFloat) -> NSPoint {
        if let button = statusItem.button, let window = button.window, window.occlusionState.contains(.visible) {
            let frame = window.convertToScreen(button.convert(button.bounds, to: nil))
            let right = (window.screen ?? NSScreen.main)?.visibleFrame.maxX ?? frame.maxX
            return NSPoint(x: min(frame.minX, right - width - 8), y: frame.minY - 4)
        }
        guard let screen = NSScreen.main else { return .zero }
        let left = screen.auxiliaryTopRightArea?.minX ?? screen.frame.maxX - width - 8
        return NSPoint(x: min(left, screen.visibleFrame.maxX - width - 8), y: screen.visibleFrame.maxY - 4)
    }

    /// Follows the content as rows show and hide, keeping the top in place.
    private func resize(to size: CGSize) {
        guard panel.isVisible else { return }
        let frame = panel.frame
        panel.setFrame(
            NSRect(x: frame.minX, y: frame.maxY - size.height, width: size.width, height: size.height), display: true)
    }

    /// Asks first: the headphones stop playing everywhere, and only their
    /// own power button turns them on again.
    private func confirmPowerOff() {
        NSApp.activate()
        let alert = NSAlert()
        alert.messageText = "Turn off \(model.name ?? "your headphones")?"
        alert.informativeText =
            "They'll stop playing on every device they're connected to. To use them again, turn them on with their power button."
        alert.addButton(withTitle: "Turn Off")
        alert.addButton(withTitle: "Cancel")
        if alert.runModal() == .alertFirstButtonReturn { model.change(.powerOff) }
    }

    // MARK: The icon

    /// Updates the icon whenever what it shows changes.
    private func observe() {
        withObservationTracking {
            _ = (model.connection, model.device, model.mode, options.showsBattery)
            _ = (model.battery, model.leftBattery, model.rightBattery)
        } onChange: {
            Task { @MainActor [weak self] in self?.observe() }
        }
        updateIcon()
    }

    /// The menu bar icon: over-ear headphones, distinct from the system's own
    /// headphones symbol, dimmed while no headset is connected, and the
    /// battery level next to it when the user wants that.
    private func updateIcon() {
        let mode = model.device?.modes.first { $0.mode == model.mode }
        let description = mode.map { "Sound Connect: \($0.name)" } ?? "Sound Connect: not connected"
        let image = NSImage(systemSymbolName: "beats.headphones", accessibilityDescription: description)?
            .withSymbolConfiguration(.init(pointSize: 15, weight: .regular))
        image?.isTemplate = true
        guard let button = statusItem.button else { return }
        button.image = image
        button.appearsDisabled = model.device == nil

        let battery = options.showsBattery ? model.lowestBattery : nil
        statusItem.length = battery == nil ? NSStatusItem.squareLength : NSStatusItem.variableLength
        button.imagePosition = battery == nil ? .imageOnly : .imageLeading
        button.title = battery.map { "\($0.level)%" } ?? ""
        button.font = .monospacedDigitSystemFont(ofSize: NSFont.systemFontSize, weight: .regular)
    }
}

/// A borderless panel with the menu's material and rounded corners, at the
/// level of menus, that can take keystrokes without activating the app.
private final class MenuPanel: NSPanel {
    private var host: NSView?

    init() {
        super.init(contentRect: .zero, styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: true)
        level = .popUpMenu
        isOpaque = false
        backgroundColor = .clear
        hasShadow = true
        isReleasedWhenClosed = false
        hidesOnDeactivate = false
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .transient, .ignoresCycle]
    }

    override var canBecomeKey: Bool { true }

    /// The size the content wants, laid out first so it's known before the
    /// panel has ever been shown.
    var contentSize: CGSize {
        host?.layoutSubtreeIfNeeded()
        return host?.fittingSize ?? .zero
    }

    /// Shows a SwiftUI view, reporting its size whenever it changes.
    func setContent(_ view: some View, onResize: @escaping (CGSize) -> Void) {
        let background = NSVisualEffectView()
        background.material = .menu
        background.state = .active
        background.wantsLayer = true
        background.layer?.cornerRadius = 12
        background.layer?.masksToBounds = true

        let host = NSHostingView(rootView: view.background(SizeReader(onChange: onResize)))
        // The panel follows the content's size, not the other way round.
        host.sizingOptions = [.intrinsicContentSize]
        host.autoresizingMask = [.width, .height]
        background.addSubview(host)
        contentView = background
        self.host = host
    }
}

/// Reports the size of what it's the background of.
private struct SizeReader: View {
    let onChange: (CGSize) -> Void

    var body: some View {
        GeometryReader { proxy in
            Color.clear
                .onAppear { onChange(proxy.size) }
                .onChange(of: proxy.size) { _, size in onChange(size) }
        }
    }
}
