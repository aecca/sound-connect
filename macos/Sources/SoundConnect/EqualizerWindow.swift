import AppKit
import SoundConnectCore
import SwiftUI

/// A small window to shape the manual equalizer.
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

                switch equalizer.bands {
                case .five(let clearBass, let bands):
                    fiveBands(clearBass: clearBass, bands: bands)
                case .ten(let bands):
                    tenBands(Array(bands))
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
        .fixedSize()
    }

    private func fiveBands(clearBass: Int8, bands: [Int8]) -> some View {
        let frequencies = ["400", "1k", "2.5k", "6.3k", "16k"]
        return HStack(alignment: .bottom, spacing: 14) {
            ForEach(bands.indices, id: \.self) { index in
                BandSlider(
                    label: frequencies[safe: index] ?? "",
                    range: -10...10,
                    value: Binding(
                        get: { Double(bands[index]) },
                        set: { value in
                            var bands = bands
                            bands[index] = Int8(value.rounded())
                            model.change(.equalizerBands(bands: .five(clearBass: clearBass, bands: bands)))
                        }
                    )
                )
            }
            Divider().frame(height: 170)
            BandSlider(
                label: "Clear Bass",
                range: -10...10,
                value: Binding(
                    get: { Double(clearBass) },
                    set: { model.change(.equalizerBands(bands: .five(clearBass: Int8($0.rounded()), bands: bands))) }
                )
            )
        }
    }

    /// The WH-1000XM6's ten bands use raw device values (0 to 12) whose dB
    /// scale isn't known yet, so they show no numbers.
    private func tenBands(_ bands: [UInt8]) -> some View {
        let frequencies = ["31", "63", "125", "250", "500", "1k", "2k", "4k", "8k", "16k"]
        return VStack(alignment: .leading, spacing: 10) {
            HStack(alignment: .bottom, spacing: 8) {
                ForEach(bands.indices, id: \.self) { index in
                    BandSlider(
                        label: frequencies[safe: index] ?? "",
                        range: 0...12,
                        showsValue: false,
                        value: Binding(
                            get: { Double(bands[index]) },
                            set: { value in
                                var bands = bands
                                bands[index] = UInt8(value.rounded())
                                model.change(.equalizerBands(bands: .ten(bands: Data(bands))))
                            }
                        )
                    )
                }
            }
            Text("Ten-band support hasn't been tested on real headphones yet.")
                .font(.callout)
                .foregroundStyle(.secondary)
        }
    }

    private func presetBinding(_ equalizer: Equalizer) -> Binding<EqPreset> {
        Binding(get: { equalizer.preset }, set: { model.change(.equalizerPreset(preset: $0)) })
    }
}

/// A vertical slider with its value on top and a label below.
private struct BandSlider: View {
    let label: String
    let range: ClosedRange<Double>
    var showsValue = true
    @Binding var value: Double

    var body: some View {
        VStack(spacing: 6) {
            Text(value > 0 ? "+\(Int(value))" : "\(Int(value))")
                .font(.caption.monospacedDigit())
                .foregroundStyle(.secondary)
                .opacity(showsValue ? 1 : 0)
            Slider(value: $value, in: range, step: 1)
                .rotationEffect(.degrees(-90))
                .frame(width: 150, height: 24)
                .frame(width: 32, height: 150)
            Text(label)
                .font(.caption)
                .fixedSize()
        }
        .frame(width: 40)
    }
}

private extension Array {
    subscript(safe index: Int) -> Element? {
        indices.contains(index) ? self[index] : nil
    }
}
