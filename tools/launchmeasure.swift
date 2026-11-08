// Usage: launchmeasure <binary> <screenshot.png> <settle_seconds> [args...]
// Launches the binary, measures time until its first on-screen window appears,
// waits settle_seconds, screenshots that window, then terminates the process.
import Foundation
import CoreGraphics
import AppKit

let args = CommandLine.arguments
let bin = args[1], shot = args[2], settle = Double(args[3])!
let extra = Array(args.dropFirst(4))

func windowsFor(pid: Int32) -> [(id: Int, w: Double, h: Double)] {
    guard let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] else { return [] }
    return list.compactMap { w in
        guard let p = w[kCGWindowOwnerPID as String] as? Int32, p == pid,
              let id = w[kCGWindowNumber as String] as? Int,
              let b = w[kCGWindowBounds as String] as? [String: Double],
              let layer = w[kCGWindowLayer as String] as? Int, layer == 0 else { return nil }
        return (id, b["Width"] ?? 0, b["Height"] ?? 0)
    }.filter { $0.w > 100 && $0.h > 100 }
}

let p = Process()
p.executableURL = URL(fileURLWithPath: bin)
p.arguments = extra
let t0 = Date()
try! p.run()
var first: Double? = nil
var win: Int? = nil
while Date().timeIntervalSince(t0) < 20 {
    let ws = windowsFor(pid: p.processIdentifier)
    if let w = ws.first { first = Date().timeIntervalSince(t0) * 1000; win = w.id; break }
    usleep(5000)
}
print(String(format: "first_window_ms=%.0f", first ?? -1))
if ProcessInfo.processInfo.environment["LM_ACTIVATE"] == "1" {
    if let app = NSRunningApplication(processIdentifier: p.processIdentifier) { app.activate(options: [.activateIgnoringOtherApps]); print("activated=1") }
}
Thread.sleep(forTimeInterval: settle)
if ProcessInfo.processInfo.environment["LM_LIST"] == "1" {
    if let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] {
        for w in list where (w[kCGWindowOwnerPID as String] as? Int32) == p.processIdentifier {
            let b = w[kCGWindowBounds as String] as? [String: Double] ?? [:]
            print("window id=\(w[kCGWindowNumber as String] ?? 0) layer=\(w[kCGWindowLayer as String] ?? 0) bounds=\(Int(b["X"] ?? 0)),\(Int(b["Y"] ?? 0)) \(Int(b["Width"] ?? 0))x\(Int(b["Height"] ?? 0)) name=\(w[kCGWindowName as String] ?? "") alpha=\(w[kCGWindowAlpha as String] ?? 0)")
        }
    }
}
// Prefer the largest window for the region capture (the frame, not a child tab window).
if ProcessInfo.processInfo.environment["LM_REGION"] == "1" {
    let ws = windowsFor(pid: p.processIdentifier)
    if let big = ws.max(by: { $0.w * $0.h < $1.w * $1.h }) { win = big.id }
}
if let w = win {
    let sc = Process(); sc.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
    if ProcessInfo.processInfo.environment["LM_REGION"] == "1", let ws = windowsFor(pid: p.processIdentifier).first(where: { $0.id == w }),
       let list = CGWindowListCopyWindowInfo([.optionIncludingWindow], CGWindowID(w)) as? [[String: Any]],
       let b = list.first?[kCGWindowBounds as String] as? [String: Double] {
        _ = ws
        sc.arguments = ["-x", "-R", "\(Int(b["X"]!)),\(Int(b["Y"]!)),\(Int(b["Width"]!)),\(Int(b["Height"]!))", shot]
    } else {
        sc.arguments = ["-x", "-o", "-l", String(w), shot]
    }
    try! sc.run(); sc.waitUntilExit()
    print("screenshot=\(shot) status=\(sc.terminationStatus)")
}
// Memory snapshot of this app's process tree only (browser + its helpers).
let ps = Process(); ps.executableURL = URL(fileURLWithPath: "/bin/sh")
ps.arguments = ["-c", "ps -o pid,ppid,rss -ax | awk -v root=\(p.processIdentifier) 'NR>1{pp[$1]=$2; r[$1]=$3} END{for(k in r){x=k; while(x!=\"\" && x!=root && x!=\"1\"){x=pp[x]}; if(x==root){rss+=r[k]; n++}} printf \"procs=%d rss_total_mb=%.0f\\n\", n, rss/1024}'"]
try! ps.run(); ps.waitUntilExit()
p.terminate()
p.waitUntilExit()
