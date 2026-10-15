// List on-screen windows (id, owner, title, bounds) so a capture can target the Steam Remote
// Play stream of the original game. Usage: swift tools/reference/windows.swift [OWNER-SUBSTRING]
import CoreGraphics
import Foundation

let filter = CommandLine.arguments.count > 1 ? CommandLine.arguments[1].lowercased() : ""
let options: CGWindowListOption = [.optionOnScreenOnly, .excludeDesktopElements]
guard let list = CGWindowListCopyWindowInfo(options, kCGNullWindowID) as? [[String: Any]] else {
    exit(1)
}
for w in list {
    let owner = w[kCGWindowOwnerName as String] as? String ?? ""
    let title = w[kCGWindowName as String] as? String ?? ""
    if !filter.isEmpty && !owner.lowercased().contains(filter) && !title.lowercased().contains(filter) {
        continue
    }
    let id = w[kCGWindowNumber as String] as? Int ?? 0
    let layer = w[kCGWindowLayer as String] as? Int ?? 0
    let b = w[kCGWindowBounds as String] as? [String: Double] ?? [:]
    print("\(id)\t\(owner)\t\(title)\t\(Int(b["Width"] ?? 0))x\(Int(b["Height"] ?? 0))\tlayer \(layer)")
}
