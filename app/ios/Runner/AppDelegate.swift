import Flutter
import UIKit

@main
@objc class AppDelegate: FlutterAppDelegate, FlutterImplicitEngineDelegate {
  override func application(
    _ application: UIApplication,
    didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
  ) -> Bool {
    return super.application(application, didFinishLaunchingWithOptions: launchOptions)
  }

  func didInitializeImplicitFlutterEngine(_ engineBridge: FlutterImplicitEngineBridge) {
    GeneratedPluginRegistrant.register(with: engineBridge.pluginRegistry)
    let channel = FlutterMethodChannel(
      name: CoreDataDir.channel,
      binaryMessenger: engineBridge.applicationRegistrar.messenger())
    channel.setMethodCallHandler { call, result in
      switch call.method {
      case "dataDir":
        do {
          result(try CoreDataDir.prepare().path)
        } catch {
          result(FlutterError(code: "dataDir", message: error.localizedDescription, details: nil))
        }
      case "share":
        guard let args = call.arguments as? [String: Any], let text = args["text"] as? String
        else {
          result(FlutterError(code: "share", message: "no text", details: nil))
          return
        }
        ShareSheet.present(text)
        result(nil)
      default:
        result(FlutterMethodNotImplemented)
      }
    }
  }
}

/// The system share sheet for an invite, link request or link approval the
/// user chose to share (flutter-app design D5). Never message text.
enum ShareSheet {
  static func present(_ text: String) {
    let root = UIApplication.shared.connectedScenes
      .compactMap { ($0 as? UIWindowScene)?.keyWindow }
      .first?.rootViewController
    guard let root else { return }
    var top = root
    while let presented = top.presentedViewController { top = presented }
    let sheet = UIActivityViewController(activityItems: [text], applicationActivities: nil)
    // iPad shows the sheet as a popover, which needs an anchor.
    sheet.popoverPresentationController?.sourceView = top.view
    sheet.popoverPresentationController?.sourceRect = CGRect(
      x: top.view.bounds.midX, y: top.view.bounds.midY, width: 0, height: 0)
    top.present(sheet, animated: true)
  }
}

/// The folder the Rust core keeps its database in (app-ui: Data on the
/// phone): Application Support/core, private to the app, excluded from
/// iCloud backup, and NSFileProtectionComplete for the folder and every
/// file in it (files the core creates later inherit the folder's class).
/// Keys are not here: the core keeps them in the iOS Keychain.
enum CoreDataDir {
  static let channel = "vyajena/platform"

  static func prepare() throws -> URL {
    let files = FileManager.default
    var dir = try files.url(
      for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true
    ).appendingPathComponent("core", isDirectory: true)
    try files.createDirectory(
      at: dir, withIntermediateDirectories: true,
      attributes: [.protectionKey: FileProtectionType.complete])
    var values = URLResourceValues()
    values.isExcludedFromBackup = true
    try dir.setResourceValues(values)
    try files.setAttributes([.protectionKey: FileProtectionType.complete], ofItemAtPath: dir.path)
    for name in try files.contentsOfDirectory(atPath: dir.path) {
      try files.setAttributes(
        [.protectionKey: FileProtectionType.complete],
        ofItemAtPath: dir.appendingPathComponent(name).path)
    }
    return dir
  }
}
