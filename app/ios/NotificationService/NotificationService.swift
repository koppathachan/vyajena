import UserNotifications

/// smarter-notifications (local-notifications: The device builds the
/// notification locally; message-delivery: Wake push payload is content-free and
/// metadata-minimal).
///
/// The server's push is a visible alert saying only "New message", with
/// `mutable-content`. iOS runs this extension (in its own process, even when the
/// app is force-quit) before it shows the alert, so the words can be made
/// better: the app saved, in the shared App Group, what it would say for what it
/// last knew to be unread ("2 new DMs, 1 new in Work"), already worded and
/// leveled by the app's "Notification detail" setting. This only copies that
/// line in.
///
/// It decrypts nothing, opens no connection and touches no MLS state, and the
/// saved line holds a type and a count only: no sender, no message text. If the
/// saved line is missing, or this cannot finish in time, the generic "New
/// message" stays.
class NotificationService: UNNotificationServiceExtension {
  /// The App Group both the app and this extension belong to.
  static let group = "group.com.purplechaamba.vyajena"
  /// What the app saved: `{ "combined": String, "singleChatId": Int? }`.
  static let snapshotKey = "wakeSnapshot"

  var contentHandler: ((UNNotificationContent) -> Void)?
  var bestAttempt: UNMutableNotificationContent?

  override func didReceive(
    _ request: UNNotificationRequest,
    withContentHandler contentHandler: @escaping (UNNotificationContent) -> Void
  ) {
    self.contentHandler = contentHandler
    bestAttempt = (request.content.mutableCopy() as? UNMutableNotificationContent)
    guard let content = bestAttempt else {
      contentHandler(request.content)
      return
    }
    if let saved = Self.saved(), !saved.combined.isEmpty {
      content.body = saved.combined
      if let chat = saved.singleChatId {
        // Alerts of one conversation stack together; a tap opens it.
        content.threadIdentifier = "chat-\(chat)"
        content.userInfo["chatId"] = chat
      }
    }
    contentHandler(content)
  }

  /// Out of time: the alert as it is (the generic text, or what was copied).
  override func serviceExtensionTimeWillExpire() {
    if let contentHandler, let bestAttempt { contentHandler(bestAttempt) }
  }

  private struct Saved {
    let combined: String
    let singleChatId: Int?
  }

  private static func saved() -> Saved? {
    guard
      let defaults = UserDefaults(suiteName: group),
      let text = defaults.string(forKey: snapshotKey),
      let data = text.data(using: .utf8),
      let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
      let combined = object["combined"] as? String
    else { return nil }
    return Saved(combined: combined, singleChatId: (object["singleChatId"] as? NSNumber)?.intValue)
  }
}
