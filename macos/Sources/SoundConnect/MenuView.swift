import AppKit
import Observation
import SoundConnectCore
import SwiftUI

/// The panel's own settings, which aren't the headset's.
@MainActor
@Observable
final class MenuOptions {
    private static let showsBatteryKey = "ShowBatteryInMenuBar"

    /// Shows the battery level next to the icon. Off at first: it makes the
    /// icon wider, and macOS hides icons that don't fit beside the notch.
    var showsBattery = UserDefaults.standard.bool(forKey: MenuOptions.showsBatteryKey) {
        didSet { UserDefaults.standard.set(showsBattery, forKey: Self.showsBatteryKey) }
    }
    /// Read from the system each time the panel opens.
    var opensAtLogin = false
    /// The titles of the sections and choices that are unfolded.
    var unfolded: Set<String> = []

    func toggle(_ title: String) {
        if unfolded.remove(title) == nil { unfolded.insert(title) }
    }
}

/// What the panel's buttons do besides changing a setting.
struct MenuActions {
    let installUpdate: () -> Void
    let editEqualizer: () -> Void
    let powerOff: () -> Void
    let connect: (String) -> Void
    let openAtLogin: (Bool) -> Void
    let quit: () -> Void
}

/// The panel's content, in the style of the Sound and Wi-Fi menus: every row
/// has a round symbol, blue while it's on or chosen, and its name. Settings
/// changed now and then fold under "Headphone Settings" and "Sound Connect
/// Settings". Which rows there are comes from the headset's `Device`.
struct MenuView: View {
    /// Kept in the panel itself; the others go in "Headphone Settings".
    private static let mainFeatures: [Feature] = [.speakToChat, .dsee]

