// The app shell: theme, lifecycle, the named routes (flutter-app design D3)
// and the first screen, which follows the core's state: join or link when
// this device has no user yet, the group (create, wait or chat) once it has.
import 'package:flutter/material.dart';

import '../chat/chat_model.dart';
import '../chat/group_gate.dart';
import '../chat/picture_picker.dart';
import '../core/core_service.dart';
import '../onboarding/onboarding_screen.dart';
import '../push/push_platform.dart';
import '../push/push_service.dart';
import 'errors.dart';
import 'lifecycle.dart';
import 'routes.dart';
import 'shell_app_bar.dart';
import 'theme.dart';

/// The app, over [core]. Pump it before opening the core: its chat model
/// follows the core's events from the start.
class VyajenaApp extends StatefulWidget {
  const VyajenaApp({
    super.key,
    required this.core,
    this.networkChanges,
    this.push,
    this.pictures = const GalleryPicker(),
  });

  final CoreService core;

  /// Where a picture to send comes from (app-ui: Send an image); the
  /// gallery, or a fake in widget tests.
  final PicturePicker pictures;

  /// The phone's network changes (see [CoreLifecycle.networkChanges]).
  final Stream<bool>? networkChanges;

  /// The phone's push service (app-ui: Push notifications); none in most
  /// widget tests.
  final PushPlatformApi? push;

  @override
  State<VyajenaApp> createState() => _VyajenaAppState();
}

class _VyajenaAppState extends State<VyajenaApp> {
  late final ChatModel _chat = ChatModel(widget.core);
  PushService? _push;

  @override
  void initState() {
    super.initState();
    final api = widget.push;
    if (api != null) {
      _push = PushService(core: widget.core, api: api)..start();
    }
  }

  @override
  void dispose() {
    _push?.dispose();
    _chat.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => CoreLifecycle(
    core: widget.core,
    networkChanges: widget.networkChanges,
    push: widget.push,
    child: PicturePickerScope(
      picker: widget.pictures,
      child: MaterialApp(
        title: 'Vyajena',
        // Never the debug banner, also in profile builds and screenshots
        // (app-release design D7).
        debugShowCheckedModeBanner: false,
        theme: lightTheme,
        darkTheme: darkTheme,
        themeMode: themeMode,
        home: HomeScreen(core: widget.core, chat: _chat),
        routes: appRoutes(widget.core),
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
