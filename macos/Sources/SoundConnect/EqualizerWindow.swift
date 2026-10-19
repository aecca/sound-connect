import AppKit
import SoundConnectCore
import SwiftUI

/// A small window to shape the manual equalizer: five bands and Clear Bass.
@MainActor
enum EqualizerWindow {
    private static var window: NSWindow?

    static func show(model: HeadsetModel) {
        if window == nil {
            let window = NSWindow(contentViewController: NSHostingController(rootView: EqualizerView(model: model)))
            window.title = "Equalizer"
            window.styleMask = [.titled, .closable]
            window.isReleasedWhenClosed = false
            window.center()
            self.window = window
        }
        // The app has no Dock icon, so bring it forward explicitly.
        NSApp.activate()
        window?.makeKeyAndOrderFront(nil)
    }
}

private struct EqualizerView: View {
    private static let frequencies = ["400", "1k", "2.5k", "6.3k", "16k"]

    let model: HeadsetModel

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            if let equalizer = model.equalizer {
                Picker("Preset", selection: presetBinding(equalizer)) {
                    ForEach(eqPresets(), id: \.self) { preset in
                        Text(eqPresetName(preset: preset)).tag(preset)
                    }
                }
                .frame(width: 220)

                HStack(alignment: .bottom, spacing: 14) {
                    ForEach(0..<5, id: \.self) { index in
                        BandSlider(label: Self.frequencies[index], value: bandBinding(equalizer, index))
                    }
                    Divider().frame(height: 170)
                    BandSlider(label: "Clear Bass", value: clearBassBinding(equalizer))
                }

                Text("Moving a slider switches the preset to Manual.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            } else {
                Text("Connect your headphones to change the equalizer.")
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, minHeight: 200)
            }
        }
        .padding(20)
        .frame(width: 440)
    }

    private func presetBinding(_ equalizer: Equalizer) -> Binding<EqPreset> {
        Binding(get: { equalizer.preset }, set: { model.setPreset($0) })
    }

    private func bandBinding(_ equalizer: Equalizer, _ index: Int) -> Binding<Double> {
        Binding(
            get: { Double(equalizer.bands[index]) },
            set: { value in
                var bands = equalizer.bands
                bands[index] = Int8(value.rounded())
                model.setBands(clearBass: equalizer.clearBass, bands: bands)
            }
        )
    }

    private func clearBassBinding(_ equalizer: Equalizer) -> Binding<Double> {
        Binding(
            get: { Double(equalizer.clearBass) },
            set: { model.setBands(clearBass: Int8($0.rounded()), bands: equalizer.bands) }
        )
    }
}

/// A vertical slider from -10 to +10 with its value on top and a label below.
private struct BandSlider: View {
    let label: String
    @Binding var value: Double

    var body: some View {
        VStack(spacing: 6) {
            Text(value > 0 ? "+\(Int(value))" : "\(Int(value))")
                .font(.caption.monospacedDigit())
                .foregroundStyle(.secondary)
            Slider(value: $value, in: -10...10, step: 1)
                .rotationEffect(.degrees(-90))
                .frame(width: 150, height: 24)
                .frame(width: 32, height: 150)
            Text(label)
                .font(.caption)
                .fixedSize()
        }
        .frame(width: 44)
    }
}
