// The app's entry point: load the Rust bridge, open the core, show the app.
import 'package:flutter/material.dart';

import 'app/app.dart';
import 'core/rust_core_service.dart';
import 'src/rust/frb_generated.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await RustLib.init();
  final core = RustCoreService();
  runApp(VyajenaApp(core: core));
  await core.open();
}
