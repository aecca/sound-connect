import Foundation
import IOBluetooth
import SoundConnectCore

/// Keeps the control channel to the connected Sony headset open and runs the
/// shared Rust protocol session over it. Everything happens on the main
/// thread, where IOBluetooth delivers its callbacks.
final class HeadsetLink: NSObject, IOBluetoothRFCOMMChannelDelegate {
    /// Wait between attempts to find the headset, growing while it keeps failing.
    private static let retryDelays: [TimeInterval] = [3, 5, 10, 30]
    private static let serviceUUIDs: [IOBluetoothSDPUUID] = serviceUuids().compactMap(sdpUUID)

    private let model: HeadsetModel
    private var session = Session()
    private var channel: IOBluetoothRFCOMMChannel?
    private var channelOpen = false
    private var failures = 0
    private var retry: Timer?
    private var timers: [Timer] = []
    private var connectNotification: IOBluetoothUserNotification?

    init(model: HeadsetModel) {
        self.model = model
        super.init()
        model.send = { [weak self] request in self?.send(request) }
        model.connect = { [weak self] name in self?.connect(name: name) }
        connectNotification = IOBluetoothDevice.register(
            forConnectNotifications: self, selector: #selector(deviceConnected(_:device:)))
        attempt()
    }

    // MARK: Finding the headset

    private func attempt() {
        retry = nil
        guard channel == nil else { return }

        let devices = (IOBluetoothDevice.pairedDevices() as? [IOBluetoothDevice]) ?? []
        model.disconnectedHeadsets = devices.filter { !$0.isConnected() && Self.looksLikeSony($0) }.map(\.nameOrAddress)

        for device in devices where device.isConnected() {
            guard let record = Self.serviceUUIDs.lazy.compactMap({ device.getServiceRecord(for: $0) }).first else {
                // Service records aren't cached yet: ask the headset, then try again.
                if Self.looksLikeSony(device) { device.performSDPQuery(self, uuids: Self.serviceUUIDs) }
                continue
            }
            var channelID: BluetoothRFCOMMChannelID = 0
            guard record.getRFCOMMChannelID(&channelID) == kIOReturnSuccess else { continue }

            var opened: IOBluetoothRFCOMMChannel?
            let status = device.openRFCOMMChannelAsync(&opened, withChannelID: channelID, delegate: self)
            if status == kIOReturnSuccess, let opened {
                channel = opened
                model.reset(to: .connecting(name: device.nameOrAddress))
                return
            }
            // IOBluetooth can hand back a channel even when opening fails;
            // close it or every failed attempt leaks a file descriptor.
            opened?.close()
        }

        model.reset(to: .searching)
        scheduleAttempt(after: Self.retryDelays[min(failures, Self.retryDelays.count - 1)])
        failures += 1
    }

    private func scheduleAttempt(after delay: TimeInterval) {
        retry?.invalidate()
        retry = schedule(after: delay, repeats: false) { [weak self] in self?.attempt() }
    }

    @objc private func deviceConnected(_ note: IOBluetoothUserNotification, device: IOBluetoothDevice) {
        guard channel == nil, Self.looksLikeSony(device) else { return }
        failures = 0
        // Give the headset a moment to finish setting up audio.
        scheduleAttempt(after: 1.5)
    }

    @objc private func sdpQueryComplete(_ device: IOBluetoothDevice, status: IOReturn) {
        if channel == nil { scheduleAttempt(after: 0.2) }
    }

    /// Connects a paired headset, like the Connect button for AirPods.
    private func connect(name: String) {
        let devices = (IOBluetoothDevice.pairedDevices() as? [IOBluetoothDevice]) ?? []
        devices.first { $0.nameOrAddress == name }?.openConnection(nil)
    }

    // MARK: The control channel

    func rfcommChannelOpenComplete(_ rfcommChannel: IOBluetoothRFCOMMChannel!, status error: IOReturn) {
        guard rfcommChannel === channel else { return }
        guard error == kIOReturnSuccess else {
            rfcommChannel.close()
            channel = nil
            model.reset(to: .searching)
            scheduleAttempt(after: Self.retryDelays[min(failures, Self.retryDelays.count - 1)])
            failures += 1
            return
        }

        failures = 0
        channelOpen = true
        session = Session()
        if let name = model.name { model.connection = .connected(name: name) }
        for request in initialRequests() { write(session.request(request: request)) }
        timers = [
            schedule(after: 0.25, repeats: true) { [weak self] in
                guard let self else { return }
                write(session.tick())
            },
            schedule(after: 60, repeats: true) { [weak self] in self?.send(.battery) },
        ]
    }

    func rfcommChannelData(_ rfcommChannel: IOBluetoothRFCOMMChannel!, data dataPointer: UnsafeMutableRawPointer!, length dataLength: Int) {
        guard rfcommChannel === channel else { return }
        let output = session.receive(bytes: Data(bytes: dataPointer, count: dataLength))
        write(output.writes)
        output.updates.forEach(model.apply)
    }

    func rfcommChannelClosed(_ rfcommChannel: IOBluetoothRFCOMMChannel!) {
        guard rfcommChannel === channel else { return }
        channel = nil
        channelOpen = false
        timers.forEach { $0.invalidate() }
        timers = []
        model.reset(to: .searching)
        scheduleAttempt(after: 1)
    }

    private func send(_ request: Request) {
        guard channelOpen else { return }
        write(session.request(request: request))
    }

    private func write(_ frames: [Data]) {
        guard let channel else { return }
        for var frame in frames {
            frame.withUnsafeMutableBytes { bytes in
                _ = channel.writeSync(bytes.baseAddress, length: UInt16(bytes.count))
            }
        }
    }

    // MARK: Helpers

    /// A timer that also fires while a menu is open.
    private func schedule(after interval: TimeInterval, repeats: Bool, _ block: @escaping () -> Void) -> Timer {
        let timer = Timer(timeInterval: interval, repeats: repeats) { _ in block() }
        RunLoop.main.add(timer, forMode: .common)
        return timer
    }

    private static func looksLikeSony(_ device: IOBluetoothDevice) -> Bool {
        let name = device.nameOrAddress ?? ""
        return ["WH-", "WF-", "WI-", "MDR-", "LinkBuds", "ULT"].contains { name.hasPrefix($0) }
    }

    private static func sdpUUID(_ string: String) -> IOBluetoothSDPUUID? {
        guard let uuid = UUID(uuidString: string) else { return nil }
        return withUnsafeBytes(of: uuid.uuid) { IOBluetoothSDPUUID(bytes: $0.baseAddress, length: 16) }
    }
}
