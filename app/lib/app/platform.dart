// The app's own platform calls, on the `vyajena/platform` channel that also
// hands the core its data folder (rust_core_service.dart).
//
// Sharing OUT: only texts the user chose to share go through `shareText` —
// invites, link requests and link approvals, never message text.
//
// Sharing IN (change `message-actions-share`): other apps share text or an
// image TO Vyajena. The native side holds the shared item and hands only the
// raw content to the app over this same channel — `takeInitialShare` pulls a
// share that launched the app, and the native side invokes `sharedContent` for
// a share that arrives while the app runs. The app does the encryption and
// sending itself through the core; nothing here touches keys or ciphertext.
import 'package:flutter/services.dart';

const _channel = MethodChannel('vyajena/platform');

/// Opens the system share sheet for [text] (iOS UIActivityViewController,
/// Android ACTION_SEND).
Future<void> shareText(String text) =>
    _channel.invokeMethod<void>('share', {'text': text});

/// Content another app shared to Vyajena (change `message-actions-share`): plain
/// text, or image bytes. The native side reads the shared image and hands its
/// raw bytes across — the app never touches a file (app-ui: UI goes through the
/// core); it passes the bytes straight to the core's image send path.
sealed class SharedContent {
  const SharedContent();

  /// Parses a native payload map, or null if it is not a share this app
  /// handles. Shape: `{type: 'text', text: ...}` or `{type: 'image', bytes: ...}`
  /// where `bytes` is the raw image bytes the native side read.
  static SharedContent? fromMap(Object? payload) {
    if (payload is! Map) return null;
    final text = payload['text'];
    final bytes = payload['bytes'];
    switch (payload['type']) {
      case 'text' when text is String && text.isNotEmpty:
        return SharedText(text);
      case 'image' when bytes is Uint8List && bytes.isNotEmpty:
        return SharedImage(bytes);
      default:
        return null;
    }
  }
}

/// Shared plain text.
class SharedText extends SharedContent {
  const SharedText(this.text);
  final String text;
}

/// A shared image as its raw [bytes] (the native side read them; the app never
/// opens a file).
class SharedImage extends SharedContent {
  const SharedImage(this.bytes);
  final Uint8List bytes;
}

/// The `vyajena/platform` channel's inbound-share surface. A thin wrapper so the
/// share intake (and its tests) need not touch the raw channel.
class ShareChannel {
  const ShareChannel();

  /// Pulls any share that launched the app (a cold-start share), clearing it
  /// natively; null when there is none. Returns null if the native side does
  /// not implement it (e.g. before the owner wires the iOS extension).
  Future<SharedContent?> takeInitialShare() async {
    try {
      final payload = await _channel.invokeMethod<Object?>('takeInitialShare');
      return SharedContent.fromMap(payload);
    } on MissingPluginException {
      return null;
    } on PlatformException {
      return null;
    }
  }

  /// Calls [onShared] when the native side pushes a share that arrived while the
  /// app runs (Android `onNewIntent`, iOS URL open). Pass null to stop.
  void onShared(void Function(SharedContent content)? onShared) {
    if (onShared == null) {
      _channel.setMethodCallHandler(null);
      return;
    }
    _channel.setMethodCallHandler((call) async {
      if (call.method == 'sharedContent') {
        final content = SharedContent.fromMap(call.arguments);
        if (content != null) onShared(content);
      }
      return null;
    });
  }
}
