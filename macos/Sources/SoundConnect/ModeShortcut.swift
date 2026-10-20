import Carbon.HIToolbox
import Foundation
import Observation

/// "Switch Modes with ⌃⌥⌘N": a keyboard shortcut that works in every app and
/// steps through the headset's listening modes. It's a Carbon hot key, the
/// one kind macOS offers that needs no permission; it must include ⌘ or ⌃,
/// since macOS 15 turns down hot keys with only ⌥ and ⇧. Off until the user
/// turns it on.
@MainActor
@Observable
final class ModeShortcut {
    static let title = "Switch Modes with ⌃⌥⌘N"
    private static let key = "ModeShortcut"
    private static let keyCode = UInt32(kVK_ANSI_N)
    private static let modifiers = UInt32(controlKey | optionKey | cmdKey)

    var isOn = UserDefaults.standard.bool(forKey: ModeShortcut.key) {
        didSet {
            UserDefaults.standard.set(isOn, forKey: Self.key)
            apply()
        }
    }

    private let model: HeadsetModel
    @ObservationIgnored private var hotKey: EventHotKeyRef?

    init(model: HeadsetModel) {
        self.model = model
        // Hot key presses arrive as Carbon events on the main thread. This
        // object lives as long as the app, so the pointer stays valid.
        var pressed = EventTypeSpec(eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyPressed))
        InstallEventHandler(
            GetApplicationEventTarget(),
            { _, _, context in
                guard let context else { return OSStatus(eventNotHandledErr) }
                let shortcut = Unmanaged<ModeShortcut>.fromOpaque(context).takeUnretainedValue()
                MainActor.assumeIsolated { shortcut.model.switchToNextMode() }
                return noErr
            },
            1, &pressed, Unmanaged.passUnretained(self).toOpaque(), nil)
        apply()
    }

    private func apply() {
        if let hotKey {
            UnregisterEventHotKey(hotKey)
            self.hotKey = nil
        }
        guard isOn else { return }
        let id = EventHotKeyID(signature: OSType(0x5343_4e54), id: 1)  // "SCNT"
        let status = RegisterEventHotKey(Self.keyCode, Self.modifiers, id, GetApplicationEventTarget(), 0, &hotKey)
        if status != noErr {
            // Another app already has it.
            NSLog("Sound Connect: couldn't register \(Self.title): \(status)")
        }
    }
}
