import AppKit
import Observation
import ServiceManagement
import SoundConnectCore
import SwiftUI

/// The menu bar icon and its menu, laid out like the AirPods section of the
/// macOS Bluetooth menu. Which items it has comes from the headset's
/// `Device`, so it works for any brand.
@MainActor
final class StatusMenu: NSObject, NSMenuDelegate {
    private let model: HeadsetModel
    private let updater: Updater
    private let calls: Calls
    private let shortcut: ModeShortcut
    private let batteryAlerts: BatteryAlerts
    private let statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
    private let menu = NSMenu()

    /// What the menu was last built for; anything else is updated in place,
    /// so an open menu (and a slider being dragged) is never rebuilt.
    private var builtFor: Layout?
    private var modeItems: [(ListeningMode, NSMenuItem)] = []
    private var levelItem: NSMenuItem?
    private var levelView: LevelSliderView?
    private var focusItem: NSMenuItem?
    private var callsItem: NSMenuItem?
    private var featureItems: [(Feature, NSMenuItem)] = []
    private var chatOptionsItem: NSMenuItem?
    private var sensitivityItems: [(VoiceSensitivity, NSMenuItem)] = []
    private var chatEndItems: [(SpeakToChatEnd, NSMenuItem)] = []
    private var presetItems: [(EqPreset, NSMenuItem)] = []
    private var autoOffItem: NSMenuItem?
    private var autoOffItems: [(AutoOff, NSMenuItem)] = []
    private var noiseButtonItem: NSMenuItem?
    private var noiseButtonItems: [(NoiseButton, NSMenuItem)] = []
    private var shortcutItem: NSMenuItem?
    private var batteryAlertsItem: NSMenuItem?
    private var batteryInMenuBarItem: NSMenuItem?
    /// Shows the battery level next to the icon. Off at first: it makes the
    /// icon wider, and macOS hides icons that don't fit beside the notch.
    private var showsBattery = UserDefaults.standard.bool(forKey: StatusMenu.showsBatteryKey) {
        didSet { UserDefaults.standard.set(showsBattery, forKey: Self.showsBatteryKey) }
    }
    private static let showsBatteryKey = "ShowBatteryInMenuBar"
    private var openAtLoginItem: NSMenuItem?

    private enum Layout: Equatable {
        case controls(Device)
        case message(String, connect: [String])
    }

    init(model: HeadsetModel, updater: Updater, calls: Calls, shortcut: ModeShortcut, batteryAlerts: BatteryAlerts) {
        self.model = model
        self.updater = updater
        self.calls = calls
        self.shortcut = shortcut
        self.batteryAlerts = batteryAlerts
        super.init()
        menu.delegate = self
        menu.autoenablesItems = false
        statusItem.menu = menu
        observe()
    }

    /// Shows the menu at a point on screen, e.g. for snapshots, whether or
    /// not the menu bar icon is visible; returns once it closes. Menus follow
    /// the system's light or dark mode unless given an appearance.
    func popUp(at point: NSPoint, appearance: NSAppearance?) {
        menu.appearance = appearance
        menu.popUp(positioning: nil, at: point, in: nil)
    }

    func close() {
        menu.cancelTracking()
    }

    /// Opens the menu under its icon or, when macOS hides the icon because the
    /// menu bar is full, where the icon would be: just right of the camera
    /// notch, or at the top right of screens without one.
    func show() {
        NSApp.activate()
        if let button = statusItem.button, button.window?.occlusionState.contains(.visible) == true {
            button.performClick(nil)
            return
        }
        guard let screen = NSScreen.main else { return }
        let left = screen.auxiliaryTopRightArea?.minX ?? screen.frame.maxX - menu.size.width - 8
        menu.popUp(positioning: nil, at: NSPoint(x: left, y: screen.visibleFrame.maxY), in: nil)
    }

    // MARK: Building

