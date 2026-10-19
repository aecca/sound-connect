import AppKit
import Observation
import SoundConnectCore
import SwiftUI

/// The menu bar icon and its menu, laid out like the AirPods section of the
/// macOS Bluetooth menu.
@MainActor
final class StatusMenu: NSObject, NSMenuDelegate {
    private static let modes: [(NoiseMode, String, String)] = [
        (.off, "Off", "person.crop.circle"),
        (.ambient, "Ambient Sound", "person.wave.2"),
        (.noiseCancelling, "Noise Cancelling", "person.crop.circle.fill"),
        // Only on models whose profile has wind reduction (WF-1000XM4/XM5).
        (.windReduction, "Wind Noise Reduction", "wind"),
    ]

    private let model: HeadsetModel
    private let statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
    private let menu = NSMenu()

    /// What the menu was last built for; anything else is updated in place,
    /// so an open menu (and a slider being dragged) is never rebuilt.
    private var builtFor: Layout?
    private var modeItems: [NSMenuItem] = []
    private var levelItem: NSMenuItem?
    private var levelView: LevelSliderView?
    private var focusItem: NSMenuItem?
    private var speakToChatItem: NSMenuItem?
    private var equalizerItem: NSMenuItem?
    private var presetItems: [(EqPreset, NSMenuItem)] = []
    private var dseeItem: NSMenuItem?

    private enum Layout: Equatable {
        case controls
        case message(String, connect: [String])
    }

    init(model: HeadsetModel) {
        self.model = model
        super.init()
        menu.delegate = self
        menu.autoenablesItems = false
        statusItem.menu = menu
        observe()
    }

    // MARK: Building

