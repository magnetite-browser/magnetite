// Usage: drive <binary> <script> [args...]
// Script: semicolon-separated steps:
//   wait <sec> | click <x>,<y> (window-relative, DIP) | rclick <x>,<y> | dblclick <x>,<y>
//   key <combo> (e.g. cmd+t, cmd+shift+t, enter, esc, ctrl+tab, cmd+alt+right)
//   type <text> | shot <file> (region of the largest app window) | list
import Foundation
import CoreGraphics
import AppKit

let a = CommandLine.arguments
let bin = a[1], script = a[2]
let extra = Array(a.dropFirst(3))

func windows(pid: Int32) -> [(id: Int, x: Double, y: Double, w: Double, h: Double, name: String)] {
    guard let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] else { return [] }
    return list.compactMap { w in
        guard let p = w[kCGWindowOwnerPID as String] as? Int32, p == pid,
              let id = w[kCGWindowNumber as String] as? Int,
              let b = w[kCGWindowBounds as String] as? [String: Double],
              let layer = w[kCGWindowLayer as String] as? Int, layer == 0 else { return nil }
        return (id, b["X"] ?? 0, b["Y"] ?? 0, b["Width"] ?? 0, b["Height"] ?? 0, (w[kCGWindowName as String] as? String) ?? "")
    }.filter { $0.w > 100 && $0.h > 100 }
}
func frame(pid: Int32) -> (x: Double, y: Double, w: Double, h: Double)? {
    windows(pid: pid).max(by: { $0.w * $0.h < $1.w * $1.h }).map { ($0.x, $0.y, $0.w, $0.h) }
}

let p = Process()
p.executableURL = URL(fileURLWithPath: bin)
p.arguments = extra
try! p.run()
let pid = p.processIdentifier
// wait for the first window
var t0 = Date()
while Date().timeIntervalSince(t0) < 20 && frame(pid: pid) == nil { usleep(20000) }
if let app = NSRunningApplication(processIdentifier: pid) { app.activate(options: [.activateIgnoringOtherApps]) }

