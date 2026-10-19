import Foundation
import Observation
import SoundConnectCore

/// What the app knows about the headset. The menu reads it; its setters
/// update it right away and ask the link to tell the headset.
@Observable
final class HeadsetModel {
    enum Connection: Equatable {
        case searching
        case connecting(name: String)
        case connected(name: String)
        case unsupported(name: String)
    }

    /// Print protocol updates; set SOUND_CONNECT_DEBUG to turn on.
    private static let debug = ProcessInfo.processInfo.environment["SOUND_CONNECT_DEBUG"] != nil

    var connection = Connection.searching {
        didSet { if Self.debug, connection != oldValue { print("connection:", connection) } }
    }
    var battery: Battery?
    var codec: String?
    var firmware: String?
    var noise: NoiseControl?
    var speakToChat: Bool?
    var equalizer: Equalizer?
    var dsee: Bool?
    /// Sony headsets paired with this Mac but not connected right now.
    var disconnectedHeadsets: [String] = []

    /// Set by the link: sends a request to the headset.
    @ObservationIgnored var send: (Request) -> Void = { _ in }
    /// Set by the link: connects a paired headset by name.
    @ObservationIgnored var connect: (String) -> Void = { _ in }

    var name: String? {
        switch connection {
        case .searching: nil
        case .connecting(let name), .connected(let name), .unsupported(let name): name
        }
    }

    func apply(_ update: Update) {
        if Self.debug { print("update:", update) }
        switch update {
        case .ready(let v2):
            if !v2, let name { connection = .unsupported(name: name) }
        case .firmware(let version): firmware = version
        case .battery(let value): battery = value
        case .codec(let name): codec = name
        case .noiseControl(let value): noise = value
        case .speakToChat(let on): speakToChat = on
        case .equalizer(let value): equalizer = value
        case .dsee(let on): dsee = on
        }
    }

    /// Forgets the headset's state, e.g. after it disconnects.
    func reset(to connection: Connection) {
        self.connection = connection
        battery = nil
        codec = nil
        firmware = nil
        noise = nil
        speakToChat = nil
        equalizer = nil
        dsee = nil
    }

    func setMode(_ mode: NoiseMode) {
        guard var noise else { return }
        noise.mode = mode
        setNoise(noise)
    }

    func setAmbientLevel(_ level: UInt8) {
        guard var noise, noise.ambientLevel != level else { return }
        noise.ambientLevel = level
        setNoise(noise)
    }

    func setFocusOnVoice(_ on: Bool) {
        guard var noise else { return }
        noise.focusOnVoice = on
        setNoise(noise)
    }

    func setSpeakToChat(_ on: Bool) {
        speakToChat = on
        send(.setSpeakToChat(on: on))
    }

    func setPreset(_ preset: EqPreset) {
        guard var equalizer, equalizer.preset != preset else { return }
        equalizer.preset = preset
        self.equalizer = equalizer
        send(.setEqualizerPreset(preset: preset))
    }

    /// Changes the manual bands (400 Hz to 16 kHz) and Clear Bass, which
    /// switches the preset to Manual.
    func setBands(clearBass: Int8, bands: [Int8]) {
        guard var equalizer else { return }
        equalizer = Equalizer(preset: .manual, clearBass: clearBass, bands: bands)
        guard equalizer != self.equalizer else { return }
        self.equalizer = equalizer
        send(.setEqualizerBands(clearBass: clearBass, bands: bands))
    }

    func setDsee(_ on: Bool) {
        dsee = on
        send(.setDsee(on: on))
    }

    private func setNoise(_ value: NoiseControl) {
        guard value != noise else { return }
        noise = value
        send(.setNoiseControl(value: value))
    }
}
