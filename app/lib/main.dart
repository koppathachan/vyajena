// The app's entry point: load the Rust bridge, open the core, show the app
// (with the phone's network changes from connectivity_plus and its push
// service).
import 'package:connectivity_plus/connectivity_plus.dart';
import 'package:flutter/material.dart';

import 'app/app.dart';
import 'app/background_task.dart';
import 'core/rust_core_service.dart';
import 'push/push_platform.dart';
import 'src/rust/frb_generated.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await RustLib.init();
  final core = RustCoreService();
  final network = Connectivity().onConnectivityChanged.map(
    (results) => results.any((r) => r != ConnectivityResult.none),
  );
  runApp(
    VyajenaApp(
      core: core,
      networkChanges: network,
      push: MethodChannelPush(),
      background: MethodChannelBackgroundTask(),
    ),
  );
  await core.open();
}
