// The app shell: theme, lifecycle and the connection indicator around the
// screens, which come with the feature folders (flutter-app design D1:
// onboarding, chat, members, profile, devices, admin; tasks 5.1-5.5).
import 'package:flutter/material.dart';

import '../core/core_service.dart';
import 'connection_indicator.dart';
import 'lifecycle.dart';
import 'theme.dart';

/// The app, over [core].
class VyajenaApp extends StatelessWidget {
  const VyajenaApp({super.key, required this.core});

  final CoreService core;

  @override
  Widget build(BuildContext context) => CoreLifecycle(
    core: core,
    child: MaterialApp(
      title: 'Vyajena',
      theme: lightTheme,
      darkTheme: darkTheme,
      themeMode: themeMode,
      home: HomeScreen(core: core),
    ),
  );
}

/// The first screen: what the core's state allows.
class HomeScreen extends StatelessWidget {
  const HomeScreen({super.key, required this.core});

  final CoreService core;

  @override
  Widget build(BuildContext context) => Scaffold(
    appBar: AppBar(
      title: const Text('Vyajena'),
      actions: [ConnectionIndicator(core: core)],
    ),
    body: ValueListenableBuilder<CoreState>(
      valueListenable: core.state,
      builder: (context, state, _) => switch (state) {
        CoreState.opening => const Center(child: CircularProgressIndicator()),
        CoreState.failed => Center(
          child: Padding(
            padding: const EdgeInsets.all(24),
            child: Text(
              'The app could not open its data: ${_describe(core.openError)}',
              textAlign: TextAlign.center,
            ),
          ),
        ),
        CoreState.unregistered => const Center(child: Text('Not joined yet')),
        CoreState.ready => ValueListenableBuilder<UserRef?>(
          valueListenable: core.user,
          builder: (context, user, _) => Center(
            child: Text(
              user == null ? '' : '${user.username}@${user.serverId}',
            ),
          ),
        ),
      },
    ),
  );

  static String _describe(Object? error) =>
      error is CoreError ? error.message : '$error';
}
