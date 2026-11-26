// app-shell: avatars. Nobody has a profile photo yet (a profile is a display
// name and an about text), so every avatar is a round monogram: the first
// letter of the name on a colour taken from the name. A photo can replace the
// monogram later without changing the callers.
import 'dart:async';

import 'package:flutter/material.dart';

import '../app/theme.dart';

import '../core/core_service.dart';

/// The green of a presence dot for a user who set their status Online
/// (manual-presence: Presence indicator on avatars). Offline uses the theme's
/// `error` colour instead; an unknown status shows no dot.
const presenceOnlineColor = Color(0xFF34C759);

/// The monogram letter of [name]: its first letter or digit, upper-cased;
/// "?" for an empty name.
String monogram(String name) {
  for (final rune in name.trim().runes) {
    final char = String.fromCharCode(rune);
    if (RegExp(r'[\p{L}\p{N}]', unicode: true).hasMatch(char)) {
      return char.toUpperCase();
    }
  }
  return '?';
}

/// A round avatar for [name]: a monogram on a colour derived from [seed]
/// (the name by default), so the same name always looks the same.
class NameAvatar extends StatelessWidget {
  const NameAvatar({
    super.key,
    required this.name,
    this.seed,
    this.radius = 22,
    this.icon,
    this.muted = false,
  });

  final String name;

  /// What the colour is derived from, when not [name].
  final String? seed;

  final double radius;

  /// An icon in place of the monogram (the lobby, a group).
  final IconData? icon;

  /// Greyed out (a closed chat).
  final bool muted;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final hue = ((seed ?? name).hashCode.abs() % 360).toDouble();
    final base = HSLColor.fromAHSL(1, hue, 0.35, 0.42).toColor();
    final background = muted ? scheme.surfaceContainerHighest : base;
    final foreground = muted ? scheme.onSurfaceVariant : Colors.white;
    // Arcade look: a square with a thin border, not a circle.
    return Container(
      width: radius * 2,
      height: radius * 2,
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: background,
        border: Border.all(color: scheme.outline, width: 2),
      ),
      child: icon != null
          ? Icon(icon, size: radius, color: foreground)
          : Text(
              monogram(name),
              // Explicit family and no shadow: an app-bar title's pixel
              // font and red shadow must not leak into the monogram.
              style: TextStyle(
                fontFamily: bodyFont,
                fontFamilyFallback: bodyFontFallback,
                shadows: const <Shadow>[],
                letterSpacing: 0,
                fontSize: radius * 0.9,
                fontWeight: FontWeight.w700,
                color: foreground,
              ),
            ),
    );
  }
}

/// A [NameAvatar] with a small manual-presence dot for [user]
/// (manual-presence: Presence indicator on avatars). Green when the user set
/// Online, the theme's `error` colour when Offline, and **no dot** when the
/// status is unknown (never guess a colour). A `null` [user] — the lobby or a
/// group, which has no single owner — shows the plain avatar with no dot.
class PresenceAvatar extends StatefulWidget {
  const PresenceAvatar({
    super.key,
    required this.core,
    required this.user,
    required this.name,
    this.seed,
    this.radius = 22,
    this.icon,
    this.muted = false,
  });

  final CoreService core;

  /// The user the avatar is for, or `null` for a group/lobby (no dot).
  final UserRef? user;
  final String name;
  final String? seed;
  final double radius;
  final IconData? icon;
  final bool muted;

  @override
  State<PresenceAvatar> createState() => _PresenceAvatarState();
}

class _PresenceAvatarState extends State<PresenceAvatar> {
  PresenceStatus _status = PresenceStatus.unknown;
  StreamSubscription<CoreEvent>? _subscription;

  @override
  void initState() {
    super.initState();
    if (widget.user != null) {
      _subscription = widget.core.events.listen(_onEvent);
      _load();
    }
  }

  @override
  void didUpdateWidget(PresenceAvatar old) {
    super.didUpdateWidget(old);
    if (_key(old.user) != _key(widget.user)) {
      _status = PresenceStatus.unknown;
      _subscription?.cancel();
      _subscription = null;
      if (widget.user != null) {
        _subscription = widget.core.events.listen(_onEvent);
        _load();
      }
    }
  }

  @override
  void dispose() {
    _subscription?.cancel();
    super.dispose();
  }

  static String? _key(UserRef? user) =>
      user == null ? null : '${user.username}@${user.serverId}';

  void _onEvent(CoreEvent event) {
    if (event case CoreEvent_PresenceChanged(
      :final user,
    ) when user != null && _key(user) == _key(widget.user)) {
      _load();
    }
  }

  Future<void> _load() async {
    final user = widget.user;
    if (user == null) return;
    try {
      final status = await widget.core.presenceOf(user);
      if (mounted) setState(() => _status = status);
    } on Object {
      // Leave the last known value; the dot is not critical.
    }
  }

  @override
  Widget build(BuildContext context) {
    final avatar = NameAvatar(
      name: widget.name,
      seed: widget.seed,
      radius: widget.radius,
      icon: widget.icon,
      muted: widget.muted,
    );
    if (widget.user == null || _status == PresenceStatus.unknown) {
      return avatar;
    }
    final scheme = Theme.of(context).colorScheme;
    final online = _status == PresenceStatus.online;
    final dotSize = (widget.radius * 0.5).clamp(8.0, 14.0);
    return Stack(
      clipBehavior: Clip.none,
      children: [
        avatar,
        Positioned(
          right: -1,
          bottom: -1,
          child: Container(
            key: const Key('presence-dot'),
            width: dotSize,
            height: dotSize,
            decoration: BoxDecoration(
              color: online ? presenceOnlineColor : scheme.error,
              border: Border.all(color: scheme.outline, width: 1.5),
            ),
          ),
        ),
      ],
    );
  }
}
