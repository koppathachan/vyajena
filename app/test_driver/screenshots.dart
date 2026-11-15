// The host side of integration_test/screenshots_test.dart (app-release
// design D15): saves each screenshot the app takes as
// $VYAJENA_SHOTS/<name>.png. Run by app/store/screenshots.sh.
import 'dart:io';

import 'package:integration_test/integration_test_driver_extended.dart';

Future<void> main() => integrationDriver(
  onScreenshot: (name, bytes, [args]) async {
    final dir = Platform.environment['VYAJENA_SHOTS'] ?? 'store/screenshots';
    final file = File('$dir/$name.png');
    file.parent.createSync(recursive: true);
    file.writeAsBytesSync(bytes);
    return true;
  },
);
