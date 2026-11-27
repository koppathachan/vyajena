import AVFoundation
import Flutter
import UIKit
import UserNotifications

/// video-messages: Video normalization in a native plugin (design D3; spike S1).
/// The Rust core cannot transcode H.264 on a phone, so a gallery clip (or a
/// recording) is normalized here with AVFoundation to the 480p/~1 Mbps H.264/MP4
/// target, trimmed to the 30-second cap, and its first frame returned as RGBA for
/// the core's ThumbHash. The normalized file goes to the app's temp; Dart reads
/// its bytes, hands them to the core to encrypt, and deletes it. Nothing here
/// touches keys or ciphertext.
enum VideoNormalize {
  static let channelName = "chat.vyajena/video"

  static func handle(_ call: FlutterMethodCall, _ result: @escaping FlutterResult) {
    switch call.method {
    case "normalize":
      guard let args = call.arguments as? [String: Any],
        let path = args["path"] as? String
      else {
        result(FlutterError(code: "normalize", message: "no path", details: nil))
        return
      }
      let maxDurationMs = (args["maxDurationMs"] as? NSNumber)?.int64Value ?? 30_000
      normalize(path: path, maxDurationMs: maxDurationMs, result: result)
    default:
      result(FlutterMethodNotImplemented)
    }
  }

  private static func normalize(
    path: String, maxDurationMs: Int64, result: @escaping FlutterResult
  ) {
    let asset = AVURLAsset(url: URL(fileURLWithPath: path))
    // Trim to the 30-second cap from the start (the slider UI chooses the window
    // before this; absent one, the first 30 s).
    let cap = CMTime(value: maxDurationMs, timescale: 1000)
    let duration = asset.duration
    let end = CMTimeMinimum(duration, cap)
    let range = CMTimeRange(start: .zero, end: end)

    // 640x480 H.264 ≈ the 480p target; the exporter writes an MP4 (H.264 + AAC).
    guard
      let export = AVAssetExportSession(
        asset: asset, presetName: AVAssetExportPreset640x480)
    else {
      result(FlutterError(code: "normalize", message: "no exporter", details: nil))
      return
    }
    let outURL = URL(fileURLWithPath: NSTemporaryDirectory())
      .appendingPathComponent("vyajena-video-\(UUID().uuidString).mp4")
    export.outputURL = outURL
    export.outputFileType = .mp4
    export.timeRange = range
    export.shouldOptimizeForNetworkUse = true
    export.exportAsynchronously {
      if export.status != .completed {
        let message = export.error?.localizedDescription ?? "export failed"
        DispatchQueue.main.async {
          result(FlutterError(code: "normalize", message: message, details: nil))
        }
        return
      }
      let out = AVURLAsset(url: outURL)
      let track = out.tracks(withMediaType: .video).first
      let size = track?.naturalSize.applying(track!.preferredTransform) ?? .zero
      let width = Int(abs(size.width))
      let height = Int(abs(size.height))
      let durationMs = Int(CMTimeGetSeconds(out.duration) * 1000)
      let frame = firstFrame(out)
      DispatchQueue.main.async {
        result([
          "path": outURL.path,
          "durationMs": durationMs,
          "width": width,
          "height": height,
          "frameRgba": frame.map { FlutterStandardTypedData(bytes: $0.rgba) } as Any,
          "frameWidth": frame?.width ?? 0,
          "frameHeight": frame?.height ?? 0,
        ])
      }
    }
  }

