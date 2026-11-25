import CoreImage
import Flutter
import UIKit
import Vision

/// photo-sticker: on-device subject cutout on iOS (Segmentation seam and RGBA
/// contract; design S1). Produces a straight-alpha, 8-bit-per-channel RGBA
/// image (sRGB, R,G,B,A byte order, row stride width*4) in which the background
/// is fully transparent and the subject keeps its colour, using Vision's
/// foreground-instance mask (`VNGenerateForegroundInstanceMaskRequest`, iOS
/// 17+). The photo never leaves the device and no model is bundled or
/// downloaded. This plugin ONLY produces the RGBA — it never encodes, encrypts,
/// uploads or touches keys; the Rust core does all of that.
///
/// The method channel (`vyajena/sticker`):
/// - `available` -> Bool: whether the OS has the subject API (iOS 17+).
/// - `cutout({photo: bytes})` -> `{rgba, width, height}`, or a `FlutterError`
///   with code `unsupported` (too old an OS) or `noSubject` (no subject found).
enum Sticker {
  static let channelName = "vyajena/sticker"

  static func handle(_ call: FlutterMethodCall, _ result: @escaping FlutterResult) {
    switch call.method {
    case "available":
      result(available())
    case "cutout":
      guard let args = call.arguments as? [String: Any],
        let data = (args["photo"] as? FlutterStandardTypedData)?.data
      else {
        result(FlutterError(code: "noSubject", message: "no photo", details: nil))
        return
      }
      cutout(data, result)
    default:
      result(FlutterMethodNotImplemented)
    }
  }

  static func available() -> Bool {
    if #available(iOS 17.0, *) { return true }
    return false
  }

  private static func cutout(_ data: Data, _ result: @escaping FlutterResult) {
    guard #available(iOS 17.0, *) else {
      result(FlutterError(code: "unsupported", message: "needs iOS 17", details: nil))
      return
    }
    // Apply the photo's orientation to the pixels up front, so the contract's
    // "orientation already applied" holds and Vision sees an upright image.
    guard let image = UIImage(data: data), let upright = Self.uprightCGImage(image) else {
      result(FlutterError(code: "noSubject", message: "cannot read the photo", details: nil))
      return
    }
    DispatchQueue.global(qos: .userInitiated).async {
      do {
        let request = VNGenerateForegroundInstanceMaskRequest()
        let handler = VNImageRequestHandler(cgImage: upright, options: [:])
        try handler.perform([request])
        guard let observation = request.results?.first,
          !observation.allInstances.isEmpty
        else {
          DispatchQueue.main.async {
            result(FlutterError(code: "noSubject", message: "no subject found", details: nil))
          }
          return
        }
        let buffer = try observation.generateMaskedImage(
          ofInstances: observation.allInstances,
          from: handler,
          croppedToInstancesExtent: true)
        let (rgba, width, height) = try Self.straightAlphaRGBA(buffer)
        DispatchQueue.main.async {
          result([
            "rgba": FlutterStandardTypedData(bytes: rgba),
            "width": width,
            "height": height,
          ])
        }
      } catch {
        DispatchQueue.main.async {
          result(FlutterError(code: "noSubject", message: error.localizedDescription, details: nil))
        }
      }
    }
  }

  /// `image` as an upright `CGImage` with its EXIF orientation baked into the
  /// pixels.
  private static func uprightCGImage(_ image: UIImage) -> CGImage? {
    if image.imageOrientation == .up, let cg = image.cgImage { return cg }
    let format = UIGraphicsImageRendererFormat.default()
    format.scale = 1
    format.opaque = false
    let renderer = UIGraphicsImageRenderer(size: image.size, format: format)
    let drawn = renderer.image { _ in
      image.draw(in: CGRect(origin: .zero, size: image.size))
    }
    return drawn.cgImage
  }

  /// The masked pixel buffer as straight-alpha (non-premultiplied) 8-bit RGBA
  /// bytes in sRGB, R,G,B,A order with no row padding, per the RGBA contract.
  private static func straightAlphaRGBA(_ buffer: CVPixelBuffer) throws -> (Data, Int, Int) {
    let ci = CIImage(cvPixelBuffer: buffer)
    let width = Int(ci.extent.width)
    let height = Int(ci.extent.height)
    let context = CIContext(options: [.workingColorSpace: CGColorSpaceCreateDeviceRGB()])
    let srgb = CGColorSpace(name: CGColorSpace.sRGB) ?? CGColorSpaceCreateDeviceRGB()
    // CoreImage renders premultiplied RGBA8; unpremultiply below for the
    // contract's straight alpha.
    var premul = [UInt8](repeating: 0, count: width * height * 4)
    premul.withUnsafeMutableBytes { raw in
      context.render(
        ci,
        toBitmap: raw.baseAddress!,
        rowBytes: width * 4,
        bounds: ci.extent,
        format: .RGBA8,
        colorSpace: srgb)
    }
    var straight = premul
    var i = 0
    while i < straight.count {
      let a = straight[i + 3]
      if a != 0 && a != 255 {
        let inv = 255.0 / Double(a)
        straight[i] = UInt8(min(255.0, Double(premul[i]) * inv))
        straight[i + 1] = UInt8(min(255.0, Double(premul[i + 1]) * inv))
        straight[i + 2] = UInt8(min(255.0, Double(premul[i + 2]) * inv))
      }
      i += 4
    }
    return (Data(straight), width, height)
  }
}
