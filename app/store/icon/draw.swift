// Draws the launcher icon images (design D15, "refined wordmark"): the Malayalam
// syllable "വ്യ" (vya) in the Arcade brand yellow (#E6D84D, the duller yellow the
// theme uses — lib/app/arcade/tokens.dart) with a hard red offset shadow on the
// Arcade navy, tilted a few degrees for the hand-drawn Arcade feel. No frame, so the
// glyph fills more of the tile and reads better at small sizes. Uses a font that ships
// with macOS, so the app itself carries no font. Run by icons.sh: swift draw.swift <out dir>
//
//   master.png      1024x1024, opaque: the iOS icon and the Play store icon
//   foreground.png  432x432, transparent: Android adaptive icon foreground
//   monochrome.png  432x432, transparent: Android 13 themed icon
//   feature.png     1024x500, opaque: the Play store feature graphic
import AppKit
import CoreText
import Foundation

// Arcade brand yellow: duller than pure #FFFF00 (owner review 2: full yellow on navy
// was too harsh for some eyes). Matches _yellow in lib/app/arcade/tokens.dart.
let seed = CGColor(red: 0xE6 / 255, green: 0xD8 / 255, blue: 0x4D / 255, alpha: 1)
let shadow = CGColor(red: 1, green: 0, blue: 0, alpha: 1)  // the heading's red shadow
let background = CGColor(red: 0x0F / 255, green: 0x0F / 255, blue: 0x23 / 255, alpha: 1)
let black = CGColor(red: 0, green: 0, blue: 0, alpha: 1)
let letter = "വ്യ"
let fontName = "MalayalamSangamMN-Bold"
let tilt: CGFloat = -3  // degrees, the hand-drawn Arcade lean

/// Draws `letter` centred on its ink box, its larger side `fill` of the
/// canvas's smaller side, tilted `rotate` degrees about the canvas centre.
func draw(
  size: Int, height: Int? = nil, fill: CGFloat, ink: CGColor, bg: CGColor?,
  shadow shadowColor: CGColor? = nil, rotate: CGFloat = 0, to path: String
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
  func line(_ points: CGFloat, _ color: CGColor) -> CTLine {
    let font = CTFontCreateWithName(fontName as CFString, points, nil)
    let attrs: [NSAttributedString.Key: Any] = [.font: font, .foregroundColor: color]
    return CTLineCreateWithAttributedString(NSAttributedString(string: letter, attributes: attrs))
  }
  let ref = line(100, ink)
  let box = CTLineGetImageBounds(ref, ctx)
  let scale = fill * side / max(box.width, box.height)
  let real = line(100 * scale, ink)
  let inkBox = CTLineGetImageBounds(real, ctx)
  let x = (w - inkBox.width) / 2 - inkBox.minX
  let y = (h - inkBox.height) / 2 - inkBox.minY

  ctx.saveGState()
  if rotate != 0 {
    ctx.translateBy(x: w / 2, y: h / 2)
    ctx.rotate(by: rotate * .pi / 180)
    ctx.translateBy(x: -w / 2, y: -h / 2)
  }
  // A hard offset shadow (no blur) down and to the right, then the letter.
  if let shadowColor {
    let off = side * 0.03
    ctx.setFillColor(shadowColor)
    ctx.textPosition = CGPoint(x: x + off, y: y - off)
    CTLineDraw(line(100 * scale, shadowColor), ctx)
  }
  ctx.setFillColor(ink)
  ctx.textPosition = CGPoint(x: x, y: y)
  CTLineDraw(real, ctx)
  ctx.restoreGState()

  guard let image = ctx.makeImage() else { throw NSError(domain: "draw", code: 2) }
  let rep = NSBitmapImageRep(cgImage: image)
  guard let png = rep.representation(using: .png, properties: [:]) else {
    throw NSError(domain: "draw", code: 3)
  }
  try png.write(to: URL(fileURLWithPath: path))
}

let out = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "."
// iOS masks the square to its own rounded shape: the letter fills ~58% (no frame now).
try draw(
  size: 1024, fill: 0.58, ink: seed, bg: background, shadow: shadow, rotate: tilt,
  to: "\(out)/master.png")
// Adaptive icons show the inner 66% (288 of 432 px); keep the letter inside it.
try draw(size: 432, fill: 0.46, ink: seed, bg: nil, shadow: shadow, rotate: tilt,
         to: "\(out)/foreground.png")
try draw(size: 432, fill: 0.50, ink: black, bg: nil, rotate: tilt, to: "\(out)/monochrome.png")
try draw(
  size: 1024, height: 500, fill: 0.52, ink: seed, bg: background, shadow: shadow, rotate: tilt,
  to: "\(out)/feature.png")
