import AppKit
let path = CommandLine.arguments[1]
let image = NSBitmapImageRep(data: try Data(contentsOf: URL(fileURLWithPath: path)))!
func alpha(_ x: Int, _ y: Int) -> CGFloat { image.colorAt(x: x, y: y)!.alphaComponent }
let scale = image.pixelsWide / 324
let outside = alpha(2 * scale, 30 * scale)
let surface = alpha(122 * scale, 46 * scale)
let button = alpha(233 * scale, 25 * scale)
precondition(outside < 0.01, "outside must be transparent: \(outside)")
precondition(surface > 0.60 && surface < 0.64, "body must be translucent: \(surface)")
precondition(button > 0.99, "controls must remain opaque: \(button)")
print("PASS alpha: outside=\(outside), body=\(surface), button=\(button)")
