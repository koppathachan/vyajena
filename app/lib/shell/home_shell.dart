// app-shell: Bottom navigation. Three destinations over one IndexedStack, so
// each tab keeps its state and scroll while another is shown: Chats, Contacts
// and Profile, whose icon is the user's own round avatar (not a gear). A
// conversation opens on top of the shell (the bar hides, like Telegram's), so
// switching tabs never discards a half-typed message.
import 'dart:async';

import 'package:flutter/material.dart';

import '../app/arcade/arcade_border.dart';

import '../chat/chat_list_screen.dart';
import '../chat/chat_model.dart';
import '../core/core_service.dart';
import '../members/contacts_tab.dart';
import '../profile/profile_screen.dart';
import 'avatars.dart';

/// The tabs, in bar order.
enum ShellTab { chats, contacts, profile }

/// The app's home once this device is in the group.
class HomeShell extends StatefulWidget {
  const HomeShell({
    super.key,
    required this.core,
    required this.chat,
    required this.admin,
  });

  final CoreService core;

  /// The app-level chat model (the lobby's).
  final ChatModel chat;

  /// Whether this device's user is the admin.
  final bool admin;

  @override
  State<HomeShell> createState() => _HomeShellState();
}

class _HomeShellState extends State<HomeShell> {
  ShellTab _tab = ShellTab.chats;
  String _ownName = '';
  StreamSubscription<CoreEvent>? _subscription;

  @override
  void initState() {
    super.initState();
    _subscription = widget.core.events.listen((event) {
      if (event is CoreEvent_ProfileChanged) _loadName();
    });
    _loadName();
  }

  @override
  void dispose() {
    _subscription?.cancel();
    super.dispose();
  }

  Future<void> _loadName() async {
    final me = widget.core.user.value;
    if (me == null) return;
    final profile = await widget.core.profile(me);
    final display = profile.displayName?.trim();
    if (mounted) {
      setState(
        () => _ownName = display == null || display.isEmpty
            ? me.username
            : display,
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    final me = widget.core.user.value;
    final avatarName = _ownName.isEmpty ? (me?.username ?? '?') : _ownName;
    Widget avatar({required bool selected}) => Stack(
      clipBehavior: Clip.none,
      children: [
        DecoratedBox(
          decoration: sketchBox(
            border: selected
                ? Theme.of(context).colorScheme.primary
                : Colors.transparent,
            seed: 21,
          ),
          child: Padding(
            padding: const EdgeInsets.all(1),
            child: NameAvatar(
              key: const Key('profile-avatar'),
              name: avatarName,
              seed: me?.username,
              radius: 13,
            ),
          ),
        ),
        // This user's own MANUAL online/offline presence (manual-presence:
        // Toggle your status on the Profile screen): green when the user set
        // Online, the error colour when Offline. Driven by the user's chosen
        // flag, never by the connection. Only the Profile tab shows it; other
        // NameAvatars are unchanged.
        Positioned(
          right: -1,
          bottom: -1,
          child: ValueListenableBuilder<PresenceStatus>(
            valueListenable: widget.core.myPresence,
            builder: (context, status, _) {
              final scheme = Theme.of(context).colorScheme;
              final online = status == PresenceStatus.online;
              return Container(
                key: const Key('profile-presence-dot'),
                width: 11,
                height: 11,
                decoration: sketchBox(
                  fill: online ? presenceOnlineColor : scheme.error,
                  border: scheme.outline,
                  width: 1.5,
                  seed: 23,
                ),
              );
            },
          ),
        ),
      ],
    );
    return Scaffold(
      body: IndexedStack(
        index: _tab.index,
        children: [
          ChatListScreen(
            core: widget.core,
            chat: widget.chat,
            admin: widget.admin,
          ),
          ContactsTab(
            core: widget.core,
            admin: widget.admin,
            onOpenProfile: () => setState(() => _tab = ShellTab.profile),
          ),
          ProfileScreen(core: widget.core, embedded: true, admin: widget.admin),
        ],
      ),
      bottomNavigationBar: DecoratedBox(
        // Hand-drawn lines: the bar's top edge is drawn, not ruled (in front,
        // or the bar's own fill would hide it).
        position: DecorationPosition.foreground,
        decoration: ShapeDecoration(
          shape: SketchEdgeBorder(
            color: Theme.of(context).colorScheme.outline,
            top: true,
            seed: 9,
          ),
        ),
        child: NavigationBar(
          key: const Key('bottom-nav'),
          selectedIndex: _tab.index,
          onDestinationSelected: (i) =>
              setState(() => _tab = ShellTab.values[i]),
          destinations: [
            const NavigationDestination(
              key: Key('tab-chats'),
              icon: Icon(Icons.chat_bubble_outline),
              selectedIcon: Icon(Icons.chat_bubble),
              label: 'Chats',
            ),
            const NavigationDestination(
              key: Key('tab-contacts'),
              icon: Icon(Icons.people_outline),
              selectedIcon: Icon(Icons.people),
              label: 'Contacts',
            ),
            NavigationDestination(
              key: const Key('tab-profile'),
              icon: avatar(selected: false),
              selectedIcon: avatar(selected: true),
              label: 'Profile',
            ),
          ],
        ),
      ),
    );
  }
}
