import Foundation
import IOBluetooth
import SoundConnectCore

/// Keeps the control channel to a supported headset open and lets its
/// driver, from the shared Rust core, speak over it. Everything happens on
/// the main thread, where IOBluetooth delivers its callbacks.
final class HeadsetLink: NSObject, IOBluetoothRFCOMMChannelDelegate {
    /// Wait between attempts to find the headset, growing while it keeps failing.
    private static let retryDelays: [TimeInterval] = [3, 5, 10, 30]
    /// How long opening the channel may take before giving up on it, e.g.
    /// because the headset turned off meanwhile and IOBluetooth never answers.
    private static let openTimeout: TimeInterval = 15
    /// Control services of supported headsets, in order of preference.
    private static let services: [(name: String, uuid: IOBluetoothSDPUUID)] = serviceUuids().compactMap { name in
        sdpUUID(name).map { (name, $0) }
    }

    private let model: HeadsetModel
    private var headset: Headset?
    private var channel: IOBluetoothRFCOMMChannel?
    private var channelOpen = false
    private var failures = 0
    /// The user asked to connect a headset and Bluetooth is still working on it.
    private var connectingManually = false
    private var retry: Timer?
    private var openDeadline: Timer?
    private var timers: [Timer] = []
    private var connectNotification: IOBluetoothUserNotification?

    init(model: HeadsetModel) {
        self.model = model
        super.init()
        model.send = { [weak self] setting in self?.send(setting) }
        model.connect = { [weak self] name in self?.connect(name: name) }
        connectNotification = IOBluetoothDevice.register(
            forConnectNotifications: self, selector: #selector(deviceConnected(_:device:)))
        attempt()
    }

    // MARK: Finding the headset

    private func attempt() {
        retry = nil
        guard channel == nil, !connectingManually else { return }

        let devices = (IOBluetoothDevice.pairedDevices() as? [IOBluetoothDevice]) ?? []
        model.disconnectedHeadsets = devices
            .filter { !$0.isConnected() && Self.controlService(of: $0) != nil }
            .map(\.nameOrAddress)

        for device in devices where device.isConnected() {
            guard let (service, record) = Self.controlService(of: device) else {
                // Service records aren't cached yet: ask the headset, then try again.
                if Self.isAudio(device) { device.performSDPQuery(self, uuids: Self.services.map(\.uuid)) }
                continue
            }
            guard let headset = headsetFor(name: device.nameOrAddress, service: service) else { continue }
            var channelID: BluetoothRFCOMMChannelID = 0
            guard record.getRFCOMMChannelID(&channelID) == kIOReturnSuccess else { continue }

            var opened: IOBluetoothRFCOMMChannel?
            let status = device.openRFCOMMChannelAsync(&opened, withChannelID: channelID, delegate: self)
            if status == kIOReturnSuccess, let opened {
                channel = opened
                self.headset = headset
                model.reset(to: .connecting(name: device.nameOrAddress))
                openDeadline = schedule(after: Self.openTimeout, repeats: false) { [weak self] in
                    self?.openFailed(opened)
                }
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
        guard channel == nil, Self.isAudio(device) else { return }
        failures = 0
        // Give the headset a moment to finish setting up audio.
        scheduleAttempt(after: 1.5)
    }

    @objc private func sdpQueryComplete(_ device: IOBluetoothDevice, status: IOReturn) {
        if channel == nil { scheduleAttempt(after: 0.2) }
    }

    /// Connects a paired headset, like the Connect button for AirPods. With
    /// a target, `openConnection` returns right away and reports back in
    /// `connectionComplete`; without one it blocks until Bluetooth gives up.
    private func connect(name: String) {
        let devices = (IOBluetoothDevice.pairedDevices() as? [IOBluetoothDevice]) ?? []
        guard let device = devices.first(where: { $0.nameOrAddress == name }) else { return }
        retry?.invalidate()
        retry = nil
        connectingManually = true
        model.notice = nil
        model.reset(to: .connecting(name: name))
        if device.openConnection(self) != kIOReturnSuccess { connectionFailed() }
    }

    @objc private func connectionComplete(_ device: IOBluetoothDevice, status: IOReturn) {
        connectingManually = false
        guard status == kIOReturnSuccess else { return connectionFailed() }
        failures = 0
        // Give the headset a moment to finish setting up audio.
        scheduleAttempt(after: 1.5)
    }

    private func connectionFailed() {
        connectingManually = false
        model.reset(to: .searching)
        model.notice = "Couldn't connect. Are they on?"
        scheduleAttempt(after: Self.retryDelays[0])
    }

    // MARK: The control channel

    func rfcommChannelOpenComplete(_ rfcommChannel: IOBluetoothRFCOMMChannel!, status error: IOReturn) {
        guard rfcommChannel === channel, let headset else { return }
        openDeadline?.invalidate()
        openDeadline = nil
        guard error == kIOReturnSuccess else { return openFailed(rfcommChannel) }

        failures = 0
        channelOpen = true
        model.notice = nil
        if let name = model.name { model.connection = .connected(name: name) }
        handle(headset.start())
        timers = [
            schedule(after: 0.25, repeats: true) { [weak self] in self?.write(headset.tick()) },
            schedule(after: 60, repeats: true) { [weak self] in self?.write(headset.refresh()) },
        ]
    }

    private func openFailed(_ rfcommChannel: IOBluetoothRFCOMMChannel) {
        guard rfcommChannel === channel, !channelOpen else { return }
        rfcommChannel.close()
        channel = nil
        headset = nil
        model.reset(to: .searching)
        scheduleAttempt(after: Self.retryDelays[min(failures, Self.retryDelays.count - 1)])
        failures += 1
    }

    func rfcommChannelData(_ rfcommChannel: IOBluetoothRFCOMMChannel!, data dataPointer: UnsafeMutableRawPointer!, length dataLength: Int) {
        guard rfcommChannel === channel, let headset else { return }
        handle(headset.receive(bytes: Data(bytes: dataPointer, count: dataLength)))
    }

    func rfcommChannelClosed(_ rfcommChannel: IOBluetoothRFCOMMChannel!) {
        guard rfcommChannel === channel else { return }
        channel = nil
        headset = nil
        channelOpen = false
        timers.forEach { $0.invalidate() }
        timers = []
        model.reset(to: .searching)
        scheduleAttempt(after: 1)
    }

    private func send(_ setting: Setting) {
        guard channelOpen, let headset else { return }
        write(headset.set(setting: setting))
    }

    private func handle(_ output: Output) {
        write(output.writes)
        output.updates.forEach(model.apply)
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

    /// The first supported control service the device offers, if macOS has
    /// its service records.
    private static func controlService(of device: IOBluetoothDevice) -> (String, IOBluetoothSDPServiceRecord)? {
        services.lazy.compactMap { service in device.getServiceRecord(for: service.uuid).map { (service.name, $0) } }.first
    }

    private static func isAudio(_ device: IOBluetoothDevice) -> Bool {
        device.deviceClassMajor == BluetoothDeviceClassMajor(kBluetoothDeviceClassMajorAudio)
    }

    private static func sdpUUID(_ string: String) -> IOBluetoothSDPUUID? {
        guard let uuid = UUID(uuidString: string) else { return nil }
        return withUnsafeBytes(of: uuid.uuid) { IOBluetoothSDPUUID(bytes: $0.baseAddress, length: 16) }
    }
}
