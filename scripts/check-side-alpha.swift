import AppKit
for name in ["left", "right", "left-expanded", "right-expanded"] {
  let image=NSBitmapImageRep(data: try Data(contentsOf: URL(fileURLWithPath:"out/e2e/\(name).png")))!
  let w = name.hasSuffix("-expanded") ? 330 : 76
  let scale=image.pixelsWide/w
  func alpha(_ x:Int,_ y:Int)->CGFloat { image.colorAt(x:x*scale,y:y*scale)!.alphaComponent }
  let right=name.hasPrefix("right")
  let inward = right ? 0 : w-1
  let attached = right ? w-1 : 0
  precondition(alpha(inward,2)<0.01,"inward top cutout must be transparent")
  precondition(alpha(w/2,4)<0.01,"band above the rail must be transparent")
  precondition(alpha(attached,6)<0.01,"no shoulder flare on the screen edge")
  precondition(alpha(attached,13)>0.5 && alpha(attached,346)>0.5,"screen-edge corners are square")
  precondition(alpha(attached,180)>0.60 && alpha(attached,180)<0.64,"attached edge must retain translucent surface")
  precondition(alpha(inward,180)>0.60 && alpha(inward,180)<0.64,"inward side body must retain translucent surface")
  print("PASS \(name): flush square screen edge, rounded inward corners, translucent sides")
}