    let model: HeadsetModel
    let updater: Updater
    let calls: Calls
    let shortcut: ModeShortcut
    let batteryAlerts: BatteryAlerts
    let popUp: ModePopUp
    let options: MenuOptions
    let actions: MenuActions

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HeaderView(model: model, updater: updater, update: actions.installUpdate)
            MenuDivider()
            if let device = connectedDevice {
                controls(device)
            } else {
                Text(message)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                    .rowLayout()
                ForEach(model.disconnectedHeadsets, id: \.self) { name in
                    ActionRow(symbol: "headphones", title: "Connect \(name)") { actions.connect(name) }
                }
            }
            MenuDivider()
            FoldingSection(title: "Sound Connect Settings", options: options) {
                appSettings
            }
            MenuDivider()
            ActionRow(symbol: nil, title: "Quit Sound Connect", key: "⌘Q", action: actions.quit)
                .keyboardShortcut("q")
        }
        .padding(.vertical, 6)
        .frame(width: 300)
        .fixedSize(horizontal: false, vertical: true)
    }

    private var connectedDevice: Device? {
        if case .connected = model.connection { model.device } else { nil }
    }

    private var message: String {
        switch model.connection {
        case .connected, .connecting: "Connecting…"
        case .unsupported(_, let reason): reason
        case .searching: model.notice ?? "Looking for your headphones…"
        }
    }

    @ViewBuilder
    private func controls(_ device: Device) -> some View {
        if !device.modes.isEmpty {
            SectionTitle(title: "Noise Control")
            ForEach(device.modes, id: \.mode) { mode in
                ToggleRow(symbol: mode.mode.symbol, title: mode.name, isOn: model.mode == mode.mode) {
                    model.change(.mode(mode: mode.mode))
                }
                // Ambient options sit right under it, shown while it's on.
                if mode.mode == .ambient, model.mode == .ambient, let ambient = device.ambient {
                    LevelRow(model: model, range: ambient.minLevel...ambient.maxLevel)
                    if ambient.focusOnVoice {
                        ToggleRow(symbol: "waveform.and.mic", title: "Focus on Voice", isOn: model.focusOnVoice == true) {
                            model.change(.focusOnVoice(on: model.focusOnVoice != true))
                        }
                    }
                }
            }
        }
        if Calls.isSupported, device.modes.contains(where: { $0.mode == .ambient }) {
            ToggleRow(symbol: "phone.and.waveform", title: "Ambient Sound During Calls", isOn: calls.isOn) {
                calls.isOn.toggle()
            }
            .help("While an app uses the microphone, as in a call, switches to Ambient Sound so you hear yourself.")
        }

        let features = reported(device.features.filter(Self.mainFeatures.contains))
        if !features.isEmpty || model.equalizer != nil {
            MenuDivider()
        }
        ForEach(features, id: \.self, content: featureRow)
        if device.equalizer != nil, let equalizer = model.equalizer {
            ChoiceRow(symbol: "slider.vertical.3", title: "Equalizer", value: eqPresetName(preset: equalizer.preset), options: options) {
                ForEach(eqPresets(), id: \.self) { preset in
                    ChoiceOption(title: eqPresetName(preset: preset), isSelected: equalizer.preset == preset) {
                        model.change(.equalizerPreset(preset: preset))
                    }
                }
                ChoiceOption(title: "Edit Manual Equalizer…", isSelected: false, action: actions.editEqualizer)
            }
        }

        if hasHeadphoneSettings(device) {
            MenuDivider()
            FoldingSection(title: "Headphone Settings", options: options) {
                headphoneSettings(device)
            }
        }
    }

    /// Settings changed now and then: the rest of the features, how they
    /// behave, and turning the headphones off. Each shows once the headset
    /// has reported it.
    @ViewBuilder
    private func headphoneSettings(_ device: Device) -> some View {
        ForEach(reported(device.features.filter { !Self.mainFeatures.contains($0) }), id: \.self, content: featureRow)
        if showsChatOptions(device), let sensitivity = model.voiceSensitivity {
            ChoiceRow(
                symbol: "bubble.left.and.bubble.right", title: "Speak-to-Chat Sensitivity",
                value: voiceSensitivityName(sensitivity: sensitivity), options: options
            ) {
                ForEach(voiceSensitivities(), id: \.self) { choice in
                    ChoiceOption(title: voiceSensitivityName(sensitivity: choice), isSelected: choice == sensitivity) {
                        model.change(.voiceSensitivity(sensitivity: choice))
                    }
                }
            }
        }
        if showsChatOptions(device), let end = model.speakToChatEnd {
            ChoiceRow(
                symbol: "arrow.uturn.backward", title: "Back to the Previous Mode", value: speakToChatEndName(end: end),
                options: options
            ) {
                ForEach(speakToChatEnds(), id: \.self) { choice in
                    ChoiceOption(title: speakToChatEndName(end: choice), isSelected: choice == end) {
                        model.change(.speakToChatEnd(end: choice))
                    }
                }
            }
        }
        if !device.autoOff.isEmpty, let autoOff = model.autoOff {
            ChoiceRow(symbol: "moon.zzz", title: "Turn Off Automatically", value: autoOffName(autoOff: autoOff), options: options) {
                ForEach(device.autoOff, id: \.self) { choice in
                    ChoiceOption(title: autoOffName(autoOff: choice), isSelected: choice == autoOff) {
                        model.change(.autoOff(autoOff: choice))
                    }
                }
            }
        }
        if !device.noiseButton.isEmpty, let button = model.noiseButton {
            ChoiceRow(
                symbol: "button.horizontal.top.press", title: "Noise Control Button", value: noiseButtonName(choice: button),
                options: options
            ) {
                ForEach(device.noiseButton, id: \.self) { choice in
                    ChoiceOption(title: noiseButtonName(choice: choice), isSelected: choice == button) {
                        model.change(.noiseButton(choice: choice))
                    }
                }
            }
        }
        // Which of the devices the headset is connected to it plays; shown
        // once it's connected to more than this Mac.
        if model.connectedDevices.count > 1 {
            let playing = model.connectedDevices.first(where: \.playing)
            ChoiceRow(symbol: "laptopcomputer.and.iphone", title: "Play Audio From", value: playing?.name ?? "", options: options) {
                ForEach(model.connectedDevices, id: \.id) { device in
                    ChoiceOption(title: device.name, isSelected: device.playing) {
                        model.change(.playFrom(id: device.id))
                    }
                }
            }
        }
        if device.powerOff {
            ActionRow(symbol: "power", title: "Turn Off Headphones…", action: actions.powerOff)
        }
    }

    private func hasHeadphoneSettings(_ device: Device) -> Bool {
        !reported(device.features.filter { !Self.mainFeatures.contains($0) }).isEmpty
            || (showsChatOptions(device) && model.voiceSensitivity != nil)
            || (!device.autoOff.isEmpty && model.autoOff != nil)
            || (!device.noiseButton.isEmpty && model.noiseButton != nil)
            || model.connectedDevices.count > 1
            || device.powerOff
    }

    @ViewBuilder
    private var appSettings: some View {
        ToggleRow(symbol: "keyboard", title: ModeShortcut.title, isOn: shortcut.isOn) {
            shortcut.isOn.toggle()
        }
        .help("Press Control-Option-Command-N in any app to switch to the next mode.")
        ToggleRow(symbol: "battery.75percent", title: "Show Battery in Menu Bar", isOn: options.showsBattery) {
            options.showsBattery.toggle()
        }
        ToggleRow(symbol: "bell.badge", title: "Low Battery Alerts", isOn: batteryAlerts.isOn) {
            batteryAlerts.isOn.toggle()
        }
        .help("A notification when the battery is down to 20%, and again at 10% and 5%.")
        ToggleRow(symbol: "rectangle.inset.topright.filled", title: ModePopUp.title, isOn: popUp.isOn) {
            popUp.isOn.toggle()
        }
        .help("Shows the new mode when you change it with the button on your headphones or a shortcut.")
        ToggleRow(symbol: "arrow.up.forward.app", title: "Open at Login", isOn: options.opensAtLogin) {
            options.opensAtLogin.toggle()
            actions.openAtLogin(options.opensAtLogin)
        }
    }

    private func featureRow(_ feature: Feature) -> some View {
        ToggleRow(symbol: Self.symbol(for: feature), title: featureName(feature: feature), isOn: model.features[feature] == true) {
            model.change(.feature(feature: feature, on: model.features[feature] != true))
        }
        .help(featureDescription(feature: feature))
    }

    /// The features the headset has reported, in order; the rest stay hidden.
    private func reported(_ features: [Feature]) -> [Feature] {
        features.filter { model.features[$0] != nil }
    }

    private func showsChatOptions(_ device: Device) -> Bool {
        device.speakToChatOptions && model.features[.speakToChat] == true
    }

    private static func symbol(for feature: Feature) -> String {
        switch feature {
        case .speakToChat: "bubble.left.and.bubble.right"
        case .dsee: "waveform"
        case .voicePrompts: "speaker.wave.2.bubble"
        case .pauseWhenTakenOff: "pause.circle"
        case .actionButtonNoiseCancelling: "button.programmable"
        }
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

// MARK: Rows

/// What every row shares, so they line up like the Sound menu's: the same
/// margins and a column of round symbols, with names after it.
private enum Row {
    static let inset: CGFloat = 14
    static let symbolSize: CGFloat = 26
    static let spacing: CGFloat = 10
    static let height: CGFloat = 34
    /// Where names start, for what lines up under them.
    static let titleInset = inset + symbolSize + spacing
}

extension View {
    /// A full-width row with the panel's margins.
    fileprivate func rowLayout(height: CGFloat = Row.height) -> some View {
        padding(.horizontal, Row.inset)
            .frame(maxWidth: .infinity, minHeight: height, alignment: .leading)
            .contentShape(Rectangle())
    }
}

/// The grey rounded highlight under the pointer, like the Wi-Fi menu's.
private struct HoverHighlight: ViewModifier {
    @State private var isHovered = false

    func body(content: Content) -> some View {
        content
            .background(RoundedRectangle(cornerRadius: 8).fill(Color.primary.opacity(isHovered ? 0.08 : 0)).padding(.horizontal, 5))
            .onHover { isHovered = $0 }
    }
}

/// A row's symbol in a circle: blue while it's on, grey otherwise.
private struct RowSymbol: View {
    let name: String
    var isOn = false

    var body: some View {
        Image(systemName: name)
            .font(.system(size: 12, weight: .medium))
            .foregroundStyle(isOn ? Color.white : Color.primary)
            .frame(width: Row.symbolSize, height: Row.symbolSize)
            .background(Circle().fill(isOn ? Color.accentColor : Color.primary.opacity(0.1)))
    }
}

private struct MenuDivider: View {
    var body: some View {
        Divider().padding(.horizontal, Row.inset).padding(.vertical, 5)
    }
}

/// A small grey title over the rows that follow, like Known Network.
private struct SectionTitle: View {
    let title: String

    var body: some View {
        Text(title)
            .font(.system(size: 11, weight: .semibold))
            .foregroundStyle(.secondary)
            .rowLayout(height: 22)
    }
}

/// Something that's on or off, or one of a few choices: its symbol turns
/// blue when it is. Clicking the row changes it.
private struct ToggleRow: View {
    let symbol: String
    let title: String
    let isOn: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: Row.spacing) {
                RowSymbol(name: symbol, isOn: isOn)
                Text(title)
                Spacer(minLength: 8)
            }
            .rowLayout()
            .modifier(HoverHighlight())
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(isOn ? .isSelected : [])
    }
}

