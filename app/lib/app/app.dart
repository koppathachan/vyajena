// The app shell: theme, lifecycle, the named routes (flutter-app design D3)
// and the first screen, which follows the core's state: join or link when
// this device has no user yet, the group (create, wait or chat) once it has.
import 'dart:async';

import 'package:flutter/material.dart';

import '../chat/chat_model.dart';
import '../chat/group_gate.dart';
import '../chat/picture_picker.dart';
import '../chat/sticker_plugins.dart';
import '../chat/video_plugins.dart';
import '../chat/voice_plugins.dart';
import '../core/core_service.dart';
import '../onboarding/onboarding_screen.dart';
import '../push/push_platform.dart';
import 'background_task.dart';
import '../push/notification_service.dart';
import '../push/open_conversation.dart';
import '../push/push_service.dart';
import 'errors.dart';
import 'lifecycle.dart';
import 'routes.dart';
import 'share_intake.dart';
import 'shell_app_bar.dart';
import 'arcade/tokens.dart';
import 'theme.dart';
import 'theme_mode.dart';

/// The app, over [core]. Pump it before opening the core: its chat model
/// follows the core's events from the start.
class VyajenaApp extends StatefulWidget {
  const VyajenaApp({
    super.key,
    required this.core,
    this.networkChanges,
    this.push,
    this.background,
    this.pictures = const GalleryPicker(),
    this.voice = const DeviceVoicePlugins(),
    this.segmenter = const DeviceSegmenter(),
    this.video = const DeviceVideoSource(),
    this.themeStore = const DeviceThemeModeStore(),
  });

  final CoreService core;

  /// Where the Auto/Light/Dark choice is kept (app-ui: Arcade look); the
  /// phone's store, or a memory one in widget tests.
  final ThemeModeStore themeStore;

  /// Where a picture to send comes from (app-ui: Send an image); the
  /// gallery, or a fake in widget tests.
  final PicturePicker pictures;

  /// The microphone and speaker for voice messages (app-ui: Record a voice
  /// message; Play a voice message); the phone's, or fakes in widget tests.
  final VoicePlugins voice;

  /// The platform's on-device subject segmentation for stickers (app-ui:
  /// Create a sticker); the phone's, or a fake in widget tests.
  final Segmenter segmenter;

  /// Where a video to send comes from and how it is normalized (app-ui: Send a
  /// video); the phone's camera/gallery and native plugin, or a fake in tests.
  final VideoSource video;

  /// The phone's network changes (see [CoreLifecycle.networkChanges]).
  final Stream<bool>? networkChanges;

  /// The phone's push service (app-ui: Push notifications); none in most
  /// widget tests.
  final PushPlatformApi? push;

  /// The phone's short background time for finishing a send (app-ui: Finish
  /// sends in the background); none in most widget tests.
  final BackgroundTaskApi? background;

  @override
  State<VyajenaApp> createState() => _VyajenaAppState();
}

class _VyajenaAppState extends State<VyajenaApp> {
  late final ChatModel _chat = ChatModel(widget.core);
  late final ThemeModeController _theme = ThemeModeController(
    widget.themeStore,
  );
  PushService? _push;
  NotificationService? _notifications;

  /// The app's navigator and messenger, so an inbound share (which can arrive
  /// while no chat is on screen) can open the picker and show a snackbar from
  /// the app root (change `message-actions-share`).
  final _navigatorKey = GlobalKey<NavigatorState>();
  final _messengerKey = GlobalKey<ScaffoldMessengerState>();
  late final ShareIntake _shareIntake = ShareIntake(
    core: widget.core,
    navigatorKey: _navigatorKey,
    messengerKey: _messengerKey,
  );

  @override
  void initState() {
    super.initState();
    unawaited(_theme.restore());
    final api = widget.push;
    if (api != null) {
      _push = PushService(core: widget.core, api: api)..start();
      _notifications = NotificationService(
        core: widget.core,
        api: api,
        onOpen: _open,
      )..start();
    }
    _shareIntake.start();
  }

  /// A tapped notification: once the core is open, the conversation it names.
  Future<void> _open(NotificationTap tap) async {
    final core = widget.core;
    if (core.state.value == CoreState.opening) {
      final ready = Completer<void>();
      void listener() {
        if (core.state.value != CoreState.opening && !ready.isCompleted) {
          ready.complete();
        }
      }

      core.state.addListener(listener);
      await ready.future;
      core.state.removeListener(listener);
    }
    final navigator = _navigatorKey.currentState;
    if (core.state.value != CoreState.ready || navigator == null) return;
    await openTappedConversation(
      navigator: navigator,
      core: core,
      chat: _chat,
      tap: tap,
    );
  }

  @override
  void dispose() {
    _notifications?.dispose();
    _shareIntake.stop();
    _push?.dispose();
    _chat.dispose();
    _theme.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => CoreLifecycle(
    core: widget.core,
    networkChanges: widget.networkChanges,
    push: widget.push,
    background: widget.background,
    child: PicturePickerScope(
      picker: widget.pictures,
      child: VoiceScope(
        plugins: widget.voice,
        child: SegmenterScope(
          segmenter: widget.segmenter,
          child: VideoScope(
            source: widget.video,
            child: ThemeModeScope(
              controller: _theme,
              // app-ui: Theme choice. The theme gives both palettes; the
              // Auto/Light/Dark choice picks between them.
              child: ListenableBuilder(
                listenable: Listenable.merge([_theme, _theme.theme]),
                builder: (context, _) => MaterialApp(
                  title: 'Vyajena',
                  navigatorKey: _navigatorKey,
                  scaffoldMessengerKey: _messengerKey,
                  // Never the debug banner, also in profile builds and screenshots
                  // (app-release design D7).
                  debugShowCheckedModeBanner: false,
                  theme: themeFor(_theme.theme.value, Brightness.light),
                  darkTheme: themeFor(_theme.theme.value, Brightness.dark),
                  themeMode: _theme.value,
                  // The text scale is capped (app-ui: Arcade motion).
                  builder: (context, child) => MediaQuery(
                    data: MediaQuery.of(context).copyWith(
                      textScaler: MediaQuery.textScalerOf(
                        context,
                      ).clamp(maxScaleFactor: maxTextScale),
                    ),
                    child: child!,
                  ),
                  home: HomeScreen(core: widget.core, chat: _chat),
                  routes: appRoutes(widget.core),
                ),
              ),
            ),
          ),
        ),
      ),
    ),
  );
}

/// The first screen: what the core's state allows.
class HomeScreen extends StatelessWidget {
  const HomeScreen({super.key, required this.core, required this.chat});

  final CoreService core;

  /// The app-level chat model.
  final ChatModel chat;

  @override
  Widget build(BuildContext context) => ValueListenableBuilder<CoreState>(
    valueListenable: core.state,
    builder: (context, state, _) => switch (state) {
      CoreState.opening => Scaffold(
        appBar: shellAppBar(core),
        body: const Center(child: CircularProgressIndicator()),
      ),
      CoreState.failed => Scaffold(
        appBar: shellAppBar(core),
        body: Center(
          child: Padding(
            padding: const EdgeInsets.all(24),
            child: Text(
              'The app could not open its data: '
              '${core.openError == null ? '' : errorText(core.openError!)}',
              textAlign: TextAlign.center,
            ),
          ),
        ),
      ),
      CoreState.unregistered => OnboardingScreen(core: core),
      CoreState.ready => GroupGate(core: core, chat: chat),
    },
  );
}
