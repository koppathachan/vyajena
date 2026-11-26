// app-ui: Arcade look. The Auto/Light/Dark choice: where it is kept and how
// the screens reach it. Auto (the default) follows the phone.
import 'package:flutter/material.dart';
import 'package:shared_preferences/shared_preferences.dart';

/// Keeps the theme choice between runs.
abstract class ThemeModeStore {
  const ThemeModeStore();

  Future<ThemeMode> load();

  Future<void> save(ThemeMode mode);
}

/// The phone's own small key-value store (the choice only, never content).
class DeviceThemeModeStore extends ThemeModeStore {
  const DeviceThemeModeStore();

  static const _key = 'theme-mode';

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
  MemoryThemeModeStore([this.mode = ThemeMode.system]);

  ThemeMode mode;

  @override
  Future<ThemeMode> load() async => mode;

  @override
  Future<void> save(ThemeMode mode) async => this.mode = mode;
}

/// The live theme choice, shared with the screens through [ThemeModeScope].
class ThemeModeController extends ValueNotifier<ThemeMode> {
  ThemeModeController(this._store) : super(ThemeMode.system);

  final ThemeModeStore _store;

  Future<void> restore() async => value = await _store.load();

  Future<void> choose(ThemeMode mode) async {
    value = mode;
    await _store.save(mode);
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
