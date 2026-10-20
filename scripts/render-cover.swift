// Renders the README cover: the icon, name and tagline on the left, and the
// real menu and equalizer pictures on the right. Run it after retaking those.
//   swift scripts/render-cover.swift assets/icon.svg docs/images
import AppKit

let arguments = CommandLine.arguments
guard arguments.count == 3 else { fatalError("usage: render-cover.swift <icon.svg> <images folder>") }
let folder = URL(fileURLWithPath: arguments[2])

let width: CGFloat = 2400, height: CGFloat = 1350
let background = NSColor(srgbRed: 0.18, green: 0.17, blue: 0.40, alpha: 1)
let ink = NSColor(srgbRed: 0.97, green: 0.95, blue: 0.90, alpha: 1)

func rounded(_ size: CGFloat, _ weight: NSFont.Weight) -> NSFont {
    let font = NSFont.systemFont(ofSize: size, weight: weight)
    return font.fontDescriptor.withDesign(.rounded).flatMap { NSFont(descriptor: $0, size: size) } ?? font
}

/// Draws lines of text downwards from `top`, `spacing` font sizes apart.
func write(_ lines: [String], font: NSFont, spacing: CGFloat, x: CGFloat, top: CGFloat) {
    for (index, line) in lines.enumerated() {
        let text = NSAttributedString(string: line, attributes: [.font: font, .foregroundColor: ink])
        text.draw(at: NSPoint(x: x, y: top - font.pointSize * (1 + spacing * CGFloat(index))))
    }
}

/// Draws one of the README pictures, saved at 2x, at `scale` of its pixel
/// size with its top left corner at `x`, `top`.
func place(_ name: String, x: CGFloat, top: CGFloat, scale: CGFloat) {
    let url = folder.appendingPathComponent(name)
    guard let data = try? Data(contentsOf: url), let picture = NSBitmapImageRep(data: data) else {
        fatalError("can't read \(url.path)")
    }
    let size = NSSize(width: CGFloat(picture.pixelsWide) * scale, height: CGFloat(picture.pixelsHigh) * scale)
    picture.draw(
        in: NSRect(origin: NSPoint(x: x, y: top - size.height), size: size), from: .zero, operation: .sourceOver,
        fraction: 1, respectFlipped: false, hints: nil)
}

let cover = NSBitmapImageRep(
    bitmapDataPlanes: nil, pixelsWide: Int(width), pixelsHigh: Int(height), bitsPerSample: 8, samplesPerPixel: 4,
    hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: cover)
// AppKit draws from the bottom left.
background.setFill()
NSRect(x: 0, y: 0, width: width, height: height).fill()

guard let icon = NSImage(contentsOf: URL(fileURLWithPath: arguments[1])) else { fatalError("can't read the icon") }
icon.draw(in: NSRect(x: 70, y: height - 600, width: 520, height: 520))
write(["Sound", "Connect"], font: rounded(196, .heavy), spacing: 1, x: 132, top: height - 600)
write(["Your Headphones,", "One Click Away."], font: rounded(70, .bold), spacing: 1.2, x: 140, top: 330)

// The equalizer behind, the menu in front of it.
place("equalizer-dark.png", x: 1470, top: height - 280, scale: 0.95)
place("menu-dark.png", x: 960, top: height - 70, scale: 1.1)

NSGraphicsContext.restoreGraphicsState()
try cover.representation(using: .png, properties: [:])!.write(to: folder.appendingPathComponent("cover.png"))
