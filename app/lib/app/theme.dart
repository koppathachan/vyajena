// app-ui: Arcade look (change arcade-restyle). One token set (arcade/tokens.dart),
// light and dark from the system: square corners, 2px borders, hard offset
// shadows, Press Start 2P for headings and labels, the platform monospace for
// body text. Stock Material widgets, themed here, so screens rarely style
// themselves.
import 'package:flutter/material.dart';

import 'arcade/app_theme.dart';
import 'arcade/arcade_border.dart';
import 'arcade/steps.dart';
import 'arcade/tokens.dart';

/// The bundled pixel font (SIL OFL), for headings and labels only.
const String pixelFont = 'PressStart2P';

/// The body font: the platform's monospace.
const String bodyFont = 'monospace';
const List<String> bodyFontFallback = ['Menlo', 'Courier', 'Roboto Mono'];

TextStyle _pixel(double size, Color color, {List<Shadow>? shadows}) =>
    TextStyle(
      letterSpacing: 1,
      fontFamily: pixelFont,
      fontFamilyFallback: bodyFontFallback,
      fontSize: size,
      height: 1.5,
      color: color,
      shadows: shadows,
    );

/// A heading or label in [t]'s heading font: the pixel font, or (a theme
/// without pixel headings) the monospace body in bold, a little larger since
/// the pixel font is one em per glyph.
TextStyle _head(
  ArcadeTokens t,
  double size,
  Color color, {
  List<Shadow>? shadows,
}) => t.pixelHeadings
    ? _pixel(size, color, shadows: shadows)
    : _mono(size < 12 ? size + 4 : size + 2, color, weight: FontWeight.w700);

TextStyle _mono(double size, Color color, {FontWeight? weight}) => TextStyle(
  fontFamily: bodyFont,
  fontFamilyFallback: bodyFontFallback,
  fontSize: size,
  height: 1.4,
  color: color,
  fontWeight: weight,
);

/// A heading's red hard shadow. Dark text on a light background reads
/// thinner than light on dark, so in light mode the heading is also drawn a
/// second time 0.6px to the right (a faux bold that keeps the gaps inside
/// letters such as "m" open) to match the dark mode's weight.
List<Shadow> headingShadows(ArcadeTokens t, [double offset = headingShadow]) =>
    !t.pixelHeadings
    ? const <Shadow>[]
    : [
      Shadow(offset: Offset(offset, offset), color: t.headingShadowColor),
      if (t.brightness == Brightness.light)
        Shadow(offset: const Offset(0.6, 0), color: t.heading),
    ];

/// The text theme: pixel headings and labels, monospace body, never a pixel
/// body.
TextTheme textThemeFor(ArcadeTokens t) {
  final heading = headingShadows(t);
  return TextTheme(
    displayLarge: _head(t, 32, t.heading, shadows: heading),
    displayMedium: _head(t, 28, t.heading, shadows: heading),
    displaySmall: _head(t, 24, t.heading, shadows: heading),
    headlineLarge: _head(t, 22, t.heading, shadows: heading),
    headlineMedium: _head(t, 20, t.heading, shadows: heading),
    headlineSmall: _head(t, 16, t.heading, shadows: heading),
    // App-bar and small titles: a 2px shadow, so 16px pixel text stays legible.
    titleLarge: _head(t, 16, t.heading, shadows: headingShadows(t, 2)),
    titleMedium: _mono(15, t.text, weight: FontWeight.bold),
    titleSmall: _mono(13, t.text, weight: FontWeight.bold),
    labelLarge: _head(t, 10, t.text),
    labelMedium: _head(t, 8, t.text),
    labelSmall: _mono(11, t.muted),
    bodyLarge: _mono(15, t.text),
    bodyMedium: _mono(13, t.text),
    bodySmall: _mono(11, t.muted),
  );
}

ArcadeBorder _box(
  ArcadeTokens t, {
  double shadow = 0,
  Color? color,
  int seed = 0,
}) => ArcadeBorder(
  color: color ?? t.border,
  shadow: shadow * t.shadowScale,
  shadowColor: t.shadow,
  seed: seed,
  rough: t.rough,
);

SketchInputBorder _field(ArcadeTokens t, Color color) => SketchInputBorder(
  borderSide: BorderSide(color: color, width: arcadeBorderWidth),
  seed: 11,
  rough: t.rough,
);

