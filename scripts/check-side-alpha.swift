import AppKit
for name in ["left", "right", "left-expanded", "right-expanded"] {
  let image = NSBitmapImageRep(data: try Data(contentsOf: URL(fileURLWithPath: "out/e2e/\(name).png")))!
  let w = name.hasSuffix("-expanded") ? 384 : 96
  let scale = image.pixelsWide / w
  func alpha(_ x: Int, _ y: Int) -> CGFloat { image.colorAt(x: x * scale, y: y * scale)!.alphaComponent }
  let right = name.hasPrefix("right")
  let edgeMarginX = right ? w - 1 : 1
  let railMidX = right ? w - 46 : 46
  precondition(alpha(w / 2, 1) < 0.08, "top floating margin must remain transparent")
  precondition(alpha(edgeMarginX, 2) < 0.05, "outer corner margin must remain transparent")
  precondition(alpha(edgeMarginX, 158) < 0.35, "floating edge inset keeps soft shadow clearance")
  precondition(alpha(railMidX, 22) > 0.70 && alpha(railMidX, 22) < 0.96, "Liquid Glass header retains smoked-quartz translucent surface")
  if name.hasSuffix("-expanded") {
    let inwardGlassX = right ? 22 : w - 22
    precondition(alpha(inwardGlassX, 22) > 0.70 && alpha(inwardGlassX, 22) < 0.96, "inward Liquid Glass studio retains smoked-quartz translucent surface")
  }
  print("PASS \(name): floating Liquid Glass capsule margins, rounded corners, and smoked-quartz body")
}