/// A row that does something when clicked, like Wi-Fi Settings….
private struct ActionRow: View {
    let symbol: String?
    let title: String
    /// Shown at the end, like a menu item's keyboard shortcut.
    var key: String?
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: Row.spacing) {
                if let symbol { RowSymbol(name: symbol) }
                Text(title)
                Spacer(minLength: 8)
                if let key { Text(key).foregroundStyle(.tertiary) }
            }
            .rowLayout(height: symbol == nil ? 28 : Row.height)
            .modifier(HoverHighlight())
        }
        .buttonStyle(.plain)
    }
}

/// A heading that shows or hides the rows under it, like Other Networks.
private struct FoldingSection<Content: View>: View {
    let title: String
    let options: MenuOptions
    @ViewBuilder let content: () -> Content

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Button {
                options.toggle(title)
            } label: {
                HStack {
                    Text(title).font(.system(size: 13, weight: .semibold)).foregroundStyle(.secondary)
                    Spacer(minLength: 8)
                    Chevron(isUnfolded: isUnfolded)
                }
                .rowLayout(height: 28)
                .modifier(HoverHighlight())
            }
            .buttonStyle(.plain)
            .accessibilityAddTraits(isUnfolded ? .isSelected : [])
            if isUnfolded { content() }
        }
    }

    private var isUnfolded: Bool { options.unfolded.contains(title) }
}