ButtonStyle _buttonStyle(
  ArcadeTokens t, {
  required Color fill,
  required Color onFill,
  bool border = true,
}) => ButtonStyle(
  backgroundColor: WidgetStateProperty.resolveWith(
    (s) => s.contains(WidgetState.disabled) ? t.disabled : fill,
  ),
  foregroundColor: WidgetStateProperty.resolveWith(
    (s) => s.contains(WidgetState.disabled) ? t.bg : onFill,
  ),
  overlayColor: WidgetStateProperty.resolveWith(
    (s) => s.contains(WidgetState.focused)
        ? t.focus.withValues(alpha: 0.35)
        : s.contains(WidgetState.pressed) || s.contains(WidgetState.hovered)
        ? onFill.withValues(alpha: 0.15)
        : null,
  ),
  elevation: const WidgetStatePropertyAll(0),
  shadowColor: const WidgetStatePropertyAll(Colors.transparent),
  surfaceTintColor: const WidgetStatePropertyAll(Colors.transparent),
  // Pressed (or hovered): the shadow is gone and the face has "moved" onto it.
  shape: WidgetStateProperty.resolveWith(
    (s) => border
        ? _box(
            t,
            shadow:
                s.contains(WidgetState.pressed) ||
                    s.contains(WidgetState.hovered) ||
                    s.contains(WidgetState.disabled)
                ? 0
                : buttonShadow,
          )
        : const RoundedRectangleBorder(),
  ),
  textStyle: WidgetStatePropertyAll(_head(t, 10, t.text)),
  minimumSize: const WidgetStatePropertyAll(Size(64, 44)),
  padding: const WidgetStatePropertyAll(
    EdgeInsets.symmetric(horizontal: 14, vertical: 10),
  ),
  animationDuration: hoverDuration,
  splashFactory: NoSplash.splashFactory,
);

