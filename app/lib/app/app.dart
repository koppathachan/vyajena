// The app shell: theme, lifecycle, the named routes (flutter-app design D3)
// and the first screen, which follows the core's state: join or link when
// this device has no user yet, the group (create, wait or chat) once it has.
import 'package:flutter/material.dart';

import '../chat/chat_model.dart';
import '../chat/group_gate.dart';
import '../core/core_service.dart';
import '../onboarding/onboarding_screen.dart';
import 'errors.dart';
import 'lifecycle.dart';
import 'routes.dart';
import 'shell_app_bar.dart';
import 'theme.dart';

/// The app, over [core]. Pump it before opening the core: its chat model
/// follows the core's events from the start.
class VyajenaApp extends StatefulWidget {
  const VyajenaApp({super.key, required this.core});

  final CoreService core;

  @override
  State<VyajenaApp> createState() => _VyajenaAppState();
}

class _VyajenaAppState extends State<VyajenaApp> {
  late final ChatModel _chat = ChatModel(widget.core);

  @override
  void dispose() {
    _chat.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => CoreLifecycle(
    core: widget.core,
    child: MaterialApp(
      title: 'Vyajena',
      theme: lightTheme,
      darkTheme: darkTheme,
      themeMode: themeMode,
      home: HomeScreen(core: widget.core, chat: _chat),
      routes: appRoutes(widget.core),
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
