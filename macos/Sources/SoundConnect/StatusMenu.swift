import AppKit
import Observation
import ServiceManagement
import SoundConnectCore
import SwiftUI

/// The menu bar icon and its menu, laid out like the AirPods section of the
/// macOS Bluetooth menu. Which items it has comes from the headset's
/// `Device`, so it works for any brand. What's used most sits at the top;
/// the rest is in "Headphone Settings" and "Sound Connect Settings".
@MainActor
final class StatusMenu: NSObject, NSMenuDelegate {
    /// Kept in the menu itself; the others go in "Headphone Settings".
    private static let mainFeatures: [Feature] = [.speakToChat, .dsee]

    private let model: HeadsetModel
    private let updater: Updater
    private let calls: Calls
    private let shortcut: ModeShortcut
    private let batteryAlerts: BatteryAlerts
    private let popUp: ModePopUp
    private let statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
    private let menu = NSMenu()

    /// What the menu was last built for; anything else is updated in place,
    /// so an open menu (and a slider being dragged) is never rebuilt.
    private var builtFor: Layout?
    private var levelItem: NSMenuItem?
    private var levelView: LevelSliderView?
    private var focusItem: NSMenuItem?
    private var focusRow: SwitchRowView?
    private var callsRow: SwitchRowView?
    private var featureItems: [(Feature, NSMenuItem)] = []
    private var equalizerItem: NSMenuItem?
    private var headphoneSettingsItem: NSMenuItem?
    private var chatOptionsItem: NSMenuItem?
    private var sensitivityItems: [(VoiceSensitivity, NSMenuItem)] = []
    private var chatEndItems: [(SpeakToChatEnd, NSMenuItem)] = []
    private var presetItems: [(EqPreset, NSMenuItem)] = []
    private var autoOffItem: NSMenuItem?
    private var autoOffItems: [(AutoOff, NSMenuItem)] = []
    private var playFromItem: NSMenuItem?
    /// The devices "Play Audio From" lists, and their items.
    private var playFromItems: [(ConnectedDevice, NSMenuItem)] = []
    private var noiseButtonItem: NSMenuItem?
    private var noiseButtonItems: [(NoiseButton, NSMenuItem)] = []
    private var shortcutItem: NSMenuItem?
    private var batteryAlertsItem: NSMenuItem?
    private var batteryInMenuBarItem: NSMenuItem?
    private var popUpItem: NSMenuItem?
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