  /// The clip's first frame as RGBA, scaled so its long edge is at most 160 px
  /// (the core scales it down to ~100 for the ThumbHash anyway), to keep the
  /// bytes that cross the channel small.
  private static func firstFrame(_ asset: AVAsset)
    -> (rgba: Data, width: Int, height: Int)?
  {
    let generator = AVAssetImageGenerator(asset: asset)
    generator.appliesPreferredTrackTransform = true
    generator.maximumSize = CGSize(width: 160, height: 160)
    guard let cg = try? generator.copyCGImage(at: .zero, actualTime: nil) else {
      return nil
    }
    let width = cg.width
    let height = cg.height
    var rgba = Data(count: width * height * 4)
    let colorSpace = CGColorSpaceCreateDeviceRGB()
    let ok = rgba.withUnsafeMutableBytes { raw -> Bool in
      guard
        let context = CGContext(
          data: raw.baseAddress, width: width, height: height, bitsPerComponent: 8,
          bytesPerRow: width * 4, space: colorSpace,
          bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
      else { return false }
      context.draw(cg, in: CGRect(x: 0, y: 0, width: width, height: height))
      return true
    }
    return ok ? (rgba, width, height) : nil
  }
}

/// Background time for finishing a send (app-ui: Finish sends in the
/// background; change finish-sends-in-background D3). `begin` asks iOS for a
/// short window after the app is paused (about 30 seconds); `end` gives it back;
/// when iOS ends it first, Dart is told with `expired` and stops the core.
enum BackgroundTask {
  static let channelName = "vyajena/background"
  static var channel: FlutterMethodChannel?
  private static var task: UIBackgroundTaskIdentifier = .invalid

  static func handle(_ call: FlutterMethodCall, _ result: @escaping FlutterResult) {
    switch call.method {
    case "begin":
      if task == .invalid {
        task = UIApplication.shared.beginBackgroundTask(withName: "finish-sends") {
          channel?.invokeMethod("expired", arguments: nil)
          end()
        }
      }
      result(nil)
    case "end":
      end()
      result(nil)
    default:
      result(FlutterMethodNotImplemented)
    }
  }

  static func end() {
    if task != .invalid {
      UIApplication.shared.endBackgroundTask(task)
      task = .invalid
    }
  }
}

@main
@objc class AppDelegate: FlutterAppDelegate, FlutterImplicitEngineDelegate {
  override func application(
    _ application: UIApplication,
    didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
  ) -> Bool {
    let started = super.application(application, didFinishLaunchingWithOptions: launchOptions)
    // Taps on a notification come here (app-ui: A notification opens its
    // conversation); with no delegate iOS shows nothing while the app is in front.
    UNUserNotificationCenter.current().delegate = self
    return started
  }

  // The app is in front and connected: a notification would only repeat the screen.
  override func userNotificationCenter(
    _ center: UNUserNotificationCenter,
    willPresent notification: UNNotification,
    withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
  ) {
    completionHandler([])
  }

  // A tap on a notification: open the conversation it is for.
  override func userNotificationCenter(
    _ center: UNUserNotificationCenter,
    didReceive response: UNNotificationResponse,
    withCompletionHandler completionHandler: @escaping () -> Void
  ) {
    Push.tapped(response.notification.request.content.userInfo)
    completionHandler()
  }

  // APNs hands over the device token (app-ui: Push notifications).
  override func application(
    _ application: UIApplication,
    didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data
  ) {
    Push.registered(deviceToken)
    super.application(application, didRegisterForRemoteNotificationsWithDeviceToken: deviceToken)
  }

  override func application(
    _ application: UIApplication,
    didFailToRegisterForRemoteNotificationsWithError error: Error
  ) {
    Push.failed()
    super.application(application, didFailToRegisterForRemoteNotificationsWithError: error)
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
      case "takeInitialShare":
        // Drain any item the share extension left in the App Group. Called by
        // Dart at startup and on resume; the extension foregrounds the app, so
        // the next active cycle picks the item up (change `message-actions-share`).
        result(SharedInbox.drain())
      default:
        result(FlutterMethodNotImplemented)
      }
    }
    let push = FlutterMethodChannel(
      name: Push.channelName,
      binaryMessenger: engineBridge.applicationRegistrar.messenger())
    Push.channel = push
    push.setMethodCallHandler(Push.handle)
    let background = FlutterMethodChannel(
      name: BackgroundTask.channelName,
      binaryMessenger: engineBridge.applicationRegistrar.messenger())
    BackgroundTask.channel = background
    background.setMethodCallHandler(BackgroundTask.handle)
    let sticker = FlutterMethodChannel(
      name: Sticker.channelName,
      binaryMessenger: engineBridge.applicationRegistrar.messenger())
    sticker.setMethodCallHandler(Sticker.handle)
    let video = FlutterMethodChannel(
      name: VideoNormalize.channelName,
      binaryMessenger: engineBridge.applicationRegistrar.messenger())
    video.setMethodCallHandler(VideoNormalize.handle)
  }
}

