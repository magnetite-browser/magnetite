// Usage: shotwin <pid> <file.png>  — screenshot of the largest on-screen window of a process (region capture).
import Foundation
import CoreGraphics
let pid = Int32(CommandLine.arguments[1])!, file = CommandLine.arguments[2]
guard let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] else { exit(1) }
let wins = list.compactMap { w -> (Double, Double, Double, Double)? in
    guard (w[kCGWindowOwnerPID as String] as? Int32) == pid, let b = w[kCGWindowBounds as String] as? [String: Double], (w[kCGWindowLayer as String] as? Int) == 0 else { return nil }
    return (b["X"]!, b["Y"]!, b["Width"]!, b["Height"]!)
}
guard let f = wins.max(by: { $0.2 * $0.3 < $1.2 * $1.3 }) else { print("no window"); exit(1) }
let sc = Process(); sc.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
sc.arguments = ["-x", "-R", "\(Int(f.0)),\(Int(f.1)),\(Int(f.2)),\(Int(f.3))", file]; try! sc.run(); sc.waitUntilExit()
print("shot \(file) \(Int(f.2))x\(Int(f.3))")
