// Renders the SVG app icon to a square PNG, using AppKit's own SVG support.
//   swift scripts/render-icon.swift assets/icon.svg out.png 512
import AppKit

let arguments = CommandLine.arguments
guard arguments.count == 4, let size = Int(arguments[3]),
      let image = NSImage(contentsOf: URL(fileURLWithPath: arguments[1])) else {
    fatalError("usage: render-icon.swift <in.svg> <out.png> <size>")
}
let bitmap = NSBitmapImageRep(
    bitmapDataPlanes: nil, pixelsWide: size, pixelsHigh: size, bitsPerSample: 8, samplesPerPixel: 4,
    hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: bitmap)
image.draw(in: NSRect(x: 0, y: 0, width: size, height: size))
NSGraphicsContext.restoreGraphicsState()
try bitmap.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: arguments[2]))
