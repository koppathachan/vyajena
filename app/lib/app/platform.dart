// The app's own platform calls, on the `vyajena/platform` channel that also
// hands the core its data folder (rust_core_service.dart). Only texts the
// user chose to share go through here: invites, link requests and link
// approvals, never message text.
import 'package:flutter/services.dart';

const _channel = MethodChannel('vyajena/platform');

/// Opens the system share sheet for [text] (iOS UIActivityViewController,
/// Android ACTION_SEND).
Future<void> shareText(String text) =>
    _channel.invokeMethod<void>('share', {'text': text});
