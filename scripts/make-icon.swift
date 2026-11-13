// Reproducible vector artwork, rendered with AppKit into the macOS iconset.
import AppKit
let destination = CommandLine.arguments[1]
try FileManager.default.createDirectory(atPath: destination, withIntermediateDirectories: true)
func color(_ hex: UInt32, _ alpha: CGFloat = 1) -> NSColor {
    NSColor(srgbRed: CGFloat((hex >> 16) & 255)/255, green: CGFloat((hex >> 8) & 255)/255, blue: CGFloat(hex & 255)/255, alpha: alpha)
}
func rounded(_ rect: NSRect, _ radius: CGFloat) -> NSBezierPath { NSBezierPath(roundedRect: rect, xRadius: radius, yRadius: radius) }
func render(_ px: Int, _ name: String) throws {
    let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: px, pixelsHigh: px, bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    let transform = NSAffineTransform(); transform.scale(by: CGFloat(px)/1024); transform.concat()
    // Standard macOS padded rounded-square silhouette.
    NSGraphicsContext.saveGraphicsState()
    let shadow = NSShadow(); shadow.shadowColor = color(0x000000, 0.30); shadow.shadowBlurRadius = 30; shadow.shadowOffset = NSSize(width: 0, height: -12); shadow.set()
    let base = rounded(NSRect(x: 92, y: 92, width: 840, height: 840), 190)
    NSGradient(starting: color(0x141d28), ending: color(0x344b5a))!.draw(in: base, angle: 70)
    NSGraphicsContext.restoreGraphicsState()
    color(0xffffff, 0.12).setStroke(); base.lineWidth = 2; base.stroke()
    // Two offset cards make “saved image history” readable without lettering.
    let rear = rounded(NSRect(x: 258, y: 344, width: 550, height: 420), 62)
    color(0x78cabb, 0.38).setFill(); rear.fill()
    color(0xc4fff1, 0.22).setStroke(); rear.lineWidth = 3; rear.stroke()
    NSGraphicsContext.saveGraphicsState()
    let cardShadow = NSShadow(); cardShadow.shadowColor = color(0x000000, 0.35); cardShadow.shadowBlurRadius = 28; cardShadow.shadowOffset = NSSize(width: 0, height: -14); cardShadow.set()
    let card = rounded(NSRect(x: 216, y: 282, width: 550, height: 420), 62)
    color(0xe6fff8).setFill(); card.fill()
    NSGraphicsContext.restoreGraphicsState()
    // A landscape, cropped cleanly into the front photograph.
    NSGraphicsContext.saveGraphicsState()
    let photo = rounded(NSRect(x: 240, y: 306, width: 502, height: 372), 42); photo.addClip()
    NSGradient(starting: color(0x155d70), ending: color(0x64c6c2))!.draw(in: photo, angle: 90)
    color(0xe4fff0).setFill(); NSBezierPath(ovalIn: NSRect(x: 577, y: 528, width: 74, height: 74)).fill()
    let back = NSBezierPath(); back.move(to: NSPoint(x: 225,y:306)); back.line(to: NSPoint(x:225,y:410)); back.curve(to:NSPoint(x:520,y:444),controlPoint1:NSPoint(x:340,y:580),controlPoint2:NSPoint(x:420,y:540)); back.curve(to:NSPoint(x:760,y:468),controlPoint1:NSPoint(x:620,y:350),controlPoint2:NSPoint(x:660,y:515)); back.line(to:NSPoint(x:760,y:306)); back.close(); color(0x35a997).setFill(); back.fill()
    let front = NSBezierPath(); front.move(to:NSPoint(x:225,y:306)); front.line(to:NSPoint(x:225,y:355)); front.curve(to:NSPoint(x:760,y:400),controlPoint1:NSPoint(x:470,y:300),controlPoint2:NSPoint(x:530,y:510)); front.line(to:NSPoint(x:760,y:306)); front.close(); color(0x134f57).setFill(); front.fill()
    NSGraphicsContext.restoreGraphicsState()
    // The short shelf below the stack echoes the app's selection marker.
    color(0x9ee8d9).setFill(); rounded(NSRect(x: 352, y: 208, width: 320, height: 18), 9).fill()
    NSGraphicsContext.restoreGraphicsState()
    try rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: destination).appendingPathComponent(name))
}
for size in [16,32,128,256,512] {
    try render(size, "icon_\(size)x\(size).png")
    try render(size*2, "icon_\(size)x\(size)@2x.png")
}
