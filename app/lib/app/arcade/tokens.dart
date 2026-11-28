// app-ui: Arcade look and Theme choice. A theme is a pair of token sets,
// light and dark, holding its colours AND its look (how rough the lines are,
// whether things tilt and bob, pixel headings, hard shadows); widgets read it
// through the ColorScheme and through [ArcadeTokens] for what a ColorScheme
// has no slot for.
import 'package:flutter/material.dart';

/// Square corners everywhere.
const double arcadeRadius = 0;

/// The border width of every card, button, field, dialog and bubble (reduced
/// from the brief's 4px, then from 2px to 1px after owner review 3: still too
/// heavy on a phone).
const double arcadeBorderWidth = 1;

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

/// The dark mode's yellow: a warm amber, duller than pure #FFFF00 and than the
/// lemon #E6D84D (owner review 3: still too bright at night).
const Color _yellow = Color(0xFFD9B84A);
/// The dark heading's offset shadow: a softer red than pure #FF0000, so it
/// does not outshine the dimmer yellow (owner review 3).
const Color _red = Color(0xFFE5484D);
const Color _purple = Color(0xFF764BA2);
/// The light page is butter, and its cards a warm off-white, so no large area
/// reads as pure white (owner review 3).
const Color _butter = Color(0xFFF6EBCB);
const Color _butterCard = Color(0xFFFFFBEF);

/// The light heading's offset shadow: a dull gold, echoing the dark yellow.
const Color _gold = Color(0xFFD9A520);

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
    this.rough = 2,
    this.lively = true,
    this.pixelHeadings = true,
    this.shadowScale = 1,
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

  // The look (app-ui: Theme choice).

  /// How far a box's corners sit off true, in px; 0 draws straight lines.
  final double rough;

  /// Whether boxes are set askew, bubbles offset and icons bob.
  final bool lively;

  /// Pixel-font headings with a red hard shadow; else plain monospace bold.
  final bool pixelHeadings;

  /// 1 for the hard offset shadows, 0 for none.
  final double shadowScale;

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
    bg: _butter,
    text: _navy,
    border: _navy,
    card: _butterCard,
    heading: _navy,
    headingShadowColor: _gold,
    button: _purple,
    onButton: Colors.white,
    focus: _navy,
    bubbleMine: _purple,
    onBubbleMine: Colors.white,
    bubbleTheirs: _butterCard,
    onBubbleTheirs: _navy,
    shadow: Colors.black,
    muted: Color(0xFF4A4A6A),
    error: Color(0xFFB00020),
    onError: Colors.white,
    mention: Color(0xFF0057B8),
    disabled: Color(0xFF5E5E78),
  );

  /// Calm, light: straight lines, no shadows or motion, a teal accent.
  static const calmLight = ArcadeTokens(
    brightness: Brightness.light,
    bg: Color(0xFFF6F7F4),
    text: Color(0xFF1F2328),
    border: Color(0xFF424A53),
    card: Colors.white,
    heading: Color(0xFF1F2328),
    headingShadowColor: Colors.transparent,
    button: Color(0xFF0F766E),
    onButton: Colors.white,
    focus: Color(0xFF0F766E),
    bubbleMine: Color(0xFF0F766E),
    onBubbleMine: Colors.white,
    bubbleTheirs: Colors.white,
    onBubbleTheirs: Color(0xFF1F2328),
    shadow: Colors.black,
    muted: Color(0xFF57606A),
    error: Color(0xFFB3261E),
    onError: Colors.white,
    mention: Color(0xFF0550AE),
    disabled: Color(0xFF59636E),
    rough: 0,
    lively: false,
    pixelHeadings: false,
    shadowScale: 0,
  );

  /// Calm, dark.
  static const calmDark = ArcadeTokens(
    brightness: Brightness.dark,
    bg: Color(0xFF161B22),
    text: Color(0xFFE6EDF3),
    border: Color(0xFF8B949E),
    card: Color(0xFF21262D),
    heading: Color(0xFFE6EDF3),
    headingShadowColor: Colors.transparent,
    button: Color(0xFF5EC4B6),
    onButton: Color(0xFF0D1117),
    focus: Color(0xFF5EC4B6),
    bubbleMine: Color(0xFF5EC4B6),
    onBubbleMine: Color(0xFF0D1117),
    bubbleTheirs: Color(0xFF21262D),
    onBubbleTheirs: Color(0xFFE6EDF3),
    shadow: Colors.black,
    muted: Color(0xFFA0AAB5),
    error: Color(0xFFFF8A80),
    onError: Color(0xFF0D1117),
    mention: Color(0xFF79C0FF),
    disabled: Color(0xFF8B949E),
    rough: 0,
    lively: false,
    pixelHeadings: false,
    shadowScale: 0,
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
