import AppKit
import Observation

/// Keeps Sound Connect up to date. It checks once a day whether a newer
/// release is out and, when the user asks, installs it with the same
/// one-line installer people used the first time, which replaces this app,
/// opens the new one and quits this one.
@MainActor
@Observable
final class Updater {
    enum Status: Equatable {
        case upToDate
        case available(version: String)
        case installing(version: String)
        case failed(version: String)
    }

    /// GitHub redirects this to the newest release's page, `.../tag/v1.2.3`.
    private static let latestRelease = URL(string: "https://github.com/aecca/sound-connect/releases/latest")!
    private static let checkEvery: Duration = .seconds(24 * 60 * 60)
    /// When a check fails, e.g. while offline right after logging in.
    private static let retryAfter: Duration = .seconds(60 * 60)
    /// The installer from the release itself; the version comes in as `$1`.
    private static let installCommand = """
        set -o pipefail; curl -fsSL "https://raw.githubusercontent.com/aecca/sound-connect/v$1/install.sh" \
        | SOUND_CONNECT_VERSION="v$1" bash
        """

    /// Print status changes; set SOUND_CONNECT_DEBUG to turn on.
    private static let debug = ProcessInfo.processInfo.environment["SOUND_CONNECT_DEBUG"] != nil

    private(set) var status = Status.upToDate {
        didSet { if Self.debug { print("updater:", status) } }
    }
    /// The running installer, kept until it ends so its termination handler runs.
    @ObservationIgnored private var installer: Process?

    /// Starts checking, unless this build has no version (e.g. `swift run`).
    func start() {
        guard let current = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String else {
            return
        }
        Task {
            while true {
                let latest = await Self.latestVersion()
                if let latest, latest.compare(current, options: .numeric) == .orderedDescending {
                    offer(latest)
                }
                try? await Task.sleep(for: latest == nil ? Self.retryAfter : Self.checkEvery)
            }
        }
    }

    /// Installs the available version, or tries again after a failure.
    func install() {
        let version: String
        switch status {
        case .available(let available), .failed(let available): version = available
        case .upToDate, .installing: return
        }
        status = .installing(version: version)

        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/bash")
        process.arguments = ["-c", Self.installCommand, "bash", version]
        process.standardInput = FileHandle.nullDevice
        let log = Self.log()
        process.standardOutput = log
        process.standardError = log
        process.terminationHandler = { process in
            let installed = process.terminationStatus == 0
            Task { @MainActor [weak self] in
                // The installer normally quits this app itself; if it's still
                // running, the new one is open already.
                if installed { NSApp.terminate(nil) } else { self?.status = .failed(version: version) }
            }
        }
        do {
            try process.run()
            installer = process
        } catch {
            status = .failed(version: version)
        }
    }

    private func offer(_ version: String) {
        if case .installing = status { return }
        status = .available(version: version)
    }

    /// The newest release's version, or nil when it can't be told.
    private static func latestVersion() async -> String? {
        var request = URLRequest(url: latestRelease)
        request.httpMethod = "HEAD"
        guard let (_, response) = try? await URLSession.shared.data(for: request) else { return nil }
        return version(ofTag: response.url?.lastPathComponent)
    }

    /// "1.2.3" for the tag "v1.2.3"; nil for anything that isn't a release tag.
    private static func version(ofTag tag: String?) -> String? {
        guard let tag, tag.hasPrefix("v") else { return nil }
        let version = tag.dropFirst()
        let parts = version.split(separator: ".", omittingEmptySubsequences: false)
        let isNumber = { (part: Substring) in !part.isEmpty && part.allSatisfy { ("0"..."9").contains($0) } }
        guard parts.count == 3, parts.allSatisfy(isNumber) else { return nil }
        return String(version)
    }

    /// Where the installer's output goes: ~/Library/Logs/Sound Connect/update.log.
    private static func log() -> FileHandle {
        let folder = FileManager.default.homeDirectoryForCurrentUser.appending(path: "Library/Logs/Sound Connect")
        let file = folder.appending(path: "update.log")
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        guard FileManager.default.createFile(atPath: file.path, contents: nil),
            let handle = try? FileHandle(forWritingTo: file)
        else { return .nullDevice }
        return handle
    }
}