/// The theme for [t].
ThemeData arcadeTheme(ArcadeTokens t) {
  final text = textThemeFor(t);
  final scheme = t.scheme;
  return ThemeData(
    useMaterial3: true,
    brightness: t.brightness,
    colorScheme: scheme,
    extensions: [t],
    scaffoldBackgroundColor: t.bg,
    canvasColor: t.bg,
    textTheme: text,
    primaryTextTheme: text,
    splashFactory: NoSplash.splashFactory,
    highlightColor: t.focus.withValues(alpha: 0.2),
    focusColor: t.focus.withValues(alpha: 0.35),
    hoverColor: Colors.transparent,
    dividerColor: t.border,
    dividerTheme: DividerThemeData(color: t.border, thickness: 2, space: 2),
    iconTheme: IconThemeData(color: t.text),
    appBarTheme: AppBarTheme(
      backgroundColor: t.card,
      foregroundColor: t.text,
      surfaceTintColor: Colors.transparent,
      elevation: 0,
      scrolledUnderElevation: 0,
      centerTitle: false,
      titleTextStyle: text.titleLarge,
      shape: SketchEdgeBorder(
        color: t.border,
        bottom: true,
        seed: 3,
        rough: t.rough,
      ),
    ),
    cardTheme: CardThemeData(
      color: t.card,
      elevation: 0,
      shadowColor: Colors.transparent,
      surfaceTintColor: Colors.transparent,
      shape: _box(t, shadow: cardShadow, seed: 1),
      margin: EdgeInsets.fromLTRB(
        2,
        2,
        cardShadow * t.shadowScale + 2,
        cardShadow * t.shadowScale + 2,
      ),
    ),
    filledButtonTheme: FilledButtonThemeData(
      style: _buttonStyle(t, fill: t.button, onFill: t.onButton),
    ),
    elevatedButtonTheme: ElevatedButtonThemeData(
      style: _buttonStyle(t, fill: t.button, onFill: t.onButton),
    ),
    outlinedButtonTheme: OutlinedButtonThemeData(
      style: _buttonStyle(t, fill: t.card, onFill: t.text),
    ),
    textButtonTheme: TextButtonThemeData(
      // Borderless text buttons read as sentences ("I already have an
      // account: link this device"), so they use the monospace body, bold.
      style:
          _buttonStyle(
            t,
            fill: Colors.transparent,
            onFill: t.text,
            border: false,
          ).copyWith(
            textStyle: WidgetStatePropertyAll(
              _mono(13, t.text, weight: FontWeight.bold),
            ),
          ),
    ),
    iconButtonTheme: IconButtonThemeData(
      style: ButtonStyle(
        shape: const WidgetStatePropertyAll(RoundedRectangleBorder()),
        foregroundColor: WidgetStatePropertyAll(t.text),
        splashFactory: NoSplash.splashFactory,
      ),
    ),
    floatingActionButtonTheme: FloatingActionButtonThemeData(
      backgroundColor: t.button,
      foregroundColor: t.onButton,
      elevation: 0,
      focusElevation: 0,
      hoverElevation: 0,
      highlightElevation: 0,
      shape: _box(t, shadow: buttonShadow, seed: 5),
    ),
    inputDecorationTheme: InputDecorationTheme(
      filled: true,
      fillColor: t.card,
      border: _field(t, t.border),
      enabledBorder: _field(t, t.border),
      focusedBorder: _field(t, t.focus),
      errorBorder: _field(t, t.error),
      focusedErrorBorder: _field(t, t.error),
      disabledBorder: _field(t, t.disabled),
      labelStyle: _mono(14, t.muted),
      hintStyle: _mono(14, t.muted),
      errorStyle: _mono(12, t.error),
    ),
    textSelectionTheme: TextSelectionThemeData(
      cursorColor: t.focus,
      selectionColor: t.focus.withValues(alpha: 0.35),
      selectionHandleColor: t.focus,
    ),
    dialogTheme: DialogThemeData(
      insetPadding: const EdgeInsets.symmetric(horizontal: 20, vertical: 24),
      backgroundColor: t.card,
      elevation: 0,
      surfaceTintColor: Colors.transparent,
      shape: _box(t, shadow: cardShadow, seed: 7),
      titleTextStyle: text.titleLarge,
      contentTextStyle: text.bodyMedium,
    ),
    bottomSheetTheme: BottomSheetThemeData(
      backgroundColor: t.card,
      modalBackgroundColor: t.card,
      surfaceTintColor: Colors.transparent,
      elevation: 0,
      shape: _box(t),
    ),
    popupMenuTheme: PopupMenuThemeData(
      color: t.card,
      surfaceTintColor: Colors.transparent,
      elevation: 0,
      shape: _box(t, shadow: buttonShadow),
      textStyle: text.bodyMedium,
      // Menu entries are sentences: the monospace body, not the pixel label.
      labelTextStyle: WidgetStateProperty.resolveWith(
        (s) => s.contains(WidgetState.disabled)
            ? text.bodyMedium?.copyWith(color: t.disabled)
            : text.bodyMedium,
      ),
    ),
    snackBarTheme: SnackBarThemeData(
      backgroundColor: t.card,
      contentTextStyle: text.bodyMedium,
      actionTextColor: t.heading,
      elevation: 0,
      shape: _box(t),
    ),
    listTileTheme: ListTileThemeData(
      textColor: t.text,
      iconColor: t.text,
      shape: const RoundedRectangleBorder(),
    ),
    navigationBarTheme: NavigationBarThemeData(
      backgroundColor: t.card,
      surfaceTintColor: Colors.transparent,
      elevation: 0,
      indicatorColor: t.button,
      indicatorShape: _box(t, seed: 17),
      labelTextStyle: WidgetStatePropertyAll(_head(t, 8, t.text)),
      iconTheme: WidgetStateProperty.resolveWith(
        (s) => IconThemeData(
          color: s.contains(WidgetState.selected) ? t.onButton : t.text,
        ),
      ),
    ),
    progressIndicatorTheme: ProgressIndicatorThemeData(
      color: t.heading,
      circularTrackColor: Colors.transparent,
    ),
    tooltipTheme: TooltipThemeData(
      decoration: sketchBox(
        border: t.border,
        fill: t.card,
        seed: 13,
        rough: t.rough,
      ),
      textStyle: text.bodySmall?.copyWith(color: t.text),
    ),
    segmentedButtonTheme: SegmentedButtonThemeData(
      style: ButtonStyle(
        shape: WidgetStatePropertyAll(_box(t)),
        side: WidgetStatePropertyAll(
          BorderSide(color: t.border, width: arcadeBorderWidth),
        ),
        backgroundColor: WidgetStateProperty.resolveWith(
          (s) => s.contains(WidgetState.selected) ? t.button : t.card,
        ),
        foregroundColor: WidgetStateProperty.resolveWith(
          (s) => s.contains(WidgetState.selected) ? t.onButton : t.text,
        ),
        textStyle: WidgetStatePropertyAll(_head(t, 8, t.text)),
        splashFactory: NoSplash.splashFactory,
      ),
    ),
    switchTheme: SwitchThemeData(
      trackOutlineColor: WidgetStatePropertyAll(t.border),
      thumbColor: WidgetStatePropertyAll(t.heading),
      trackColor: WidgetStateProperty.resolveWith(
        (s) => s.contains(WidgetState.selected) ? t.button : t.card,
      ),
    ),
  );
}

/// The Arcade theme, light.
final ThemeData lightTheme = arcadeTheme(ArcadeTokens.light);

/// The Arcade theme, dark.
final ThemeData darkTheme = arcadeTheme(ArcadeTokens.dark);

final Map<ArcadeTokens, ThemeData> _built = {
  ArcadeTokens.light: lightTheme,
  ArcadeTokens.dark: darkTheme,
};

/// The Material theme of [theme] in [brightness] (app-ui: Theme choice).
ThemeData themeFor(AppTheme theme, Brightness brightness) {
  final t = theme.tokens(brightness);
  return _built.putIfAbsent(t, () => arcadeTheme(t));
}

/// Light or dark follows the phone.
const ThemeMode themeMode = ThemeMode.system;
