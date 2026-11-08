// Usage: pixel <png> <x> <y>  -> prints "WxH r g b" (top-left origin, image pixels)
import Foundation
import CoreGraphics
import ImageIO
let a = CommandLine.arguments
let src = CGImageSourceCreateWithURL(URL(fileURLWithPath: a[1]) as CFURL, nil)!
let img = CGImageSourceCreateImageAtIndex(src, 0, nil)!
let x = Int(a[2])!, y = Int(a[3])!
var px = [UInt8](repeating: 0, count: 4)
px.withUnsafeMutableBytes { buf in
    let ctx = CGContext(data: buf.baseAddress, width: 1, height: 1, bitsPerComponent: 8, bytesPerRow: 4,
                        space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    ctx.draw(img, in: CGRect(x: -x, y: -(img.height - 1 - y), width: img.width, height: img.height))
}
print("\(img.width)x\(img.height) \(px[0]) \(px[1]) \(px[2])")
