// The app bar every main screen shares: a title, the user this device
// belongs to, the connection indicator, and the screen's own actions.
import 'package:flutter/material.dart';

import '../core/core_service.dart';
import 'connection_indicator.dart';

/// An app bar with [title], this device's user under it, the connection
/// indicator and [actions].
AppBar shellAppBar(
  CoreService core, {
  String title = 'Vyajena',
  List<Widget> actions = const [],
}) => AppBar(
  title: ValueListenableBuilder<UserRef?>(
    valueListenable: core.user,
    builder: (context, user, _) => Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(title),
        if (user != null)
          Text(userText(user), style: Theme.of(context).textTheme.bodySmall),
      ],
    ),
  ),
  actions: [
    ConnectionIndicator(core: core),
    ...actions,
  ],
);

/// `user@server`.
String userText(UserRef user) => '${user.username}@${user.serverId}';
