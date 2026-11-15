// Draws the launcher icon images (app-release design D11): the Malayalam
// syllable "വ്യ" (vya) in the app's slate seed colour (lib/app/theme.dart) on a light
// background. Uses a font that ships with macOS, so the app itself carries no
// font. Run by icons.sh: swift draw.swift <out dir>
//
//   master.png      1024x1024, opaque: the iOS icon and the Play store icon
//   foreground.png  432x432, transparent: Android adaptive icon foreground
//   monochrome.png  432x432, transparent: Android 13 themed icon
//   feature.png     1024x500, opaque: the Play store feature graphic
import AppKit
import CoreText
import Foundation

let seed = CGColor(red: 0x5E / 255, green: 0x6B / 255, blue: 0x78 / 255, alpha: 1)
let background = CGColor(red: 0xF1 / 255, green: 0xF3 / 255, blue: 0xF5 / 255, alpha: 1)
let black = CGColor(red: 0, green: 0, blue: 0, alpha: 1)
let letter = "വ്യ"
let fontName = "MalayalamSangamMN-Bold"

/// Draws `letter` centred on its ink box, its larger side `fill` of the
/// canvas's smaller side.
func draw(
  size: Int, height: Int? = nil, fill: CGFloat, ink: CGColor, bg: CGColor?, to path: String
) throws {
  let width = size
  let height = height ?? size
  let space = CGColorSpace(name: CGColorSpace.sRGB)!
  let info = bg == nil
    ? CGImageAlphaInfo.premultipliedLast.rawValue
    : CGImageAlphaInfo.noneSkipLast.rawValue
  guard let ctx = CGContext(
    data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0,
    space: space, bitmapInfo: info)
  else { throw NSError(domain: "draw", code: 1) }
  let w = CGFloat(width)
  let h = CGFloat(height)
  let side = min(w, h)
  if let bg {
    ctx.setFillColor(bg)
    ctx.fill(CGRect(x: 0, y: 0, width: w, height: h))
  }

  // Lay the letter out at a reference size, measure its ink, then scale.
  func line(_ points: CGFloat) -> CTLine {
    let font = CTFontCreateWithName(fontName as CFString, points, nil)
    let attrs: [NSAttributedString.Key: Any] = [
      .font: font, .foregroundColor: ink,
    ]
    return CTLineCreateWithAttributedString(NSAttributedString(string: letter, attributes: attrs))
  }
  let ref = line(100)
  let box = CTLineGetImageBounds(ref, ctx)
  let scale = fill * side / max(box.width, box.height)
  let real = line(100 * scale)
  let ink = CTLineGetImageBounds(real, ctx)
  ctx.textPosition = CGPoint(x: (w - ink.width) / 2 - ink.minX, y: (h - ink.height) / 2 - ink.minY)
  CTLineDraw(real, ctx)

  guard let image = ctx.makeImage() else { throw NSError(domain: "draw", code: 2) }
  let rep = NSBitmapImageRep(cgImage: image)
  guard let png = rep.representation(using: .png, properties: [:]) else {
    throw NSError(domain: "draw", code: 3)
  }
  try png.write(to: URL(fileURLWithPath: path))
}

let out = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "."
// iOS masks the square to its own rounded shape: the letter fills ~58%.
try draw(size: 1024, fill: 0.58, ink: seed, bg: background, to: "\(out)/master.png")
// Adaptive icons show the inner 66% (288 of 432 px); keep the letter inside it.
try draw(size: 432, fill: 0.50, ink: seed, bg: nil, to: "\(out)/foreground.png")
try draw(size: 432, fill: 0.50, ink: black, bg: nil, to: "\(out)/monochrome.png")
try draw(size: 1024, height: 500, fill: 0.56, ink: seed, bg: background, to: "\(out)/feature.png")
