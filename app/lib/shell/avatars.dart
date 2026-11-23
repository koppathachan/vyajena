// app-shell: avatars. Nobody has a profile photo yet (a profile is a display
// name and an about text), so every avatar is a round monogram: the first
// letter of the name on a colour taken from the name. A photo can replace the
// monogram later without changing the callers.
import 'package:flutter/material.dart';

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
    return CircleAvatar(
      radius: radius,
      backgroundColor: background,
      foregroundColor: foreground,
      child: icon != null
          ? Icon(icon, size: radius)
          : Text(
              monogram(name),
              style: TextStyle(
                fontSize: radius * 0.9,
                fontWeight: FontWeight.w600,
              ),
            ),
    );
  }
}
