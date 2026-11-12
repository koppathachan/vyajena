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
      guard call.method == "dataDir" else {
        result(FlutterMethodNotImplemented)
        return
      }
      do {
        result(try CoreDataDir.prepare().path)
      } catch {
        result(FlutterError(code: "dataDir", message: error.localizedDescription, details: nil))
      }
    }
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