    private var layout: Layout {
        switch model.connection {
        case .connected where model.noise != nil:
            .controls
        case .connected, .connecting:
            .message("Connecting…", connect: [])
        case .unsupported:
            .message("These headphones aren't supported yet", connect: [])
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
    /// headphones symbol, dimmed while no headset is connected.
    private func updateIcon() {
        let mode = model.noise.flatMap { noise in Self.modes.first { $0.0 == noise.mode } }
        let description = mode.map { "Sound Connect: \($0.1)" } ?? "Sound Connect: not connected"
        let image = NSImage(systemSymbolName: "beats.headphones", accessibilityDescription: description)?
            .withSymbolConfiguration(.init(pointSize: 15, weight: .regular))
        image?.isTemplate = true
        statusItem.button?.image = image
        statusItem.button?.appearsDisabled = model.noise == nil
    }

    private func build(_ layout: Layout) {
        builtFor = layout
        menu.removeAllItems()
        modeItems = []
        levelItem = nil
        levelView = nil
        focusItem = nil
        speakToChatItem = nil
        equalizerItem = nil
        presetItems = []
        dseeItem = nil

        // Views inside menus need a real frame; SwiftUI sizes the header itself.
        let header = NSHostingView(rootView: HeaderView(model: model))
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

        case .controls:
            menu.addItem(.sectionHeader(title: "Noise Control"))
            for (index, (mode, title, icon)) in Self.modes.enumerated() {
                let item = actionItem(title, #selector(selectMode(_:)))
                item.tag = index
                item.image = symbol(icon)
                menu.addItem(item)
                modeItems.append(item)

                // Ambient options sit right under Ambient Sound, shown while it's on.
                if mode == .ambient {
                    let slider = LevelSliderView { [weak self] level in self?.model.setAmbientLevel(level) }
                    levelView = slider
                    levelItem = viewItem(slider)
                    menu.addItem(levelItem!)

                    focusItem = actionItem("Focus on Voice", #selector(toggleFocusOnVoice))
                    focusItem!.indentationLevel = 1
                    menu.addItem(focusItem!)
                }
            }

            menu.addItem(.separator())
            speakToChatItem = actionItem("Speak-to-Chat", #selector(toggleSpeakToChat))
            speakToChatItem!.image = symbol("bubble.left.and.bubble.right")
            speakToChatItem!.toolTip = "Switches to ambient sound when you start talking"
            menu.addItem(speakToChatItem!)

            menu.addItem(.separator())
            menu.addItem(.sectionHeader(title: "Sound"))
            equalizerItem = NSMenuItem(title: "Equalizer", action: nil, keyEquivalent: "")
            equalizerItem!.image = symbol("slider.vertical.3")
            equalizerItem!.submenu = equalizerMenu()
            menu.addItem(equalizerItem!)
            dseeItem = actionItem("DSEE Extreme", #selector(toggleDsee))
            dseeItem!.image = symbol("waveform")
            dseeItem!.toolTip = "Restores detail lost in compressed music"
            menu.addItem(dseeItem!)
        }

        menu.addItem(.separator())
        menu.addItem(actionItem("Quit Sound Connect", #selector(quit), key: "q"))
    }

    private func update() {
        guard let noise = model.noise else { return }
        let ambient = noise.mode == .ambient
        for (item, (mode, _, _)) in zip(modeItems, Self.modes) {
            item.state = noise.mode == mode ? .on : .off
            if mode == .windReduction { item.isHidden = model.profile?.windReduction != true }
        }
        levelItem?.isHidden = !ambient
        levelView?.level = noise.ambientLevel
        focusItem?.isHidden = !ambient
        focusItem?.state = noise.focusOnVoice ? .on : .off
        speakToChatItem?.isHidden = model.speakToChat == nil
        speakToChatItem?.state = model.speakToChat == true ? .on : .off
        equalizerItem?.isHidden = model.equalizer == nil
        for (preset, item) in presetItems {
            item.state = model.equalizer?.preset == preset ? .on : .off
        }
        dseeItem?.isHidden = model.dsee == nil
        dseeItem?.state = model.dsee == true ? .on : .off
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

    /// Re-renders whenever anything the menu shows changes.
    private func observe() {
        withObservationTracking {
            _ = (model.connection, model.noise, model.speakToChat, model.disconnectedHeadsets, model.equalizer, model.dsee, model.notice)
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
        model.setMode(Self.modes[item.tag].0)
    }

    @objc private func toggleFocusOnVoice() {
        model.setFocusOnVoice(!(model.noise?.focusOnVoice ?? false))
    }

    @objc private func toggleSpeakToChat() {
        model.setSpeakToChat(!(model.speakToChat ?? false))
    }

    @objc private func selectPreset(_ item: NSMenuItem) {
        if let preset = item.representedObject as? EqPreset { model.setPreset(preset) }
    }

    @objc private func editEqualizer() {
        EqualizerWindow.show(model: model)
    }

    @objc private func toggleDsee() {
        model.setDsee(!(model.dsee ?? false))
    }

    @objc private func connectHeadset(_ item: NSMenuItem) {
        if let name = item.representedObject as? String { model.connect(name) }
    }

    @objc private func quit() {
        NSApp.terminate(nil)
    }

    // MARK: Helpers

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

/// Headphones name, connection details and battery, like the AirPods header.
private struct HeaderView: View {
    let model: HeadsetModel

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
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 6)
        .frame(width: 290, alignment: .leading)
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
            let untested = model.profile.map { !$0.tested } == true ? "Untested model" : nil
            return ["Connected", model.codec, model.firmware.map { "Firmware \($0)" }, untested]
                .compactMap { $0 }
                .joined(separator: " · ")
        }
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
    private static let indent: CGFloat = 57
    private let slider = NSSlider(value: 20, minValue: 1, maxValue: 20, target: nil, action: nil)
    private let onChange: (UInt8) -> Void

    var level: UInt8 {
        get { UInt8(slider.integerValue) }
        set {
            // Don't fight the user's finger while they drag.
            if NSEvent.pressedMouseButtons == 0 { slider.integerValue = Int(newValue) }
        }
    }

    init(onChange: @escaping (UInt8) -> Void) {
        self.onChange = onChange
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