    private var layout: Layout {
        switch model.connection {
        case .connected:
            if let device = model.device { .controls(device) } else { .message("Connecting…", connect: []) }
        case .connecting:
            .message("Connecting…", connect: [])
        case .unsupported(_, let reason):
            .message(reason, connect: [])
        case .searching:
            .message(model.notice ?? "Looking for your headphones…", connect: model.disconnectedHeadsets)
        }
    }

    private func refresh() {
        if layout != builtFor { build(layout) }
        update()
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

        let battery = showsBattery ? model.lowestBattery : nil
        statusItem.length = battery == nil ? NSStatusItem.squareLength : NSStatusItem.variableLength
        button.imagePosition = battery == nil ? .imageOnly : .imageLeading
        button.title = battery.map { "\($0.level)%" } ?? ""
        button.font = .monospacedDigitSystemFont(ofSize: NSFont.systemFontSize, weight: .regular)
    }

    private func build(_ layout: Layout) {
        builtFor = layout
        menu.removeAllItems()
        modeItems = []
        levelItem = nil
        levelView = nil
        focusItem = nil
        callsItem = nil
        featureItems = []
        chatOptionsItem = nil
        sensitivityItems = []
        chatEndItems = []
        presetItems = []
        autoOffItem = nil
        autoOffItems = []
        noiseButtonItem = nil
        noiseButtonItems = []

        // Views inside menus need a real frame; SwiftUI sizes the header itself.
        let header = NSHostingView(rootView: HeaderView(model: model, updater: updater) { [weak self] in
            self?.menu.cancelTracking()
            self?.updater.install()
        })
        header.frame = NSRect(origin: .zero, size: header.fittingSize)
        menu.addItem(viewItem(header))
        menu.addItem(.separator())

        switch layout {
        case .message(let text, let connect):
            let item = NSMenuItem(title: text, action: nil, keyEquivalent: "")
            item.isEnabled = false
            menu.addItem(item)
            for name in connect {
                let item = actionItem("Connect \(name)", #selector(connectHeadset(_:)))
                item.representedObject = name
                item.image = symbol("headphones")
                menu.addItem(item)
            }

        case .controls(let device):
            addModes(of: device)
            if !device.features.isEmpty {
                menu.addItem(.separator())
                for feature in device.features {
                    let item = actionItem(featureName(feature: feature), #selector(toggleFeature(_:)))
                    item.representedObject = feature
                    item.image = symbol(Self.icon(for: feature))
                    item.toolTip = featureDescription(feature: feature)
                    menu.addItem(item)
                    featureItems.append((feature, item))
                }
                if device.speakToChatOptions {
                    let item = NSMenuItem(title: "Speak-to-Chat Options", action: nil, keyEquivalent: "")
                    item.indentationLevel = 1
                    item.submenu = speakToChatMenu()
                    chatOptionsItem = item
                    menu.addItem(item)
                }
            }
            if device.equalizer != nil {
                menu.addItem(.separator())
                let item = NSMenuItem(title: "Equalizer", action: nil, keyEquivalent: "")
                item.image = symbol("slider.vertical.3")
                item.submenu = equalizerMenu()
                menu.addItem(item)
            }
            if !device.autoOff.isEmpty {
                let item = NSMenuItem(title: "Turn Off Automatically", action: nil, keyEquivalent: "")
                item.image = symbol("moon.zzz")
                item.submenu = autoOffMenu(device.autoOff)
                autoOffItem = item
                menu.addItem(item)
            }
            if !device.noiseButton.isEmpty {
                let item = NSMenuItem(title: "Noise Control Button", action: nil, keyEquivalent: "")
                item.image = symbol("button.horizontal.top.press")
                item.toolTip = "What the button on your headphones switches between"
                item.submenu = noiseButtonMenu(device.noiseButton)
                noiseButtonItem = item
                menu.addItem(item)
            }
        }

        menu.addItem(.separator())
        shortcutItem = actionItem(ModeShortcut.title, #selector(toggleShortcut))
        shortcutItem?.toolTip = "Press Control-Option-Command-N in any app to switch to the next mode."
        menu.addItem(shortcutItem!)
        batteryInMenuBarItem = actionItem("Show Battery in Menu Bar", #selector(toggleBatteryInMenuBar))
        menu.addItem(batteryInMenuBarItem!)
        batteryAlertsItem = actionItem("Low Battery Alerts", #selector(toggleBatteryAlerts))
        batteryAlertsItem?.toolTip = "A notification when the battery is down to 20%, and again at 10%."
        menu.addItem(batteryAlertsItem!)
        openAtLoginItem = actionItem("Open at Login", #selector(toggleOpenAtLogin))
        menu.addItem(openAtLoginItem!)
        menu.addItem(actionItem("Quit Sound Connect", #selector(quit), key: "q"))
    }

    private func addModes(of device: Device) {
        guard !device.modes.isEmpty else { return }
        menu.addItem(.sectionHeader(title: "Noise Control"))
        for mode in device.modes {
            let item = actionItem(mode.name, #selector(selectMode(_:)))
            item.representedObject = mode.mode
            item.image = symbol(Self.icon(for: mode.mode))
            menu.addItem(item)
            modeItems.append((mode.mode, item))

            // Ambient options sit right under their mode, shown while it's on.
            if mode.mode == .ambient, let ambient = device.ambient {
                let slider = LevelSliderView(range: ambient.minLevel...ambient.maxLevel) { [weak self] level in
                    self?.model.change(.ambientLevel(level: level))
                }
                levelView = slider
                let levelItem = viewItem(slider)
                self.levelItem = levelItem
                menu.addItem(levelItem)

                if ambient.focusOnVoice {
                    let focusItem = actionItem("Focus on Voice", #selector(toggleFocusOnVoice))
                    focusItem.indentationLevel = 1
                    self.focusItem = focusItem
                    menu.addItem(focusItem)
                }
            }
        }

        if Calls.isSupported, device.modes.contains(where: { $0.mode == .ambient }) {
            let item = actionItem("Ambient Sound During Calls", #selector(toggleCalls))
            item.image = symbol("phone.and.waveform")
            item.toolTip = "While an app uses the microphone, as in a call, switches to Ambient Sound so you hear yourself."
            callsItem = item
            menu.addItem(item)
        }
    }

    private func update() {
        openAtLoginItem?.state = SMAppService.mainApp.status == .enabled ? .on : .off
        shortcutItem?.state = shortcut.isOn ? .on : .off
        batteryAlertsItem?.state = batteryAlerts.isOn ? .on : .off
        batteryInMenuBarItem?.state = showsBattery ? .on : .off
        for (mode, item) in modeItems {
            item.state = model.mode == mode ? .on : .off
        }
        let ambient = model.mode == .ambient
        levelItem?.isHidden = !ambient
        if let level = model.ambientLevel { levelView?.level = level }
        focusItem?.isHidden = !ambient
        focusItem?.state = model.focusOnVoice == true ? .on : .off
        callsItem?.state = calls.isOn ? .on : .off
        for (feature, item) in featureItems {
            // Hidden until the headset has reported it.
            item.isHidden = model.features[feature] == nil
            item.state = model.features[feature] == true ? .on : .off
        }
        chatOptionsItem?.isHidden = model.voiceSensitivity == nil || model.features[.speakToChat] != true
        for (sensitivity, item) in sensitivityItems {
            item.state = model.voiceSensitivity == sensitivity ? .on : .off
        }
        for (end, item) in chatEndItems {
            item.state = model.speakToChatEnd == end ? .on : .off
        }
        for (preset, item) in presetItems {
            item.state = model.equalizer?.preset == preset ? .on : .off
        }
        // Hidden until the headset has reported it, like the features.
        autoOffItem?.isHidden = model.autoOff == nil
        for (choice, item) in autoOffItems {
            item.state = model.autoOff == choice ? .on : .off
        }
        noiseButtonItem?.isHidden = model.noiseButton == nil
        for (choice, item) in noiseButtonItems {
            item.state = model.noiseButton == choice ? .on : .off
        }
    }

    private func equalizerMenu() -> NSMenu {
        let submenu = NSMenu()
        submenu.autoenablesItems = false
        for preset in eqPresets() {
            let item = actionItem(eqPresetName(preset: preset), #selector(selectPreset(_:)))
            item.representedObject = preset
            submenu.addItem(item)
            presetItems.append((preset, item))
        }
        submenu.addItem(.separator())
        submenu.addItem(actionItem("Edit Manual Equalizer…", #selector(editEqualizer)))
        return submenu
    }

    private func autoOffMenu(_ choices: [AutoOff]) -> NSMenu {
        let submenu = NSMenu()
        submenu.autoenablesItems = false
        for choice in choices {
            let item = actionItem(autoOffName(autoOff: choice), #selector(selectAutoOff(_:)))
            item.representedObject = choice
            submenu.addItem(item)
            autoOffItems.append((choice, item))
        }
        return submenu
    }

    private func speakToChatMenu() -> NSMenu {
        let submenu = NSMenu()
        submenu.autoenablesItems = false
        submenu.addItem(.sectionHeader(title: "Sensitivity"))
        for sensitivity in voiceSensitivities() {
            let item = actionItem(voiceSensitivityName(sensitivity: sensitivity), #selector(selectSensitivity(_:)))
            item.representedObject = sensitivity
            submenu.addItem(item)
            sensitivityItems.append((sensitivity, item))
        }
        submenu.addItem(.separator())
        submenu.addItem(.sectionHeader(title: "Back to the Previous Mode"))
        for end in speakToChatEnds() {
            let item = actionItem(speakToChatEndName(end: end), #selector(selectChatEnd(_:)))
            item.representedObject = end
            submenu.addItem(item)
            chatEndItems.append((end, item))
        }
        return submenu
    }

    private func noiseButtonMenu(_ choices: [NoiseButton]) -> NSMenu {
        let submenu = NSMenu()
        submenu.autoenablesItems = false
        for choice in choices {
            let item = actionItem(noiseButtonName(choice: choice), #selector(selectNoiseButton(_:)))
            item.representedObject = choice
            submenu.addItem(item)
            noiseButtonItems.append((choice, item))
        }
        return submenu
    }

    /// Re-renders whenever anything the menu shows changes.
    private func observe() {
        withObservationTracking {
            _ = (model.connection, model.device, model.mode, model.ambientLevel, model.focusOnVoice)
            _ = (model.features, model.autoOff, model.noiseButton, model.equalizer, model.disconnectedHeadsets)
            _ = (model.voiceSensitivity, model.speakToChatEnd, model.notice)
            _ = (model.battery, model.leftBattery, model.rightBattery)
        } onChange: {
            Task { @MainActor [weak self] in
                self?.refresh()
                self?.observe()
            }
        }
        refresh()
    }

    // MARK: Actions

    @objc private func selectMode(_ item: NSMenuItem) {
        if let mode = item.representedObject as? ListeningMode { model.change(.mode(mode: mode)) }
    }

    @objc private func toggleFocusOnVoice() {
        model.change(.focusOnVoice(on: model.focusOnVoice != true))
    }

    @objc private func toggleCalls() {
        calls.isOn.toggle()
        update()
    }

    @objc private func toggleFeature(_ item: NSMenuItem) {
        guard let feature = item.representedObject as? Feature else { return }
        model.change(.feature(feature: feature, on: model.features[feature] != true))
    }

    @objc private func selectPreset(_ item: NSMenuItem) {
        if let preset = item.representedObject as? EqPreset { model.change(.equalizerPreset(preset: preset)) }
    }

    @objc private func selectAutoOff(_ item: NSMenuItem) {
        if let choice = item.representedObject as? AutoOff { model.change(.autoOff(autoOff: choice)) }
    }

    @objc private func selectSensitivity(_ item: NSMenuItem) {
        if let sensitivity = item.representedObject as? VoiceSensitivity {
            model.change(.voiceSensitivity(sensitivity: sensitivity))
        }
    }

    @objc private func selectChatEnd(_ item: NSMenuItem) {
        if let end = item.representedObject as? SpeakToChatEnd { model.change(.speakToChatEnd(end: end)) }
    }

    @objc private func selectNoiseButton(_ item: NSMenuItem) {
        if let choice = item.representedObject as? NoiseButton { model.change(.noiseButton(choice: choice)) }
    }

    @objc private func editEqualizer() {
        EqualizerWindow.show(model: model)
    }

    @objc private func connectHeadset(_ item: NSMenuItem) {
        if let name = item.representedObject as? String { model.connect(name) }
    }

    @objc private func toggleShortcut() {
        shortcut.isOn.toggle()
        update()
    }

    @objc private func toggleBatteryInMenuBar() {
        showsBattery.toggle()
        update()
        updateIcon()
    }

    @objc private func toggleBatteryAlerts() {
        batteryAlerts.isOn.toggle()
        update()
    }

    @objc private func toggleOpenAtLogin() {
        LoginItem.set(SMAppService.mainApp.status != .enabled)
        update()
    }

    @objc private func quit() {
        NSApp.terminate(nil)
    }

    // MARK: Helpers

    private static func icon(for mode: ListeningMode) -> String {
        switch mode {
        case .off: "person.crop.circle"
        case .ambient: "person.wave.2"
        case .noiseCancelling: "person.crop.circle.fill"
        case .noiseCancellingLow: "person.crop.circle.badge.minus"
        case .windReduction: "wind"
        }
    }

    private static func icon(for feature: Feature) -> String {
        switch feature {
        case .speakToChat: "bubble.left.and.bubble.right"
        case .dsee: "waveform"
        case .voicePrompts: "speaker.wave.2.bubble"
        case .pauseWhenTakenOff: "pause.circle"
        case .actionButtonNoiseCancelling: "button.programmable"
        }
    }

    private func actionItem(_ title: String, _ action: Selector, key: String = "") -> NSMenuItem {
        let item = NSMenuItem(title: title, action: action, keyEquivalent: key)
        item.target = self
        return item
    }

    private func viewItem(_ view: NSView) -> NSMenuItem {
        let item = NSMenuItem()
        item.view = view
        return item
    }

    private func symbol(_ name: String) -> NSImage? {
        NSImage(systemSymbolName: name, accessibilityDescription: nil)?
            .withSymbolConfiguration(.init(pointSize: 15, weight: .regular))
    }
}

/// Headphones name, connection details and battery, like the AirPods header,
/// and an Update button while a new version is out.
private struct HeaderView: View {
    let model: HeadsetModel
    let updater: Updater
    let update: () -> Void

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: "headphones")
                .font(.system(size: 14, weight: .medium))
                .foregroundStyle(.white)
                .frame(width: 30, height: 30)
                .background(Circle().fill(isConnected ? Color.accentColor : Color.secondary.opacity(0.5)))

            VStack(alignment: .leading, spacing: 1) {
                Text(model.name ?? "Sound Connect")
                    .font(.system(size: 13, weight: .semibold))
                Text(details)
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
            }

            Spacer(minLength: 16)

            if let battery = model.battery {
                BatteryLabel(title: nil, battery: battery)
            }
            // Earbuds: each side and the case, like the AirPods header.
            HStack(spacing: 8) {
                if let left = model.leftBattery { BatteryLabel(title: "L", battery: left) }
                if let right = model.rightBattery { BatteryLabel(title: "R", battery: right) }
                if let charging = model.caseBattery { BatteryLabel(title: "Case", battery: charging) }
            }

            switch updater.status {
            case .upToDate: EmptyView()
            case .available(let version):
                Button("Update", action: update).buttonStyle(PillButtonStyle()).help("Install Sound Connect \(version)")
            case .installing:
                Text("Updating…").font(.system(size: 11)).foregroundStyle(.secondary)
            case .failed(let version):
                Button("Try Again", action: update).buttonStyle(PillButtonStyle())
                    .help("Sound Connect \(version) didn't install. Details are in ~/Library/Logs/Sound Connect/update.log.")
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 6)
        .frame(width: 290, alignment: .leading)
        .help(model.firmware.map { "Firmware \($0)" } ?? "")
    }

    private var isConnected: Bool {
        if case .connected = model.connection { true } else { false }
    }

    private var details: String {
        switch model.connection {
        case .searching: return "Not connected"
        case .connecting: return "Connecting…"
        case .unsupported: return "Not supported yet"
        case .connected:
            // Be upfront when this model's support hasn't been tried yet.
            let untested = model.device.map { !$0.tested } == true ? "Untested model" : nil
            return ["Connected", model.codec, untested]
                .compactMap { $0 }
                .joined(separator: " · ")
        }
    }
}

/// A small blue capsule button, like Update in the App Store.
private struct PillButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 11, weight: .semibold))
            .foregroundStyle(.white)
            .padding(.horizontal, 10)
            .padding(.vertical, 3)
            .background(Capsule().fill(Color.accentColor.opacity(configuration.isPressed ? 0.75 : 1)))
    }
}

/// A battery level with the system's battery symbol, optionally titled
/// ("L", "R", "Case") for earbuds.
private struct BatteryLabel: View {
    let title: String?
    let battery: Battery

    var body: some View {
        HStack(spacing: 4) {
            Text([title, "\(battery.level)%"].compactMap { $0 }.joined(separator: " "))
                .font(.system(size: title == nil ? 13 : 11))
                .foregroundStyle(.secondary)
            Image(systemName: batterySymbol(battery))
                .font(.system(size: title == nil ? 15 : 12))
                .foregroundStyle(battery.charging ? .green : battery.level <= 20 ? .red : .secondary)
        }
    }

    private func batterySymbol(_ battery: Battery) -> String {
        if battery.charging { return "battery.100percent.bolt" }
        switch battery.level {
        case ..<13: return "battery.0percent"
        case ..<38: return "battery.25percent"
        case ..<63: return "battery.50percent"
        case ..<88: return "battery.75percent"
        default: return "battery.100percent"
        }
    }
}

/// The "Ambient Sound Level" row: a label and a slider, like the volume
/// slider in the Sound menu.
private final class LevelSliderView: NSView {
    /// Lines up with the indented "Focus on Voice" item below it.
    private static let indent: CGFloat = 62
    private let slider: NSSlider
    private let onChange: (UInt8) -> Void

    var level: UInt8 {
        get { UInt8(slider.integerValue) }
        set {
            // Don't fight the user's finger while they drag.
            if NSEvent.pressedMouseButtons == 0 { slider.integerValue = Int(newValue) }
        }
    }

    init(range: ClosedRange<UInt8>, onChange: @escaping (UInt8) -> Void) {
        self.onChange = onChange
        slider = NSSlider(
            value: Double(range.upperBound), minValue: Double(range.lowerBound), maxValue: Double(range.upperBound),
            target: nil, action: nil)
        super.init(frame: NSRect(x: 0, y: 0, width: 290, height: 46))

        let label = NSTextField(labelWithString: "Ambient Sound Level")
        label.font = .menuFont(ofSize: 0)
        label.textColor = .secondaryLabelColor
        slider.isContinuous = true
        slider.target = self
        slider.action = #selector(changed)

        for view in [label, slider] {
            view.translatesAutoresizingMaskIntoConstraints = false
            addSubview(view)
        }
        NSLayoutConstraint.activate([
            label.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Self.indent),
            label.topAnchor.constraint(equalTo: topAnchor, constant: 4),
            slider.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Self.indent),
            slider.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -16),
            slider.topAnchor.constraint(equalTo: label.bottomAnchor, constant: 4),
        ])
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    @objc private func changed() {
        onChange(UInt8(slider.integerValue))
    }
}
