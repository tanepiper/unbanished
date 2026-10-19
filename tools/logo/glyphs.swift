// Usage: swift glyphs.swift FONT.ttf WEIGHT SIZE TRACKING TEXT
// Prints: advance-width, then the SVG path data of TEXT laid out from (0, 0) baseline, y down.
import CoreText
import Foundation

let a = CommandLine.arguments
let url = URL(fileURLWithPath: a[1]) as CFURL
let weight = Double(a[2])!
let size = CGFloat(Double(a[3])!)
let tracking = CGFloat(Double(a[4])!)
let text = a[5]

let descs = CTFontManagerCreateFontDescriptorsFromURL(url) as! [CTFontDescriptor]
let wght: UInt32 = 0x77676874  // 'wght'
let attrs = [kCTFontVariationAttribute: [wght: weight]] as CFDictionary
let desc = CTFontDescriptorCreateCopyWithAttributes(descs[0], attrs)
let font = CTFontCreateWithFontDescriptor(desc, size, nil)

let attr = NSAttributedString(string: text, attributes: [
  NSAttributedString.Key(kCTFontAttributeName as String): font,
  NSAttributedString.Key(kCTKernAttributeName as String): tracking,
])
let line = CTLineCreateWithAttributedString(attr)
var out = ""
func f(_ v: CGFloat) -> String { String(format: "%.2f", v) }
for run in CTLineGetGlyphRuns(line) as! [CTRun] {
  let n = CTRunGetGlyphCount(run)
  var glyphs = [CGGlyph](repeating: 0, count: n)
  var pos = [CGPoint](repeating: .zero, count: n)
  CTRunGetGlyphs(run, CFRange(location: 0, length: n), &glyphs)
  CTRunGetPositions(run, CFRange(location: 0, length: n), &pos)
  for i in 0..<n {
    guard let p = CTFontCreatePathForGlyph(font, glyphs[i], nil) else { continue }
    let ox = pos[i].x
    p.applyWithBlock { e in
      let el = e.pointee
      let pt = el.points
      func P(_ k: Int) -> String { "\(f(ox + pt[k].x)) \(f(-pt[k].y))" }
      switch el.type {
      case .moveToPoint: out += "M\(P(0))"
      case .addLineToPoint: out += "L\(P(0))"
      case .addQuadCurveToPoint: out += "Q\(P(0)) \(P(1))"
      case .addCurveToPoint: out += "C\(P(0)) \(P(1)) \(P(2))"
      case .closeSubpath: out += "Z"
      @unknown default: break
      }
    }
  }
}
print(f(CGFloat(CTLineGetTypographicBounds(line, nil, nil, nil))))
print(out)
