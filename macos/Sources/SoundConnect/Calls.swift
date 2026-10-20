import CoreAudio
import Foundation
import SoundConnectCore

/// "Ambient Sound During Calls": while an app records from a microphone, as
/// in a video call, the headset switches to Ambient Sound so you hear
/// yourself, and back to its mode once the call is over. The shared core
/// decides; this tells it whether anything is recording.
@MainActor
final class Calls {
    /// macOS 14.2 and later say which apps are recording, without asking for
    /// access to the microphone.
    static var isSupported: Bool {
        if #available(macOS 14.2, *) { true } else { false }
    }

    private static let key = "AmbientSoundDuringCalls"

    var isOn = UserDefaults.standard.bool(forKey: Calls.key) {
        didSet {
            UserDefaults.standard.set(isOn, forKey: Self.key)
            watch()
        }
    }

    private let model: HeadsetModel
    private var policy = CallAmbience()
    private var timer: Timer?

    init(model: HeadsetModel) {
        self.model = model
        watch()
    }

    /// Looks once a second while turned on. CoreAudio can report changes
    /// instead, but developers find those reports unreliable for this.
    private func watch() {
        timer?.invalidate()
        timer = nil
        policy = CallAmbience()
        guard isOn, #available(macOS 14.2, *) else { return }
        let timer = Timer(timeInterval: 1, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated { self?.check() }
        }
        RunLoop.main.add(timer, forMode: .common)
        self.timer = timer
    }

    @available(macOS 14.2, *)
    private func check() {
        let hasAmbient = model.device?.modes.contains { $0.mode == .ambient } == true
        if let mode = policy.update(recording: Self.anyAppIsRecording(), mode: hasAmbient ? model.mode : nil) {
            model.change(.mode(mode: mode))
        }
    }

    /// Whether any app is recording from any microphone. Each app doing audio
    /// gets a process object; reading it needs no permission. Sound Connect
    /// itself never records.
    @available(macOS 14.2, *)
    private static func anyAppIsRecording() -> Bool {
        let system = AudioObjectID(kAudioObjectSystemObject)
        var address = AudioObjectPropertyAddress(
            mSelector: kAudioHardwarePropertyProcessObjectList,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain)
        var size: UInt32 = 0
        guard AudioObjectGetPropertyDataSize(system, &address, 0, nil, &size) == noErr else { return false }
        var processes = [AudioObjectID](repeating: 0, count: Int(size) / MemoryLayout<AudioObjectID>.size)
        guard AudioObjectGetPropertyData(system, &address, 0, nil, &size, &processes) == noErr else { return false }

        address.mSelector = kAudioProcessPropertyIsRunningInput
        return processes.contains { process in
            var running: UInt32 = 0
            var size = UInt32(MemoryLayout<UInt32>.size)
            return AudioObjectGetPropertyData(process, &address, 0, nil, &size, &running) == noErr && running != 0
        }
    }
}
