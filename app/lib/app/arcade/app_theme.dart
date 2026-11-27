// app-ui: Theme choice. The themes the Profile tab offers; each is a look and
// its colours together (never chosen separately), in a light and a dark
// palette that the Auto/Light/Dark choice picks between.
import 'package:flutter/material.dart';

import 'tokens.dart';

enum AppTheme {
  /// Pixel headings, hand-drawn crooked boxes, bobbing icons; yellow and
  /// purple on navy and cream.
  arcade('Arcade', ArcadeTokens.light, ArcadeTokens.dark),

  /// Straight lines, no shadows or motion, plain headings; teal.
  calm('Calm', ArcadeTokens.calmLight, ArcadeTokens.calmDark);

  const AppTheme(this.label, this.light, this.dark);

  final String label;
  final ArcadeTokens light;
  final ArcadeTokens dark;

  ArcadeTokens tokens(Brightness brightness) =>
      brightness == Brightness.dark ? dark : light;
}
