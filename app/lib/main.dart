// The app's entry point. Screens, CoreService and the theme come with
// flutter-app tasks 4-5; this only loads the Rust bridge.
import 'package:flutter/material.dart';

import 'src/rust/frb_generated.dart';

Future<void> main() async {
  await RustLib.init();
  runApp(const MaterialApp(home: Scaffold()));
}
