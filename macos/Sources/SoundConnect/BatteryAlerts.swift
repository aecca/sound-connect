import Foundation
import Observation
import SoundConnectCore
import UserNotifications

/// "Low Battery Alerts": a notification when the headset's battery runs low,
/// at 20% and again at 10% and 5%. The shared core decides when. macOS asks once
/// whether Sound Connect may show notifications, the first time it has one.
@MainActor
final class BatteryAlerts: NSObject, UNUserNotificationCenterDelegate {
    private static let key = "LowBatteryAlerts"

    var isOn = UserDefaults.standard.object(forKey: BatteryAlerts.key) as? Bool ?? true {
        didSet { UserDefaults.standard.set(isOn, forKey: Self.key) }
    }

    private let model: HeadsetModel
    private var low = LowBattery()
    /// Which headset `low` is about: another one starts afresh.
    private var watching: String?

    init(model: HeadsetModel) {
        self.model = model
        super.init()
        observe()
    }

    private func observe() {
        withObservationTracking {
            _ = (model.battery, model.leftBattery, model.rightBattery)
        } onChange: {
            Task { @MainActor [weak self] in
                self?.check()
                self?.observe()
            }
        }
    }

    private func check() {
        if let name = model.name, name != watching {
            watching = name
            low = LowBattery()
        }
        // Earbuds: the emptier one that isn't charging.
        let earbuds = [model.leftBattery, model.rightBattery].compactMap { $0 }.filter { !$0.charging }
        guard let battery = model.battery ?? earbuds.min(by: { $0.level < $1.level }),
            let device = model.device,
            low.update(battery: battery),
            isOn
        else { return }

        let content = UNMutableNotificationContent()
        content.title = "Your headphones' battery is low"
        content.body = "\(brandName(brand: device.brand)) \(device.model) has \(battery.level)% left."
        let request = UNNotificationRequest(identifier: "low-battery", content: content, trigger: nil)
        // Only reached with an app bundle: the notification center needs one,
        // and the README snapshots run without.
        UNUserNotificationCenter.current().delegate = self
        Task {
            let center = UNUserNotificationCenter.current()
            guard (try? await center.requestAuthorization(options: [.alert, .sound])) == true else { return }
            try? await center.add(request)
        }
    }

    /// Shows it even while Sound Connect's menu is open.
    nonisolated func userNotificationCenter(
        _ center: UNUserNotificationCenter, willPresent notification: UNNotification,
        withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
    ) {
        completionHandler([.banner, .sound])
    }
}
