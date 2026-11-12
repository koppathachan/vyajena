// app-ui: Light Material 3 look (flutter-app design D8): one muted seed,
// the neutral dynamic scheme variant, light and dark from the system, stock
// widgets and the system font.
import 'package:flutter/material.dart';

/// The one seed colour, a muted slate. The owner may change it.
const Color seedColor = Color(0xFF5E6B78);

/// The scheme for [brightness] from [seedColor].
ColorScheme schemeFor(Brightness brightness) => ColorScheme.fromSeed(
  seedColor: seedColor,
  brightness: brightness,
  dynamicSchemeVariant: DynamicSchemeVariant.neutral,
);

/// The light theme.
final ThemeData lightTheme = ThemeData(
  useMaterial3: true,
  colorScheme: schemeFor(Brightness.light),
);

/// The dark theme.
final ThemeData darkTheme = ThemeData(
  useMaterial3: true,
  colorScheme: schemeFor(Brightness.dark),
);

/// Light or dark follows the phone.
const ThemeMode themeMode = ThemeMode.system;
