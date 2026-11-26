// app-ui: Arcade look. The one token set, light and dark; widgets read it
// through the ColorScheme and through [ArcadeTokens] for what a ColorScheme
// has no slot for (the heading shadow, the bubble pair, the hard shadows).
import 'package:flutter/material.dart';

/// Square corners everywhere.
const double arcadeRadius = 0;

/// The border width of every card, button, field, dialog and bubble (reduced
/// from the brief's 4px after review on a phone: 4px was too heavy).
const double arcadeBorderWidth = 2;

/// Hard (blur 0) shadow offsets (reduced from 12/4/6 after review; bubbles 2).
const double cardShadow = 6;
const double buttonShadow = 3;
const double bubbleShadow = 2;

/// The heading's red offset shadow.
const double headingShadow = 4;

/// The text scale cap.
const double maxTextScale = 1.3;

const Color _navy = Color(0xFF0F0F23);
const Color _cardDark = Color(0xFF1B1B3A);

/// The dark mode's yellow, a little duller than pure #FFFF00 (owner review 2:
/// full yellow on navy was too harsh for some eyes).
const Color _yellow = Color(0xFFE6D84D);
const Color _red = Color(0xFFFF0000);
const Color _purple = Color(0xFF764BA2);
const Color _cream = Color(0xFFFFF8E7);

/// The arcade palette for one brightness.
@immutable
class ArcadeTokens extends ThemeExtension<ArcadeTokens> {
  const ArcadeTokens({
    required this.brightness,
    required this.bg,
    required this.text,
    required this.border,
    required this.card,
    required this.heading,
    required this.headingShadowColor,
    required this.button,
    required this.onButton,
    required this.focus,
    required this.bubbleMine,
    required this.onBubbleMine,
    required this.bubbleTheirs,
    required this.onBubbleTheirs,
    required this.shadow,
    required this.muted,
    required this.error,
    required this.onError,
    required this.mention,
    required this.disabled,
  });

  final Brightness brightness;
  final Color bg;
  final Color text;
  final Color border;
  final Color card;
  final Color heading;
  final Color headingShadowColor;
  final Color button;
  final Color onButton;
  final Color focus;
  final Color bubbleMine;
  final Color onBubbleMine;
  final Color bubbleTheirs;
  final Color onBubbleTheirs;

  /// The hard shadow colour (black in both modes).
  final Color shadow;

  // Derived (outside the owner's validated palette; contrast-tested).
  final Color muted;
  final Color error;
  final Color onError;
  final Color mention;
  final Color disabled;

  static const dark = ArcadeTokens(
    brightness: Brightness.dark,
    bg: _navy,
    text: Colors.white,
    border: Colors.white,
    card: _cardDark,
    heading: _yellow,
    headingShadowColor: _red,
    button: _yellow,
    onButton: _navy,
    focus: _yellow,
    bubbleMine: _yellow,
    onBubbleMine: _navy,
    bubbleTheirs: _cardDark,
    onBubbleTheirs: Colors.white,
    shadow: Colors.black,
    muted: Color(0xFFB4B4D6),
    error: Color(0xFFFF7B7B),
    onError: _navy,
    mention: Color(0xFF00FFFF),
    disabled: Color(0xFF8A8AAD),
  );

  static const light = ArcadeTokens(
    brightness: Brightness.light,
    bg: _cream,
    text: _navy,
    border: _navy,
    card: Colors.white,
    heading: _navy,
    headingShadowColor: _red,
    button: _purple,
    onButton: Colors.white,
    focus: _navy,
    bubbleMine: _purple,
    onBubbleMine: Colors.white,
    bubbleTheirs: Colors.white,
    onBubbleTheirs: _navy,
    shadow: Colors.black,
    muted: Color(0xFF4A4A6A),
    error: Color(0xFFB00020),
    onError: Colors.white,
    mention: Color(0xFF0057B8),
    disabled: Color(0xFF5E5E78),
  );

  static ArcadeTokens of(Brightness brightness) =>
      brightness == Brightness.dark ? dark : light;

  /// The Material colour scheme these tokens map onto, so stock widgets and
  /// every `colorScheme.x` read in the app follow the palette.
  ColorScheme get scheme => ColorScheme(
    brightness: brightness,
    primary: button,
    onPrimary: onButton,
    primaryContainer: button,
    onPrimaryContainer: onButton,
    secondary: bubbleMine,
    onSecondary: onBubbleMine,
    secondaryContainer: bubbleMine,
    onSecondaryContainer: onBubbleMine,
    tertiary: mention,
    onTertiary: bg,
    error: error,
    onError: onError,
    errorContainer: error,
    onErrorContainer: onError,
    surface: bg,
    onSurface: text,
    onSurfaceVariant: muted,
    surfaceContainerLowest: card,
    surfaceContainerLow: card,
    surfaceContainer: card,
    surfaceContainerHigh: bubbleTheirs,
    surfaceContainerHighest: card,
    outline: border,
    outlineVariant: border,
    shadow: shadow,
    scrim: Colors.black,
    inverseSurface: card,
    onInverseSurface: text,
    inversePrimary: button,
    surfaceTint: Colors.transparent,
  );

  @override
  ArcadeTokens copyWith() => this;

  @override
  ArcadeTokens lerp(ThemeExtension<ArcadeTokens>? other, double t) =>
      t < 0.5 ? this : (other as ArcadeTokens? ?? this);
}

extension ArcadeContext on BuildContext {
  /// The arcade tokens of the current theme.
  ArcadeTokens get arcade =>
      Theme.of(this).extension<ArcadeTokens>() ??
      ArcadeTokens.of(Theme.of(this).brightness);
}