/// APNs for the app (app-ui: Push notifications; push design D8, D9): no
/// Firebase. The server sends a visible alert with the fixed text "New message"
/// and `mutable-content`; iOS shows it, after the Notification Service Extension
/// (smarter-notifications) has had the chance to put the app's saved
/// count-and-type line in its place. A tap opens the conversation it is for.
/// While the app is in front no alert is shown: it is connected.
enum Push {
  /// The App Group the app and its notification extension share, and the key of
  /// the one line the app saves for the extension (see NotificationService).
  static let group = "group.com.purplechaamba.vyajena"
  static let snapshotKey = "wakeSnapshot"
  /// The tap that launched the app, until Dart asks; after that taps go straight
  /// to Dart.
  private static var pendingTap: [String: Any]?
  private static var dartListening = false

  static let channelName = "vyajena/push"
  static var channel: FlutterMethodChannel?
  /// The APNs token as hex, once iOS gave one this launch.
  private static var token: String?
  private static var waiting: [FlutterResult] = []

  static func handle(_ call: FlutterMethodCall, _ result: @escaping FlutterResult) {
    switch call.method {
    case "ensurePermission":
      ensurePermission(result)
    case "token":
      requestToken(result)
    case "deleteToken":
      UIApplication.shared.unregisterForRemoteNotifications()
      token = nil
      result(nil)
    case "clearNotifications":
      UNUserNotificationCenter.current().removeAllDeliveredNotifications()
      result(nil)
    case "clearConversation":
      clearConversation(call.arguments as? Int)
      result(nil)
    case "saveSnapshot":
      saveSnapshot(call.arguments as? [String: Any])
      result(nil)
    case "takeLaunchTap":
      dartListening = true
      let tap = pendingTap
      pendingTap = nil
      result(tap)
    // iOS shows the server's alert itself, and the extension rewrites it.
    case "showNotifications", "wakeDone":
      result(nil)
    default:
      result(FlutterMethodNotImplemented)
    }
  }

  /// iOS itself asks only once per install; later calls report the setting.
  private static func ensurePermission(_ result: @escaping FlutterResult) {
    let center = UNUserNotificationCenter.current()
    center.getNotificationSettings { settings in
      switch settings.authorizationStatus {
      case .notDetermined:
        center.requestAuthorization(options: [.alert, .sound, .badge]) { granted, _ in
          DispatchQueue.main.async { result(granted) }
        }
      case .authorized, .provisional, .ephemeral:
        DispatchQueue.main.async { result(true) }
      default:
        DispatchQueue.main.async { result(false) }
      }
    }
  }

  private static func requestToken(_ result: @escaping FlutterResult) {
    if let token {
      result(token)
      return
    }
    waiting.append(result)
    UIApplication.shared.registerForRemoteNotifications()
    // No answer (no network, simulator without APNs): try again next connect.
    DispatchQueue.main.asyncAfter(deadline: .now() + 15) { answer(nil) }
  }

  static func registered(_ deviceToken: Data) {
    let hex = deviceToken.map { String(format: "%02x", $0) }.joined()
    let rotated = token != nil && token != hex
    token = hex
    answer(hex)
    if rotated { channel?.invokeMethod("tokenChanged", arguments: hex) }
  }