/// A setting with a few choices: its name and the choice in use, unfolding
/// to all of them with a checkmark on that one.
private struct ChoiceRow<Choices: View>: View {
    let symbol: String
    let title: String
    let value: String
    let options: MenuOptions
    @ViewBuilder let choices: () -> Choices

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Button {
                options.toggle(title)
            } label: {
                HStack(spacing: Row.spacing) {
                    RowSymbol(name: symbol)
                    VStack(alignment: .leading, spacing: 0) {
                        Text(title)
                        Text(value).font(.system(size: 11)).foregroundStyle(.secondary)
                    }
                    Spacer(minLength: 8)
                    Chevron(isUnfolded: isUnfolded)
                }
                .rowLayout(height: 38)
                .modifier(HoverHighlight())
            }
            .buttonStyle(.plain)
            if isUnfolded { choices() }
        }
    }

    private var isUnfolded: Bool { options.unfolded.contains(title) }
}

/// One of a choice row's choices, its name lined up under the row's name and
/// the checkmark just before it.
private struct ChoiceOption: View {
    private static let checkWidth: CGFloat = 12
    private static let checkSpacing: CGFloat = 6

    let title: String
    let isSelected: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: Self.checkSpacing) {
                Image(systemName: "checkmark")
                    .font(.system(size: 11, weight: .semibold))
                    .opacity(isSelected ? 1 : 0)
                    .frame(width: Self.checkWidth)
                Text(title)
                Spacer(minLength: 8)
            }
            .padding(.leading, Row.titleInset - Row.inset - Self.checkWidth - Self.checkSpacing)
            .rowLayout(height: 26)
            .modifier(HoverHighlight())
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(isSelected ? .isSelected : [])
    }
}

private struct Chevron: View {
    let isUnfolded: Bool

    var body: some View {
        Image(systemName: "chevron.right")
            .font(.system(size: 11, weight: .semibold))
            .foregroundStyle(.secondary)
            .rotationEffect(.degrees(isUnfolded ? 90 : 0))
    }
}

/// The Ambient Sound level: a slider and the number, under the mode's name,
/// like the volume slider in the Sound menu.
private struct LevelRow: View {
    let model: HeadsetModel
    let range: ClosedRange<UInt8>

    var body: some View {
        HStack(spacing: 10) {
            Slider(
                value: Binding(
                    get: { Double(level) }, set: { model.change(.ambientLevel(level: UInt8($0.rounded()))) }),
                in: Double(range.lowerBound)...Double(range.upperBound)
            )
            .controlSize(.small)
            .labelsHidden()
            .accessibilityLabel("Ambient Sound level")
            Text("\(level)")
                .monospacedDigit()
                .foregroundStyle(.secondary)
                .frame(minWidth: 18, alignment: .trailing)
        }
        .padding(.leading, Row.titleInset)
        .padding(.trailing, Row.inset)
        .frame(minHeight: 26)
    }

    private var level: UInt8 { model.ambientLevel ?? range.upperBound }
}

// MARK: Header

/// The headphones as a large symbol, their name and connection details, the
/// battery at the side like the AirPods header, and an Update button while a
/// new version is out.
private struct HeaderView: View {
    let model: HeadsetModel
    let updater: Updater
    let update: () -> Void

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: symbol)
                .symbolRenderingMode(.hierarchical)
                .font(.system(size: 30, weight: .light))
                .foregroundStyle(isConnected ? Color.primary : Color.secondary)
                .frame(width: 40, height: 40)

            VStack(alignment: .leading, spacing: 1) {
                Text(model.name ?? "Sound Connect")
                    .font(.system(size: 13, weight: .semibold))
                Text(details)
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
            }

            Spacer(minLength: 8)

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
        .padding(.horizontal, Row.inset)
        .padding(.vertical, 6)
        .help(model.firmware.map { "Firmware \($0)" } ?? "")
    }

    /// Earbuds or over-ear headphones, by how the headset reports its battery;
    /// headphones on macOS versions without the earbuds symbol.
    private var symbol: String {
        let earbuds =
            model.device?.battery == .earbuds && NSImage(systemSymbolName: "earbuds", accessibilityDescription: nil) != nil
        return earbuds ? "earbuds" : "headphones"
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
