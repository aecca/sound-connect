import Foundation
import Observation
import SoundConnectCore

/// What the app knows about the headset, whatever its brand. The menu reads
/// it; `change(_:)` updates it right away and asks the link to tell the
/// headset.
@Observable
final class HeadsetModel {
    enum Connection: Equatable {
        case searching
        case connecting(name: String)
        case connected(name: String)
        case unsupported(name: String, reason: String)
    }

    /// Print protocol updates; set SOUND_CONNECT_DEBUG to turn on.
    private static let debug = ProcessInfo.processInfo.environment["SOUND_CONNECT_DEBUG"] != nil

    var connection = Connection.searching {
        didSet { if Self.debug, connection != oldValue { print("connection:", connection) } }
    }
    /// Which controls the headset has, once its driver knows.
    var device: Device?
    var battery: Battery?
    /// Earbuds report each side and the case instead of `battery`.
    var leftBattery: Battery?
    var rightBattery: Battery?
    var caseBattery: Battery?
    var codec: String?
    var firmware: String?
    var mode: ListeningMode?
    var ambientLevel: UInt8?
    var focusOnVoice: Bool?
    var features: [Feature: Bool] = [:]
    var autoOff: AutoOff?
    var noiseButton: NoiseButton?
    var voiceSensitivity: VoiceSensitivity?
    var speakToChatEnd: SpeakToChatEnd?
    var equalizer: Equalizer?
    /// Supported headsets paired with this Mac but not connected right now.
    var disconnectedHeadsets: [String] = []
    /// A problem to show while not connected, e.g. a failed connection.
    var notice: String?

    /// Set by the link: sends a change to the headset.
    @ObservationIgnored var send: (Setting) -> Void = { _ in }
    /// Set by the link: connects a paired headset by name.
    @ObservationIgnored var connect: (String) -> Void = { _ in }

    var name: String? {
        switch connection {
        case .searching: nil
        case .connecting(let name), .connected(let name), .unsupported(let name, _): name
        }
    }

    func apply(_ update: Update) {
        if Self.debug { print("update:", update) }
        switch update {
        case .device(let device): self.device = device
        case .unsupported(let reason):
            if let name { connection = .unsupported(name: name, reason: reason) }
        case .firmware(let version): firmware = version
        case .codec(let name): codec = name
        case .battery(let value): battery = value
        case .earbuds(let left, let right):
            leftBattery = left
            rightBattery = right
        case .caseBattery(let value): caseBattery = value
        case .mode(let mode): self.mode = mode
        case .ambientLevel(let level): ambientLevel = level
        case .focusOnVoice(let on): focusOnVoice = on
        case .feature(let feature, let on): features[feature] = on
        case .autoOff(let value): autoOff = value
        case .noiseButton(let choice): noiseButton = choice
        case .voiceSensitivity(let sensitivity): voiceSensitivity = sensitivity
        case .speakToChatEnd(let end): speakToChatEnd = end
        case .equalizer(let value): equalizer = value
        }
    }

    /// Forgets the headset's state, e.g. after it disconnects.
    func reset(to connection: Connection) {
        self.connection = connection
        device = nil
        battery = nil
        leftBattery = nil
        rightBattery = nil
        caseBattery = nil
        codec = nil
        firmware = nil
        mode = nil
        ambientLevel = nil
        focusOnVoice = nil
        features = [:]
        autoOff = nil
        noiseButton = nil
        voiceSensitivity = nil
        speakToChatEnd = nil
        equalizer = nil
    }

    /// The headset's battery, or the emptier earbud's.
    var lowestBattery: Battery? {
        battery ?? [leftBattery, rightBattery].compactMap { $0 }.min { $0.level < $1.level }
    }

    /// Switches to the listening mode after the current one, in the order the
    /// menu lists them, and back to the first after the last.
    func switchToNextMode() {
        guard let modes = device?.modes.map(\.mode), !modes.isEmpty else { return }
        let next = mode.flatMap { modes.firstIndex(of: $0) }.map { modes[($0 + 1) % modes.count] } ?? modes[0]
        change(.mode(mode: next))
    }

    /// Applies a change the user made and sends it to the headset, unless
    /// nothing actually changed (a slider reports every position).
    func change(_ setting: Setting) {
        switch setting {
        case .mode(let mode):
            guard self.mode != mode else { return }
            self.mode = mode
        case .ambientLevel(let level):
            guard ambientLevel != level else { return }
            ambientLevel = level
        case .focusOnVoice(let on):
            guard focusOnVoice != on else { return }
            focusOnVoice = on
        case .feature(let feature, let on):
            guard features[feature] != on else { return }
            features[feature] = on
        case .autoOff(let value):
            guard autoOff != value else { return }
            autoOff = value
        case .noiseButton(let choice):
            guard noiseButton != choice else { return }
            noiseButton = choice
        case .voiceSensitivity(let sensitivity):
            guard voiceSensitivity != sensitivity else { return }
            voiceSensitivity = sensitivity
        case .speakToChatEnd(let end):
            guard speakToChatEnd != end else { return }
            speakToChatEnd = end
        case .equalizerPreset(let preset):
            guard var equalizer, equalizer.preset != preset else { return }
            equalizer.preset = preset
            self.equalizer = equalizer
        case .equalizerBands(let bands):
            let value = Equalizer(preset: .manual, bands: bands)
            guard equalizer != nil, equalizer != value else { return }
            equalizer = value
        }
        send(setting)
    }
}
