// The Dart entry point of an Android data wake when the app is not running
// (change `smarter-notifications`). The phone starts a headless Flutter engine
// here for the few seconds FCM allows: the core opens, connects and catches up,
// the notifications are built from its summary and posted, and the phone is told
// it may stop (app-ui: Type-aware notification wording; local-notifications:
// The device builds the notification locally). No screen is shown and nothing
// here reads message text.
import 'package:flutter/widgets.dart';

import '../core/rust_core_service.dart';
import '../src/rust/frb_generated.dart';
import 'notification_service.dart';
import 'push_platform.dart';

@pragma('vm:entry-point')
Future<void> notificationWake() async {
  WidgetsFlutterBinding.ensureInitialized();
  final api = MethodChannelPush();
  try {
    await RustLib.init();
    final core = RustCoreService();
    // Not in front: this engine has no screen. `open` starts the core.
    final service = NotificationService(
      core: core,
      api: api,
      isForeground: () => false,
    );
    await service.handleWake();
    await core.dispose();
  } on Object {
    // A wake that cannot finish shows nothing; the next app open catches up.
  } finally {
    await api.wakeDone();
  }
}
