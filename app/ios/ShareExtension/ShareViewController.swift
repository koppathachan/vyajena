// Share Extension (change `message-actions-share`): makes Vyajena appear in the
// iOS share sheet for text and images. It only moves the RAW shared item into
// the App Group container and opens the host app — it holds no key, no
// ciphertext and does no encryption. The host app reads the item and does all
// encryption and sending through the Rust core.
//
// This target is added to the Xcode project by the owner (its own bundle id,
// provisioning profile and App Group); see README.md. The App Group id and URL
// scheme here must match the Runner's.
import UIKit
import UniformTypeIdentifiers

/// The App Group shared by the Runner and this extension. Must match
/// `SharedInbox.appGroup` in the Runner's AppDelegate and be registered on both
/// targets in the Apple Developer portal (README.md).
private let appGroupId = "group.com.purplechaamba.vyajena"

/// The custom URL the extension opens to hand control back to the host app. Must
/// match the Runner's `CFBundleURLTypes` entry.
private let hostURL = "vyajena-share://shared"

final class ShareViewController: UIViewController {
  override func viewDidLoad() {
    super.viewDidLoad()
    handleShare()
  }

  private func handleShare() {
    guard
      let item = extensionContext?.inputItems.first as? NSExtensionItem,
      let attachment = item.attachments?.first
    else {
      return finish()
    }
    if attachment.hasItemConformingToTypeIdentifier(UTType.image.identifier) {
      attachment.loadItem(forTypeIdentifier: UTType.image.identifier, options: nil) {
        [weak self] data, _ in
        self?.saveImage(data)
      }
    } else if attachment.hasItemConformingToTypeIdentifier(UTType.plainText.identifier) {
      attachment.loadItem(forTypeIdentifier: UTType.plainText.identifier, options: nil) {
        [weak self] data, _ in
        self?.saveText(data as? String)
      }
    } else if attachment.hasItemConformingToTypeIdentifier(UTType.url.identifier) {
      attachment.loadItem(forTypeIdentifier: UTType.url.identifier, options: nil) {
        [weak self] data, _ in
        self?.saveText((data as? URL)?.absoluteString)
      }
    } else {
      finish()
    }
  }

  private func container() -> URL? {
    FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: appGroupId)
  }

  private func saveText(_ text: String?) {
    guard let text, !text.isEmpty, let dir = container() else { return finish() }
    writeManifest(["type": "text", "text": text], dir: dir)
    openHostAndFinish()
  }

  private func saveImage(_ data: NSSecureCoding?) {
    guard let dir = container() else { return finish() }
    let imageURL = dir.appendingPathComponent("share.img")
    let bytes: Data?
    switch data {
    case let url as URL: bytes = try? Data(contentsOf: url)
    case let d as Data: bytes = d
    case let image as UIImage: bytes = image.pngData()
    default: bytes = nil
    }
    guard let bytes else { return finish() }
    // The raw image bytes go to a file in the App Group; the app reads and
    // removes them (the app never opens a file of its own). The manifest just
    // marks the pending item's kind.
    try? bytes.write(to: imageURL)
    writeManifest(["type": "image"], dir: dir)
    openHostAndFinish()
  }

  private func writeManifest(_ manifest: [String: Any], dir: URL) {
    let url = dir.appendingPathComponent("share.json")
    if let data = try? JSONSerialization.data(withJSONObject: manifest) {
      try? data.write(to: url)
    }
  }

  /// Opens the host app via the custom URL scheme, then closes the sheet. The
  /// responder-chain walk is the standard way a share extension opens its host.
  private func openHostAndFinish() {
    guard let url = URL(string: hostURL) else { return finish() }
    var responder: UIResponder? = self
    while let current = responder {
      if let application = current as? UIApplication {
        application.open(url, options: [:], completionHandler: nil)
        break
      }
      responder = current.next
    }
    finish()
  }

  private func finish() {
    extensionContext?.completeRequest(returningItems: [], completionHandler: nil)
  }
}