func post(_ e: CGEvent?) { e?.post(tap: .cghidEventTap); usleep(30000) }
func mouse(_ type: CGEventType, _ pt: CGPoint, button: CGMouseButton, clicks: Int = 1) {
    let e = CGEvent(mouseEventSource: nil, mouseType: type, mouseCursorPosition: pt, mouseButton: button)
    e?.setIntegerValueField(.mouseEventClickState, value: Int64(clicks))
    post(e)
}
var mouseFlags: CGEventFlags = []
func mouseEv(_ type: CGEventType, _ pt: CGPoint, button: CGMouseButton, clicks: Int) {
    let e = CGEvent(mouseEventSource: nil, mouseType: type, mouseCursorPosition: pt, mouseButton: button)
    e?.setIntegerValueField(.mouseEventClickState, value: Int64(clicks))
    e?.flags = mouseFlags
    post(e)
}
func click(_ x: Double, _ y: Double, button: CGMouseButton = .left, clicks: Int = 1, flags: CGEventFlags = []) {
    guard let f = frame(pid: pid) else { print("no window"); return }
    let pt = CGPoint(x: f.x + x, y: f.y + y)
    post(CGEvent(mouseEventSource: nil, mouseType: .mouseMoved, mouseCursorPosition: pt, mouseButton: .left))
    usleep(80000)
    mouseFlags = flags
    let down: CGEventType = button == .left ? .leftMouseDown : .rightMouseDown
    let up: CGEventType = button == .left ? .leftMouseUp : .rightMouseUp
    for c in 1...clicks { mouseEv(down, pt, button: button, clicks: c); mouseEv(up, pt, button: button, clicks: c) }
    mouseFlags = []
}
let keycodes: [String: CGKeyCode] = ["a":0,"s":1,"d":2,"f":3,"h":4,"g":5,"z":6,"x":7,"c":8,"v":9,"b":11,"q":12,"w":13,"e":14,"r":15,"y":16,"t":17,"1":18,"2":19,"3":20,"4":21,"6":22,"5":23,"=":24,"9":25,"7":26,"-":27,"8":28,"0":29,"o":31,"u":32,"i":34,"p":35,"l":37,"j":38,"k":40,"n":45,"m":46,"enter":36,"return":36,"tab":48,"space":49,"esc":53,"escape":53,"left":123,"right":124,"down":125,"up":126,"delete":51,"backspace":51,",":43,".":47,"/":44,";":41]
func key(_ combo: String) {
    var flags: CGEventFlags = []
    var keyName = ""
    for part in combo.lowercased().split(separator: "+") {
        switch part {
        case "cmd", "command": flags.insert(.maskCommand)
        case "shift": flags.insert(.maskShift)
        case "alt", "option": flags.insert(.maskAlternate)
        case "ctrl", "control": flags.insert(.maskControl)
        default: keyName = String(part)
        }
    }
    guard let code = keycodes[keyName] else { print("unknown key \(keyName)"); return }
    let d = CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: true); d?.flags = flags; post(d)
    let u = CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: false); u?.flags = flags; post(u)
    // Release modifiers explicitly so later events do not inherit them.
    for (mod, vk) in [(CGEventFlags.maskCommand, CGKeyCode(55)), (.maskShift, 56), (.maskAlternate, 58), (.maskControl, 59)] where flags.contains(mod) {
        let r = CGEvent(keyboardEventSource: nil, virtualKey: vk, keyDown: false); r?.type = .flagsChanged; r?.flags = []; post(r)
    }
}
func type(_ text: String) {
    for ch in text.unicodeScalars {
        var u = [UniChar(ch.value)]
        let d = CGEvent(keyboardEventSource: nil, virtualKey: 0, keyDown: true); d?.flags = []; d?.keyboardSetUnicodeString(stringLength: 1, unicodeString: &u); post(d)
        let up = CGEvent(keyboardEventSource: nil, virtualKey: 0, keyDown: false); up?.flags = []; up?.keyboardSetUnicodeString(stringLength: 1, unicodeString: &u); post(up)
    }
}
func shot(_ file: String) {
    guard let f = frame(pid: pid) else { print("no window for shot"); return }
    let sc = Process(); sc.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
    sc.arguments = ["-x", "-R", "\(Int(f.x)),\(Int(f.y)),\(Int(f.w)),\(Int(f.h))", file]; try! sc.run(); sc.waitUntilExit()
    print("shot \(file)")
}

for raw in script.split(separator: ";") {
    let step = raw.trimmingCharacters(in: .whitespaces)
    guard !step.isEmpty else { continue }
    let parts = step.split(separator: " ", maxSplits: 1).map(String.init)
    let arg = parts.count > 1 ? parts[1] : ""
    switch parts[0] {
    case "wait": Thread.sleep(forTimeInterval: Double(arg) ?? 1)
    case "click", "rclick", "dblclick", "shiftclick", "altclick":
        let xy = arg.split(separator: ",").compactMap { Double($0.trimmingCharacters(in: .whitespaces)) }
        let fl: CGEventFlags = parts[0] == "shiftclick" ? [.maskShift] : parts[0] == "altclick" ? [.maskAlternate] : []
        if xy.count == 2 { click(xy[0], xy[1], button: parts[0] == "rclick" ? .right : .left, clicks: parts[0] == "dblclick" ? 2 : 1, flags: fl) }
    case "key": key(arg)
    case "type": type(arg)
    case "shot": shot(arg)
    case "list": for w in windows(pid: pid) { print("window \(w.id) \(Int(w.x)),\(Int(w.y)) \(Int(w.w))x\(Int(w.h)) \(w.name)") }
    default: print("unknown step \(step)")
    }
}
p.terminate()
p.waitUntilExit()