  static func failed() { answer(nil) }

  /// app-ui: A notification opens its conversation. The extension put the
  /// conversation's id in `userInfo` when exactly one was involved.
  static func tapped(_ userInfo: [AnyHashable: Any]) {
    let chat = (userInfo["chatId"] as? NSNumber)?.intValue
    if dartListening {
      channel?.invokeMethod("notificationTapped", arguments: chat)
    } else {
      pendingTap = ["chatId": chat as Any? ?? NSNull()]
    }
  }

  /// Removes the delivered alerts of one conversation (the extension groups them
  /// by thread).
  private static func clearConversation(_ chat: Int?) {
    guard let chat else { return }
    let center = UNUserNotificationCenter.current()
    center.getDeliveredNotifications { delivered in
      let ids = delivered
        .filter { $0.request.content.threadIdentifier == "chat-\(chat)" }
        .map { $0.request.identifier }
      center.removeDeliveredNotifications(withIdentifiers: ids)
    }
  }

  /// Saves, for the Notification Service Extension, the one line it may show
  /// (a type and a count, already worded at the user's chosen detail level; no
  /// sender, no text), and sets the app-icon badge.
  private static func saveSnapshot(_ arguments: [String: Any]?) {
    let combined = arguments?["combined"] as? String ?? ""
    let badge = arguments?["badge"] as? Int ?? 0
    if let defaults = UserDefaults(suiteName: group) {
      if combined.isEmpty {
        defaults.removeObject(forKey: snapshotKey)
      } else {
        var line: [String: Any] = ["combined": combined]
        if let chat = arguments?["singleChatId"] as? Int { line["singleChatId"] = chat }
        if let data = try? JSONSerialization.data(withJSONObject: line),
          let text = String(data: data, encoding: .utf8)
        {
          defaults.set(text, forKey: snapshotKey)
        }
      }
    }
    DispatchQueue.main.async {
      if #available(iOS 16.0, *) {
        UNUserNotificationCenter.current().setBadgeCount(badge)
      } else {
        UIApplication.shared.applicationIconBadgeNumber = badge
      }
    }
  }

  private static func answer(_ value: String?) {
    let results = waiting
    waiting = []
    for result in results { result(value) }
  }
}

/// Inbound share from the Share Extension (change `message-actions-share`). The
/// extension writes the raw shared item (text, or an image file) into the App
/// Group container as `share.json`; the app drains it when it becomes active and
/// hands the payload to Dart, which opens the chat picker and sends it through
/// the core. Nothing here touches keys or ciphertext.
///
/// The App Group id must match the extension's and be registered on both targets
/// in the Apple Developer portal (ios/ShareExtension/README.md).
enum SharedInbox {
  static let appGroup = "group.com.purplechaamba.vyajena"

  private static func container() -> URL? {
    FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: appGroup)
  }

  /// Reads and removes a pending shared item, returning the Dart payload map
  /// (`{type: "text", text: ...}` or `{type: "image", bytes: <Data>}`), or nil
  /// if there is none. For an image, the extension wrote the raw bytes to
  /// `share.img` in the App Group; this reads them and hands them to Dart, which
  /// passes them straight to the core — the app opens no file of its own.
  static func drain() -> [String: Any]? {
    let files = FileManager.default
    guard let dir = container() else { return nil }
    let manifest = dir.appendingPathComponent("share.json")
    guard let data = try? Data(contentsOf: manifest),
      let payload = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
      let type = payload["type"] as? String
    else {
      return nil
    }
    try? files.removeItem(at: manifest)
    switch type {
    case "text":
      return payload
    case "image":
      let image = dir.appendingPathComponent("share.img")
      guard let bytes = try? Data(contentsOf: image) else { return nil }
      try? files.removeItem(at: image)
      return ["type": "image", "bytes": FlutterStandardTypedData(bytes: bytes)]
    default:
      return nil
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
