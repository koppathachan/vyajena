// app-ui: Arcade look and Theme choice. The Auto/Light/Dark choice and the
// theme (Arcade, Calm): where they are kept and how the screens reach them.
// Auto and Arcade are the defaults.
import 'package:flutter/material.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'arcade/app_theme.dart';

/// Keeps the appearance and theme choices between runs.
abstract class ThemeModeStore {
  const ThemeModeStore();

  Future<ThemeMode> load();

  Future<void> save(ThemeMode mode);

  Future<AppTheme> loadTheme();

  Future<void> saveTheme(AppTheme theme);
}

/// The phone's own small key-value store (the choice only, never content).
class DeviceThemeModeStore extends ThemeModeStore {
  const DeviceThemeModeStore();

  static const _key = 'theme-mode';
  static const _themeKey = 'app-theme';

  @override
  Future<AppTheme> loadTheme() async {
    try {
      final saved = (await SharedPreferences.getInstance()).getString(
        _themeKey,
      );
      return AppTheme.values.asNameMap()[saved] ?? AppTheme.arcade;
    } catch (_) {
      return AppTheme.arcade;
    }
  }

  @override
  Future<void> saveTheme(AppTheme theme) async {
    try {
      await (await SharedPreferences.getInstance()).setString(
        _themeKey,
        theme.name,
      );
    } catch (_) {
      // Not kept this time; the choice still applies until the app closes.
    }
  }

  @override
  Future<ThemeMode> load() async {
    try {
      final saved = (await SharedPreferences.getInstance()).getString(_key);
      return ThemeMode.values.asNameMap()[saved] ?? ThemeMode.system;
    } catch (_) {
      return ThemeMode.system;
    }
  }

  @override
  Future<void> save(ThemeMode mode) async {
    try {
      await (await SharedPreferences.getInstance()).setString(_key, mode.name);
    } catch (_) {
      // Not kept this time; the choice still applies until the app closes.
    }
  }
}

/// Holds the choice in memory only (widget tests).
class MemoryThemeModeStore extends ThemeModeStore {
  MemoryThemeModeStore([this.mode = ThemeMode.system, this.theme = AppTheme.arcade]);

  ThemeMode mode;
  AppTheme theme;

  @override
  Future<AppTheme> loadTheme() async => theme;

  @override
  Future<void> saveTheme(AppTheme theme) async => this.theme = theme;

  @override
  Future<ThemeMode> load() async => mode;

  @override
  Future<void> save(ThemeMode mode) async => this.mode = mode;
}

/// The live theme choice, shared with the screens through [ThemeModeScope].
class ThemeModeController extends ValueNotifier<ThemeMode> {
  ThemeModeController(this._store) : super(ThemeMode.system);

  final ThemeModeStore _store;

  /// The chosen theme (app-ui: Theme choice).
  final ValueNotifier<AppTheme> theme = ValueNotifier(AppTheme.arcade);

  Future<void> restore() async {
    value = await _store.load();
    theme.value = await _store.loadTheme();
  }

  Future<void> choose(ThemeMode mode) async {
    value = mode;
    await _store.save(mode);
  }

  Future<void> chooseTheme(AppTheme chosen) async {
    theme.value = chosen;
    await _store.saveTheme(chosen);
  }

  @override
  void dispose() {
    theme.dispose();
    super.dispose();
  }
}

class ThemeModeScope extends InheritedWidget {
  const ThemeModeScope({
    super.key,
    required this.controller,
    required super.child,
  });

  final ThemeModeController controller;

  /// The controller, or null outside the app (a screen built alone in a test).
  static ThemeModeController? maybeOf(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<ThemeModeScope>()?.controller;

  @override
  bool updateShouldNotify(ThemeModeScope old) => controller != old.controller;
}
