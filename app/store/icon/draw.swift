// Draws the launcher icon images (app-release design D11, restyled by the
// arcade-restyle change): the Malayalam syllable "വ്യ" (vya) in the Arcade
// yellow with a hard red offset shadow on the Arcade navy (lib/app/arcade/tokens.dart),
// inside a white square frame. Uses a font that ships with macOS, so the app itself carries no
// font. Run by icons.sh: swift draw.swift <out dir>
//
//   master.png      1024x1024, opaque: the iOS icon and the Play store icon
//   foreground.png  432x432, transparent: Android adaptive icon foreground
//   monochrome.png  432x432, transparent: Android 13 themed icon
//   feature.png     1024x500, opaque: the Play store feature graphic
import AppKit
import CoreText
import Foundation

let seed = CGColor(red: 1, green: 1, blue: 0, alpha: 1)  // Arcade yellow
let shadow = CGColor(red: 1, green: 0, blue: 0, alpha: 1)  // the heading's red shadow
let frame = CGColor(red: 1, green: 1, blue: 1, alpha: 1)
let background = CGColor(red: 0x0F / 255, green: 0x0F / 255, blue: 0x23 / 255, alpha: 1)
let black = CGColor(red: 0, green: 0, blue: 0, alpha: 1)
let letter = "വ്യ"
let fontName = "MalayalamSangamMN-Bold"

/// Draws `letter` centred on its ink box, its larger side `fill` of the
/// canvas's smaller side.
func draw(
  size: Int, height: Int? = nil, fill: CGFloat, ink: CGColor, bg: CGColor?,
  shadow shadowColor: CGColor? = nil, frame frameColor: CGColor? = nil, to path: String
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
  func line2(_ points: CGFloat, _ color: CGColor) -> CTLine {
    let font = CTFontCreateWithName(fontName as CFString, points, nil)
    let attrs: [NSAttributedString.Key: Any] = [.font: font, .foregroundColor: color]
    return CTLineCreateWithAttributedString(NSAttributedString(string: letter, attributes: attrs))
  }
  let ref = line(100)
  let box = CTLineGetImageBounds(ref, ctx)
  let scale = fill * side / max(box.width, box.height)
  let real = line(100 * scale)
  let ink = CTLineGetImageBounds(real, ctx)
  // A hard offset shadow (no blur) down and to the right, then the letter.
  let x = (w - ink.width) / 2 - ink.minX
  let y = (h - ink.height) / 2 - ink.minY
  if shadowColor != nil {
    let off = side * 0.035
    ctx.setFillColor(shadowColor!)
    ctx.textPosition = CGPoint(x: x + off, y: y - off)
    CTLineDraw(line2(100 * scale, shadowColor!), ctx)
  }
  ctx.textPosition = CGPoint(x: x, y: y)
  CTLineDraw(real, ctx)
  // A thick square frame (the Arcade 4px border), when asked for.
  if let frameColor {
    let t = side * 0.045
    ctx.setStrokeColor(frameColor)
    ctx.setLineWidth(t)
    ctx.stroke(CGRect(x: side * 0.06 + t / 2, y: side * 0.06 + t / 2,
                      width: w - 2 * (side * 0.06 + t / 2), height: h - 2 * (side * 0.06 + t / 2)))
  }

  guard let image = ctx.makeImage() else { throw NSError(domain: "draw", code: 2) }
  let rep = NSBitmapImageRep(cgImage: image)
  guard let png = rep.representation(using: .png, properties: [:]) else {
    throw NSError(domain: "draw", code: 3)
  }
  try png.write(to: URL(fileURLWithPath: path))
}

let out = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "."
// iOS masks the square to its own rounded shape: the letter fills ~58%.
try draw(
  size: 1024, fill: 0.50, ink: seed, bg: background, shadow: shadow, frame: frame,
  to: "\(out)/master.png")
// Adaptive icons show the inner 66% (288 of 432 px); keep the letter inside it.
try draw(size: 432, fill: 0.46, ink: seed, bg: nil, shadow: shadow, to: "\(out)/foreground.png")
try draw(size: 432, fill: 0.50, ink: black, bg: nil, to: "\(out)/monochrome.png")
try draw(
  size: 1024, height: 500, fill: 0.52, ink: seed, bg: background, shadow: shadow, frame: frame,
  to: "\(out)/feature.png")
