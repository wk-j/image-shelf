// Safe fixture test: preserve pasteboard bytes in memory, restore only if still ours.
import AppKit
let root = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
let board = NSPasteboard.general
let saved = (board.pasteboardItems ?? []).map { item in item.types.compactMap { t -> (NSPasteboard.PasteboardType,Data)? in item.data(forType:t).map { (t,$0) } } }
let image = NSBitmapImageRep(bitmapDataPlanes:nil,pixelsWide:32,pixelsHigh:24,bitsPerSample:8,samplesPerPixel:4,hasAlpha:true,isPlanar:false,colorSpaceName:.deviceRGB,bytesPerRow:0,bitsPerPixel:0)!
for y in 0..<24 { for x in 0..<32 { image.setColor(.init(deviceRed:0.1,green:0.7,blue:0.6,alpha:1),atX:x,y:y) } }
let png = image.representation(using:.png,properties:[:])!
board.clearContents(); board.setData(png,forType:.png)
let ours = board.changeCount
defer {
 if board.changeCount == ours {
  board.clearContents()
  let items = saved.map { pairs -> NSPasteboardItem in let item=NSPasteboardItem(); for (t,d) in pairs {item.setData(d,forType:t)};return item }
  board.writeObjects(items)
 }
}
let testDir = root.appendingPathComponent("out/clipboard-smoke-\(UUID().uuidString)")
let process = Process(); process.executableURL = root.appendingPathComponent("out/Image Shelf.app/Contents/MacOS/image-shelf")
var env = ProcessInfo.processInfo.environment; env["IMAGE_SHELF_DIR"] = testDir.path; process.environment = env
try process.run(); Thread.sleep(forTimeInterval:4)
process.terminate(); process.waitUntilExit()
let files = try FileManager.default.contentsOfDirectory(at:testDir,includingPropertiesForKeys:nil).filter {$0.pathExtension == "png"}
guard files.count == 1, try Data(contentsOf:files[0]) == png else {fatalError("Capture mismatch")}
print("PASS: native clipboard PNG captured once and persisted, original pasteboard restored if unchanged")