    init(
        model: HeadsetModel, updater: Updater, calls: Calls, shortcut: ModeShortcut, batteryAlerts: BatteryAlerts,
        popUp: ModePopUp
    ) {
        self.model = model
        self.updater = updater
        self.calls = calls
        self.shortcut = shortcut
        self.batteryAlerts = batteryAlerts
        self.popUp = popUp
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

    // Changes made while the menu is open show in it; the pop-up is for the rest.
    func menuWillOpen(_ menu: NSMenu) {
        popUp.menuIsOpen = true
    }

    func menuDidClose(_ menu: NSMenu) {
        popUp.menuIsOpen = false
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
        levelItem = nil
        levelView = nil
        focusItem = nil
        focusRow = nil
        callsRow = nil
        featureItems = []
        equalizerItem = nil
        headphoneSettingsItem = nil
        chatOptionsItem = nil
        sensitivityItems = []
        chatEndItems = []
        presetItems = []
        autoOffItem = nil
        autoOffItems = []
        playFromItem = nil
        playFromItems = []
        noiseButtonItem = nil
        noiseButtonItems = []

        // Views inside menus need a real frame; SwiftUI sizes them itself.
        menu.addItem(hostingItem(HeaderView(model: model, updater: updater) { [weak self] in
            self?.menu.cancelTracking()
            self?.updater.install()
        }))
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
            let features = device.features.filter(Self.mainFeatures.contains)
            if !features.isEmpty || device.equalizer != nil {
                menu.addItem(.separator())
            }
            for feature in features {
                menu.addItem(featureItem(feature))
            }
            if device.equalizer != nil {
                let item = NSMenuItem(title: "Equalizer", action: nil, keyEquivalent: "")
                item.image = symbol("slider.vertical.3")
                item.submenu = equalizerMenu()
                equalizerItem = item
                menu.addItem(item)
            }
            menu.addItem(.separator())
            let settings = NSMenuItem(title: "Headphone Settings", action: nil, keyEquivalent: "")
            settings.image = symbol("headphones")
            settings.submenu = headphoneSettingsMenu(device)
            headphoneSettingsItem = settings
            menu.addItem(settings)
        }

        menu.addItem(.separator())
        let appSettings = NSMenuItem(title: "Sound Connect Settings", action: nil, keyEquivalent: "")
        appSettings.image = symbol("gearshape")
        appSettings.submenu = appSettingsMenu()
        menu.addItem(appSettings)
        menu.addItem(actionItem("Quit Sound Connect", #selector(quit), key: "q"))
    }

    private func addModes(of device: Device) {
        guard !device.modes.isEmpty else { return }
        menu.addItem(hostingItem(ModeButtons(model: model)))

        // Ambient options sit right under the buttons, shown while it's on.
        if let ambient = device.ambient {
            let slider = LevelSliderView(range: ambient.minLevel...ambient.maxLevel) { [weak self] level in
                self?.model.change(.ambientLevel(level: level))
            }
            levelView = slider
            let levelItem = viewItem(slider)
            self.levelItem = levelItem
            menu.addItem(levelItem)

            if ambient.focusOnVoice {
                let row = SwitchRowView(title: "Focus on Voice") { [weak self] on in
                    self?.model.change(.focusOnVoice(on: on))
                }
                focusRow = row
                let focusItem = viewItem(row)
                self.focusItem = focusItem
                menu.addItem(focusItem)
            }
        }

        if Calls.isSupported, device.modes.contains(where: { $0.mode == .ambient }) {
            let row = SwitchRowView(
                title: "Ambient Sound During Calls",
                toolTip: "While an app uses the microphone, as in a call, switches to Ambient Sound so you hear yourself."
            ) { [weak self] on in
                self?.calls.isOn = on
            }
            callsRow = row
            menu.addItem(viewItem(row))
        }
    }

    /// Settings changed now and then: the rest of the features, how they
    /// behave, and turning the headphones off.
    private func headphoneSettingsMenu(_ device: Device) -> NSMenu {
        let submenu = NSMenu()
        submenu.autoenablesItems = false
        for feature in device.features where !Self.mainFeatures.contains(feature) {
            submenu.addItem(featureItem(feature))
        }
        if device.features.contains(.speakToChat), device.speakToChatOptions {
            let item = NSMenuItem(title: "Speak-to-Chat Options", action: nil, keyEquivalent: "")
            item.image = symbol(Self.icon(for: .speakToChat))
            item.submenu = speakToChatMenu()
            chatOptionsItem = item
            submenu.addItem(item)
        }
        if !device.autoOff.isEmpty {
            let item = NSMenuItem(title: "Turn Off Automatically", action: nil, keyEquivalent: "")
            item.image = symbol("moon.zzz")
            item.submenu = autoOffMenu(device.autoOff)
            autoOffItem = item
            submenu.addItem(item)
        }
        if !device.noiseButton.isEmpty {
            let item = NSMenuItem(title: "Noise Control Button", action: nil, keyEquivalent: "")
            item.image = symbol("button.horizontal.top.press")
            item.toolTip = "What the button on your headphones switches between"
            item.submenu = noiseButtonMenu(device.noiseButton)
            noiseButtonItem = item
            submenu.addItem(item)
        }
        // Which of the devices the headset is connected to it plays; shown
        // once it's connected to more than this Mac.
        let playFrom = NSMenuItem(title: "Play Audio From", action: nil, keyEquivalent: "")
        playFrom.image = symbol("laptopcomputer.and.iphone")
        playFrom.submenu = NSMenu()
        playFrom.submenu?.autoenablesItems = false
        playFromItem = playFrom
        submenu.addItem(playFrom)
        if device.powerOff {
            submenu.addItem(.separator())
            let item = actionItem("Turn Off Headphones…", #selector(powerOff))
            item.image = symbol("power")
            submenu.addItem(item)
        }
        return submenu
    }

    private func appSettingsMenu() -> NSMenu {
        let submenu = NSMenu()
        submenu.autoenablesItems = false
        shortcutItem = actionItem(ModeShortcut.title, #selector(toggleShortcut))
        shortcutItem?.toolTip = "Press Control-Option-Command-N in any app to switch to the next mode."
        submenu.addItem(shortcutItem!)
        batteryInMenuBarItem = actionItem("Show Battery in Menu Bar", #selector(toggleBatteryInMenuBar))
        submenu.addItem(batteryInMenuBarItem!)
        batteryAlertsItem = actionItem("Low Battery Alerts", #selector(toggleBatteryAlerts))
        batteryAlertsItem?.toolTip = "A notification when the battery is down to 20%, and again at 10% and 5%."
        submenu.addItem(batteryAlertsItem!)
        popUpItem = actionItem(ModePopUp.title, #selector(togglePopUp))
        popUpItem?.toolTip = "Shows the new mode when you change it with the button on your headphones or a shortcut."
        submenu.addItem(popUpItem!)
        openAtLoginItem = actionItem("Open at Login", #selector(toggleOpenAtLogin))
        submenu.addItem(openAtLoginItem!)
        return submenu
    }

    private func update() {
        openAtLoginItem?.state = SMAppService.mainApp.status == .enabled ? .on : .off
        shortcutItem?.state = shortcut.isOn ? .on : .off
        batteryAlertsItem?.state = batteryAlerts.isOn ? .on : .off
        batteryInMenuBarItem?.state = showsBattery ? .on : .off
        popUpItem?.state = popUp.isOn ? .on : .off
        let ambient = model.mode == .ambient
        levelItem?.isHidden = !ambient
        if let level = model.ambientLevel { levelView?.level = level }
        focusItem?.isHidden = !ambient
        focusRow?.isOn = model.focusOnVoice == true
        callsRow?.isOn = calls.isOn
        for (feature, item) in featureItems {
            // Hidden until the headset has reported it.
            item.isHidden = model.features[feature] == nil
            item.state = model.features[feature] == true ? .on : .off
        }
        equalizerItem?.badge = model.equalizer.map { NSMenuItemBadge(string: eqPresetName(preset: $0.preset)) }
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
        updatePlayFrom()
        noiseButtonItem?.isHidden = model.noiseButton == nil
        for (choice, item) in noiseButtonItems {
            item.state = model.noiseButton == choice ? .on : .off
        }
        if let headphoneSettingsItem, let submenu = headphoneSettingsItem.submenu {
            let items = submenu.items.filter { !$0.isSeparatorItem }
            headphoneSettingsItem.isHidden = items.allSatisfy(\.isHidden)
            // "Turn Off Headphones…" is set apart, unless it's alone.
            let separator = submenu.items.first(where: \.isSeparatorItem)
            separator?.isHidden = items.dropLast().allSatisfy(\.isHidden)
        }
    }

    private func featureItem(_ feature: Feature) -> NSMenuItem {
        let item = actionItem(featureName(feature: feature), #selector(toggleFeature(_:)))
        item.representedObject = feature
        item.image = symbol(Self.icon(for: feature))
        item.toolTip = featureDescription(feature: feature)
        featureItems.append((feature, item))
        return item
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

    /// The connected devices change while the rest of the menu stays as it
    /// is, so their items are remade when another device comes or goes.
    private func updatePlayFrom() {
        guard let playFromItem, let submenu = playFromItem.submenu else { return }
        let devices = model.connectedDevices
        playFromItem.isHidden = devices.count < 2
        let listed = playFromItems.map { ($0.0.id, $0.0.name) }
        if listed.map(\.0) != devices.map(\.id) || listed.map(\.1) != devices.map(\.name) {
            submenu.removeAllItems()
            playFromItems = devices.map { device in
                let item = actionItem(device.name, #selector(playFrom(_:)))
                item.representedObject = device.id
                submenu.addItem(item)
                return (device, item)
            }
        }
        for (device, item) in playFromItems {
            item.state = devices.first { $0.id == device.id }?.playing == true ? .on : .off
        }
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
            _ = (model.battery, model.leftBattery, model.rightBattery, model.connectedDevices)
        } onChange: {
            Task { @MainActor [weak self] in
                self?.refresh()
                self?.observe()
            }
        }
        refresh()
    }

    // MARK: Actions

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

    @objc private func playFrom(_ item: NSMenuItem) {
        if let id = item.representedObject as? String { model.change(.playFrom(id: id)) }
    }

    @objc private func selectNoiseButton(_ item: NSMenuItem) {
        if let choice = item.representedObject as? NoiseButton { model.change(.noiseButton(choice: choice)) }
    }

    @objc private func editEqualizer() {
        EqualizerWindow.show(model: model)
    }

    /// Asks first: the headphones stop playing everywhere, and only their
    /// own power button turns them on again.
    @objc private func powerOff() {
        NSApp.activate()
        let alert = NSAlert()
        alert.messageText = "Turn off \(model.name ?? "your headphones")?"
        alert.informativeText =
            "They'll stop playing on every device they're connected to. To use them again, turn them on with their power button."
        alert.addButton(withTitle: "Turn Off")
        alert.addButton(withTitle: "Cancel")
        if alert.runModal() == .alertFirstButtonReturn { model.change(.powerOff) }
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

    @objc private func togglePopUp() {
        popUp.isOn.toggle()
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

    private func hostingItem(_ view: some View) -> NSMenuItem {
        let host = NSHostingView(rootView: view)
        host.frame = NSRect(origin: .zero, size: host.fittingSize)
        return viewItem(host)
    }

    private func symbol(_ name: String) -> NSImage? {
        NSImage(systemSymbolName: name, accessibilityDescription: nil)?
            .withSymbolConfiguration(.init(pointSize: 15, weight: .regular))
    }
}

extension ListeningMode {
    /// Its symbol, in the style of the AirPods noise control icons.
    var symbol: String {
        switch self {
        case .off: "person.crop.circle"
        case .ambient: "person.wave.2"
        case .noiseCancelling: "person.crop.circle.fill"
        case .noiseCancellingLow: "person.crop.circle.badge.minus"
        case .windReduction: "wind"
        }
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

            Spacer(minLength: 12)

            if let battery = model.battery {
                BatteryRing(title: nil, battery: battery)
            }
            // Earbuds: each side and the case, like the AirPods header.
            HStack(spacing: 8) {
                if let left = model.leftBattery { BatteryRing(title: "L", battery: left) }
                if let right = model.rightBattery { BatteryRing(title: "R", battery: right) }
                if let charging = model.caseBattery { BatteryRing(title: "Case", battery: charging) }
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

/// A battery level as a ring that empties, with the level under it, like the
/// Batteries widget; titled ("L", "R", "Case") for earbuds.
private struct BatteryRing: View {
    let title: String?
    let battery: Battery

    var body: some View {
        VStack(spacing: 2) {
            ZStack {
                Circle().stroke(Color.primary.opacity(0.15), lineWidth: 3)
                Circle()
                    .trim(from: 0, to: CGFloat(battery.level) / 100)
                    .stroke(color, style: StrokeStyle(lineWidth: 3, lineCap: .round))
                    .rotationEffect(.degrees(-90))
                if battery.charging {
                    Image(systemName: "bolt.fill").font(.system(size: 9)).foregroundStyle(color)
                }
            }
            .frame(width: 22, height: 22)
            Text([title, "\(battery.level)%"].compactMap { $0 }.joined(separator: " "))
                .font(.system(size: 10).monospacedDigit())
                .foregroundStyle(.secondary)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(
            [title.map { "\($0) battery" } ?? "Battery", "\(battery.level)%", battery.charging ? "charging" : nil]
                .compactMap { $0 }
                .joined(separator: ", "))
    }

    private var color: Color {
        !battery.charging && battery.level <= 20 ? .red : .green
    }
}

/// The listening modes as buttons side by side, like the noise control in
/// Control Center's AirPods module; the one that's on is filled.
private struct ModeButtons: View {
    let model: HeadsetModel

    var body: some View {
        HStack(spacing: 6) {
            ForEach(model.device?.modes ?? [], id: \.mode) { mode in
                let isOn = model.mode == mode.mode
                Button {
                    model.change(.mode(mode: mode.mode))
                } label: {
                    VStack(spacing: 3) {
                        Image(systemName: mode.mode.symbol).font(.system(size: 16))
                        Text(mode.name)
                            .font(.system(size: 10.5))
                            .lineLimit(2)
                            .multilineTextAlignment(.center)
                            .padding(.horizontal, 6)
                    }
                    .frame(maxWidth: .infinity, minHeight: 50)
                    .foregroundStyle(isOn ? Color.white : Color.primary)
                    .background(
                        RoundedRectangle(cornerRadius: 10).fill(isOn ? Color.accentColor : Color.primary.opacity(0.08)))
                    .contentShape(RoundedRectangle(cornerRadius: 10))
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(isOn ? .isSelected : [])
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 4)
        .frame(width: 290)
    }
}

/// The "Ambient Sound" row: its name and level, and a slider, like the
/// volume slider in the Sound menu.
private final class LevelSliderView: NSView {
    private let slider: NSSlider
    private let value = NSTextField(labelWithString: "")
    private let onChange: (UInt8) -> Void

    var level: UInt8 {
        get { UInt8(slider.integerValue) }
        set {
            // Don't fight the user's finger while they drag.
            if NSEvent.pressedMouseButtons == 0 { slider.integerValue = Int(newValue) }
            value.stringValue = "\(slider.integerValue)"
        }
    }

    init(range: ClosedRange<UInt8>, onChange: @escaping (UInt8) -> Void) {
        self.onChange = onChange
        slider = NSSlider(
            value: Double(range.upperBound), minValue: Double(range.lowerBound), maxValue: Double(range.upperBound),
            target: nil, action: nil)
        super.init(frame: NSRect(x: 0, y: 0, width: 290, height: 46))

        let label = NSTextField(labelWithString: "Ambient Sound")
        label.font = .menuFont(ofSize: 0)
        label.textColor = .secondaryLabelColor
        value.font = .monospacedDigitSystemFont(ofSize: NSFont.systemFontSize, weight: .regular)
        slider.isContinuous = true
        slider.target = self
        slider.action = #selector(changed)

        for view in [label, value, slider] {
            view.translatesAutoresizingMaskIntoConstraints = false
            addSubview(view)
        }
        NSLayoutConstraint.activate([
            label.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 14),
            label.topAnchor.constraint(equalTo: topAnchor, constant: 4),
            value.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -14),
            value.firstBaselineAnchor.constraint(equalTo: label.firstBaselineAnchor),
            slider.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 14),
            slider.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -14),
            slider.topAnchor.constraint(equalTo: label.bottomAnchor, constant: 4),
        ])
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    @objc private func changed() {
        value.stringValue = "\(slider.integerValue)"
        onChange(UInt8(slider.integerValue))
    }
}

/// A setting with a switch at the end of its row, like Wi-Fi's in the Wi-Fi
/// menu.
private final class SwitchRowView: NSView {
    private let toggle = NSSwitch()
    private let onChange: (Bool) -> Void

    var isOn: Bool {
        get { toggle.state == .on }
        set { toggle.state = newValue ? .on : .off }
    }

    init(title: String, toolTip: String? = nil, onChange: @escaping (Bool) -> Void) {
        self.onChange = onChange
        super.init(frame: NSRect(x: 0, y: 0, width: 290, height: 26))
        self.toolTip = toolTip

        let label = NSTextField(labelWithString: title)
        label.font = .menuFont(ofSize: 0)
        toggle.controlSize = .mini
        toggle.target = self
        toggle.action = #selector(changed)

        for view in [label, toggle] {
            view.translatesAutoresizingMaskIntoConstraints = false
            addSubview(view)
        }
        NSLayoutConstraint.activate([
            label.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 14),
            label.centerYAnchor.constraint(equalTo: centerYAnchor),
            toggle.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -14),
            toggle.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    @objc private func changed() {
        onChange(isOn)
    }
}
