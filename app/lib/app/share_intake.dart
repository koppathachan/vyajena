// app-ui: Vyajena is a share target (change `message-actions-share`). When
// another app shares text or an image to Vyajena, the native side hands the raw
// content here; this opens the one-chat picker and composes the content in the
// chosen chat through the normal send path. All encryption and sending happen in
// the core — nothing here holds a key or ciphertext.
//
// It lives at the app root (app.dart), not a chat screen, because a share can
// arrive while no chat is on screen (cold start, or the chat list). It drives the
// UI through the app's navigator and messenger keys.
import 'dart:async';

import 'package:flutter/material.dart';

import '../chat/forward_picker.dart';
import '../core/core_service.dart';
import 'platform.dart';

/// Shown when content is shared before a user is registered on the device.
const shareBeforeOnboardingText = 'Finish setting up Vyajena to share here';

/// Receives inbound shares and routes them to the chat picker. Start it at the
/// app root once the core exists; stop it on dispose.
///
/// Two paths bring a share in: the native side pushes `sharedContent` for a
/// share that arrives while the app runs (Android `onNewIntent`), and the app
/// pulls any pending share with `takeInitialShare` — at startup and again on
/// each resume, which covers iOS, where the share extension foregrounds the app
/// and the app drains the App Group inbox when it becomes active.
class ShareIntake with WidgetsBindingObserver {
  ShareIntake({
    required this.core,
    required this.navigatorKey,
    required this.messengerKey,
    this.channel = const ShareChannel(),
  });

  final CoreService core;
  final GlobalKey<NavigatorState> navigatorKey;
  final GlobalKey<ScaffoldMessengerState> messengerKey;
  final ShareChannel channel;

  /// One share at a time: a second arriving while the picker is open is dropped,
  /// so two pickers never stack.
  bool _busy = false;

  /// Listens for runtime shares and pulls any share that launched the app.
  void start() {
    channel.onShared(deliver);
    WidgetsBinding.instance.addObserver(this);
    unawaited(_takeInitial());
  }

  /// Stops listening.
  void stop() {
    WidgetsBinding.instance.removeObserver(this);
    channel.onShared(null);
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed) unawaited(_takeInitial());
  }

  Future<void> _takeInitial() async {
    final content = await channel.takeInitialShare();
    if (content != null) await deliver(content);
  }

  /// Handles one shared item: ensure a user is registered, pick a chat, send.
  /// Public so a widget test can feed a payload without the platform channel.
  Future<void> deliver(SharedContent content) async {
    if (_busy) return;
    _busy = true;
    try {
      final messenger = messengerKey.currentState;
      if (core.state.value != CoreState.ready) {
        messenger?.showSnackBar(
          const SnackBar(content: Text(shareBeforeOnboardingText)),
        );
        return;
      }
      final context = navigatorKey.currentContext;
      if (context == null) return;
      final target = await ForwardPicker.pick(
        context,
        core,
        title: shareToTitle,
      );
      if (target == null) return;
      try {
        await _send(content, target.chatId);
        messenger?.showSnackBar(
          SnackBar(content: Text('Sent to ${target.title}')),
        );
      } catch (error) {
        messenger?.showSnackBar(SnackBar(content: Text('$error')));
      }
    } finally {
      _busy = false;
    }
  }

  /// Sends [content] into chat [chatId] through the normal send path: text as a
  /// text message, an image as a new attachment (its designed entry for picked
  /// bytes — no re-encode concern, it is a fresh image from another app). The
  /// image arrives as raw bytes the native side read, so nothing here opens a
  /// file (app-ui: UI goes through the core).
  Future<void> _send(SharedContent content, int chatId) async {
    switch (content) {
      case SharedText(:final text):
        if (chatId == 0) {
          await core.sendText(text);
        } else {
          await core.sendMessageIn(chatId, text);
        }
      case SharedImage(:final bytes):
        await core.sendImage(bytes, chatId: chatId);
    }
  }
}
