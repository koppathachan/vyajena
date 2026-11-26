// app-ui: Lobby row styling (change lobby-distinct), in the Arcade look: the
// lobby is the system space, so its chat-list row carries a `#` tile, an
// italic name, a "system" pill and a faint accent tint. Only the lobby.
import 'package:flutter/material.dart';

import '../app/theme.dart';

/// The `#` hash tile that replaces the lobby's plain avatar: a bordered square
/// in the accent (button) colours, as wide as a [NameAvatar] of [radius].
class LobbyHashTile extends StatelessWidget {
  const LobbyHashTile({super.key, this.radius = 22});

  final double radius;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Container(
      key: const Key('lobby-hash-tile'),
      width: radius * 2,
      height: radius * 2,
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: scheme.primary,
        border: Border.all(color: scheme.outline, width: 2),
      ),
      child: Text(
        '#',
        style: TextStyle(
          fontFamily: pixelFont,
          fontFamilyFallback: bodyFontFallback,
          shadows: const <Shadow>[],
          fontSize: radius,
          height: 1,
          color: scheme.onPrimary,
        ),
      ),
    );
  }
}

/// The small "system" tag beside the lobby's name.
class SystemPill extends StatelessWidget {
  const SystemPill({super.key});

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Container(
      key: const Key('lobby-system-pill'),
      padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 3),
      decoration: BoxDecoration(
        color: scheme.primary,
        border: Border.all(color: scheme.outline, width: 1.5),
      ),
      child: Text(
        'system',
        style: TextStyle(
          fontFamily: pixelFont,
          fontFamilyFallback: bodyFontFallback,
          shadows: const <Shadow>[],
          fontSize: 8,
          height: 1,
          color: scheme.onPrimary,
        ),
      ),
    );
  }
}

/// The faint accent tint behind the lobby row.
Color lobbyRowTint(BuildContext context) =>
    Theme.of(context).colorScheme.primary.withValues(alpha: 0.18);
