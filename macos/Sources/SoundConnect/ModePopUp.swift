import AppKit
import Observation
import SoundConnectCore
import SwiftUI

/// "Mode Change Pop-up": a brief notice with the new listening mode at the
/// top right of the screen, like the volume one, when the mode changes while
/// the menu is closed: with the button on the headphones, the keyboard
/// shortcut, a link or a call. On until the user turns it off.
@MainActor
final class ModePopUp {
    static let title = "Mode Change Pop-up"
    private static let key = "ModeChangePopUp"
    /// How long it stays before fading out.
    private static let shownFor: TimeInterval = 1.6

    var isOn = UserDefaults.standard.object(forKey: ModePopUp.key) as? Bool ?? true {
        didSet { UserDefaults.standard.set(isOn, forKey: Self.key) }
    }
    /// Set by the menu: changes made in it show there instead.
    var menuIsOpen = false

    private let model: HeadsetModel
    /// The mode last seen, so only an actual change shows, not the first report.
    private var lastMode: ListeningMode?
    private var panel: NSPanel?
    private var hideTimer: Timer?

    init(model: HeadsetModel) {
        self.model = model
        lastMode = model.mode
        observe()
    }

    private func observe() {
        withObservationTracking {
            _ = model.mode
        } onChange: {
            Task { @MainActor [weak self] in
                self?.modeChanged()
                self?.observe()
            }
        }
    }

    private func modeChanged() {
        let previous = lastMode
        lastMode = model.mode
        guard isOn, !menuIsOpen, let previous, let mode = model.mode, mode != previous,
            let name = model.device?.modes.first(where: { $0.mode == mode })?.name
        else { return }
        let details = [model.device?.model, model.lowestBattery.map { "\($0.level)%" }].compactMap { $0 }
        show(PopUpView(symbol: mode.symbol, title: name, details: details.joined(separator: " · ")))
    }

    private func show(_ content: PopUpView) {
        guard let screen = NSScreen.main else { return }
        let panel = self.panel ?? makePanel()
        self.panel = panel
        let host = NSHostingView(rootView: content)
        let size = host.fittingSize
        let background = NSVisualEffectView(frame: NSRect(origin: .zero, size: size))
        background.material = .hudWindow
        background.state = .active
        background.wantsLayer = true
        background.layer?.cornerRadius = 16
        host.frame = background.bounds
        background.addSubview(host)
        panel.contentView = background
        let visible = screen.visibleFrame
        panel.setFrame(
            NSRect(x: visible.maxX - size.width - 12, y: visible.maxY - size.height - 12, width: size.width, height: size.height),
            display: true)

        panel.orderFrontRegardless()
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.15
            panel.animator().alphaValue = 1
        }
        hideTimer?.invalidate()
        let timer = Timer(timeInterval: Self.shownFor, repeats: false) { [weak self] _ in
            MainActor.assumeIsolated { self?.hide() }
        }
        RunLoop.main.add(timer, forMode: .common)
        hideTimer = timer
    }

    private func hide() {
        guard let panel else { return }
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.3
            panel.animator().alphaValue = 0
        } completionHandler: {
            // Unless it was shown again meanwhile.
            MainActor.assumeIsolated { if panel.alphaValue == 0 { panel.orderOut(nil) } }
        }
    }

    /// A panel that floats over every app and space without taking focus or clicks.
    private func makePanel() -> NSPanel {
        let panel = NSPanel(
            contentRect: .zero, styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: true)
        panel.level = .statusBar
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = true
        panel.ignoresMouseEvents = true
        panel.alphaValue = 0
        panel.collectionBehavior = [.canJoinAllSpaces, .stationary, .fullScreenAuxiliary, .ignoresCycle]
        return panel
    }
}

/// The mode's symbol in a filled circle, its name, and the headphones with
/// their battery.
private struct PopUpView: View {
    let symbol: String
    let title: String
    let details: String

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: symbol)
                .font(.system(size: 20))
                .foregroundStyle(.white)
                .frame(width: 40, height: 40)
                .background(Circle().fill(Color.accentColor))
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.system(size: 14, weight: .semibold))
                Text(details).font(.system(size: 12)).foregroundStyle(.secondary)
            }
        }
        .padding(.leading, 14)
        .padding(.trailing, 22)
        .padding(.vertical, 12)
        .accessibilityElement(children: .combine)
    }
}
