// app-ui: UI goes through the core. The Dart code under lib/ opens no
// connection (only the Rust core does, over the bridge), and its debug log
// names events, never their text.
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/core/core_service.dart';

import '../support/app.dart';
import '../support/fake_core_service.dart';

/// Every Dart file under lib/, generated bindings included.
List<File> libFiles() =>
    Directory('lib')
        .listSync(recursive: true)
        .whereType<File>()
        .where((f) => f.path.endsWith('.dart'))
        .toList()
      ..sort((a, b) => a.path.compareTo(b.path));

/// The URIs a file imports or exports.
Iterable<String> directives(File file) => RegExp(
  r"""^\s*(?:import|export)\s+['"]([^'"]+)['"]""",
  multiLine: true,
).allMatches(file.readAsStringSync()).map((m) => m.group(1)!);

/// Whether [file] is the generated bridge binding (lib/src/rust/).
bool generated(File file) =>
    file.path.replaceAll(r'\', '/').startsWith('lib/src/rust/');

/// The Dart core libraries lib/ may use: none of them does I/O.
const allowedDart = {
  'dart:async',
  'dart:collection',
  'dart:convert',
  'dart:math',
  'dart:typed_data',
};

/// Packages lib/ may import. `flutter/services.dart` is there for the
/// `vyajena/platform` MethodChannel (data folder, share sheet) and the
/// clipboard; it has no socket or HTTP client.
const allowedFlutter = {
  'package:flutter/foundation.dart',
  'package:flutter/gestures.dart',
  'package:flutter/material.dart',
  'package:flutter/services.dart',
  'package:flutter/widgets.dart',
};

/// What only main.dart may import: the phone's network state, to tell the
/// core to reconnect (qa-fixes design D6). It reads whether Wi-Fi or mobile
/// data is up and opens no connection.
const mainOnly = {'package:connectivity_plus/connectivity_plus.dart'};

/// What only the one picture-picker file may import: the phone's gallery
/// (app-ui: Send an image). It picks a file the user chooses and opens no
/// connection.
const pickerOnly = {'package:image_picker/image_picker.dart'};

/// What only the link sheet's file may import: it hands a tapped URL to the
/// OS browser (app-ui: Links in messages). It opens no connection in Dart.
const linksOnly = {'package:url_launcher/url_launcher.dart'};

/// What only the theme-choice file may import: the phone's small key-value
/// store, for the Auto/Light/Dark choice (app-ui: Arcade look). One string; no
/// message content, no network.
const themeOnly = {'package:shared_preferences/shared_preferences.dart'};

/// What only the sticker preview may import: `dart:ui` to draw the cutout's
/// RGBA over a checkerboard (app-ui: Create a sticker). It is a rendering API
/// (`decodeImageFromPixels`, `Canvas`) with no socket, HTTP client or file API.
const stickerPreviewOnly = {'dart:ui'};

/// What only the one voice-plugin file may import: the microphone and the
/// speaker (app-ui: Record a voice message; Play a voice message). It records
/// to a temporary file and plays a file the core cached; it opens no
/// connection, and `dart:io` there is for those files only.
const voiceOnly = {
  'dart:io',
  'package:record/record.dart',
  'package:just_audio/just_audio.dart',
};

/// The video files (app-ui: Send a video; Video bubble and player):
/// `video_plugins.dart` picks/records and normalizes a clip through the native
/// plugin, `video_bubble.dart` plays the core's cached file. Neither opens a
/// connection (the player only ever opens a local file).
const videoFiles = {'lib/chat/video_plugins.dart', 'lib/chat/video_bubble.dart'};

/// The files that may read local media files via `dart:io` (a temporary
/// recording, a picked clip, the core's cached MP4): the voice and video
/// plugins and the video bubble. None holds network code.
const ioFiles = {
  'lib/chat/voice_plugins.dart',
  'lib/chat/video_plugins.dart',
  'lib/chat/video_bubble.dart',
};

/// The files that may pick a file from the gallery (`image_picker`): the one
/// picture picker and the video plugins. Neither opens a connection.
const pickerFiles = {
  'lib/chat/picture_picker.dart',
  'lib/chat/video_plugins.dart',
};

/// What only the generated bindings may import: FFI to the Rust core.
const bindingOnly = {
  'dart:ffi',
  'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart',
  'package:flutter_rust_bridge/flutter_rust_bridge_for_generated_io.dart',
  'package:freezed_annotation/freezed_annotation.dart',
};

void main() {
  // spec: app-ui/ui-goes-through-the-core
  test('No socket in Dart: lib/ imports no dart:io, no HTTP client and no '
      'network package', () {
    final bad = <String>[];
    for (final file in libFiles()) {
      for (final uri in directives(file)) {
        final ok = switch (uri) {
          _ when !uri.contains(':') => true, // a file of this package
          _ when uri.startsWith('package:vyajena/') => true,
          _ when allowedDart.contains(uri) => true,
          _ when allowedFlutter.contains(uri) => true,
          _ when bindingOnly.contains(uri) => generated(file),
          _ when mainOnly.contains(uri) =>
            file.path.replaceAll(r'\', '/') == 'lib/main.dart',
          _ when linksOnly.contains(uri) =>
            file.path.replaceAll(r'\', '/') == 'lib/chat/links.dart',
          _ when themeOnly.contains(uri) =>
            file.path.replaceAll(r'\', '/') == 'lib/app/theme_mode.dart',
          // dart:io, image_picker and video_player overlap across the voice and
          // video files, so these are matched by uri before the set arms below.
          'dart:io' => ioFiles.contains(file.path.replaceAll(r'\', '/')),
          'package:image_picker/image_picker.dart' =>
            pickerFiles.contains(file.path.replaceAll(r'\', '/')),
          'package:video_player/video_player.dart' =>
            videoFiles.contains(file.path.replaceAll(r'\', '/')),
          _ when pickerOnly.contains(uri) =>
            file.path.replaceAll(r'\', '/') == 'lib/chat/picture_picker.dart',
          _ when stickerPreviewOnly.contains(uri) =>
            file.path.replaceAll(r'\', '/') == 'lib/chat/sticker_preview.dart',
          _ when voiceOnly.contains(uri) =>
            file.path.replaceAll(r'\', '/') == 'lib/chat/voice_plugins.dart',
          _ => false,
        };
        if (!ok) bad.add('${file.path}: $uri');
      }
    }
    expect(bad, isEmpty, reason: 'imports that could reach the network');
  });

  // spec: app-ui/ui-goes-through-the-core
  test('No socket in Dart: no socket, HTTP or file API is named in lib/', () {
    final api = RegExp(
      r'\b(Socket|RawSocket|ServerSocket|SecureSocket|RawDatagramSocket|'
      r'WebSocket|HttpClient|HttpServer|File|RandomAccessFile|Process)\b',
    );
    final bad = [
      for (final file in libFiles())
        // The voice plugins' temporary recording and the video files' cached
        // MP4 are the files this app touches; they hold no network code.
        if (file.path.replaceAll(r'\', '/') != 'lib/chat/voice_plugins.dart' &&
            !videoFiles.contains(file.path.replaceAll(r'\', '/')))
          for (final m in api.allMatches(file.readAsStringSync()))
            '${file.path}: ${m.group(0)}',
    ];
    expect(bad, isEmpty);
  });

  // spec: app-ui/ui-goes-through-the-core
  test('the app depends on no network package', () {
    final pubspec = File('pubspec.yaml').readAsStringSync();
    final section = RegExp(
      r'^dependencies:\n((?:[ #].*\n|\n)*)',
      multiLine: true,
    ).firstMatch(pubspec)!.group(1)!;
    final names = RegExp(
      r'^  ([a-z_]+):',
      multiLine: true,
    ).allMatches(section).map((m) => m.group(1)!).toSet();
    // connectivity_plus only reports whether a connection is up (D6);
    // image_picker only picks a file from the gallery (Send an image);
    // url_launcher only hands a tapped URL to the OS browser (Links in
    // messages).
    // record and just_audio only use the microphone and the speaker (Record a
    // voice message; Play a voice message).
    // video_player only plays the core's cached local MP4 (Video bubble and
    // player); image_picker also picks the video to send (Send a video).
    // shared_preferences only keeps the Auto/Light/Dark choice on the phone
    // (Arcade look).
    expect(names, {
      'flutter',
      'bridge',
      'flutter_rust_bridge',
      'freezed_annotation',
      'connectivity_plus',
      'image_picker',
      'url_launcher',
      'record',
      'just_audio',
      'video_player',
      'shared_preferences',
    });
  });

  // spec: app-ui/ui-goes-through-the-core
  test('every server call goes through a bridge function: only the core '
      'service and the two entry points reach the bindings', () {
    final bad = <String>[];
    for (final file in libFiles()) {
      if (generated(file)) continue;
      final path = file.path.replaceAll(r'\', '/');
      for (final uri in directives(file)) {
        if (!uri.contains('src/rust/')) continue;
        final ok = switch (path) {
          'lib/main.dart' => uri == 'src/rust/frb_generated.dart',
          // The second entry point: the headless engine an Android data wake
          // starts when the app is closed (smarter-notifications).
          'lib/push/wake_entry.dart' => uri == '../src/rust/frb_generated.dart',
          'lib/core/rust_core_service.dart' => true,
          // The plain types and errors, re-exported to the screens.
          'lib/core/core_service.dart' =>
            uri.endsWith('api/types.dart') || uri.endsWith('api/error.dart'),
          _ => false,
        };
        if (!ok) bad.add('$path: $uri');
      }
    }
    expect(bad, isEmpty);
  });

  // spec: app-ui/ui-goes-through-the-core
  test('nothing in lib/ prints or logs but the core service\'s one event '
      'line', () {
    final call = RegExp(r'\b(print|debugPrint|log|debugPrintStack)\(');
    final bad = [
      for (final file in libFiles())
        for (final m in call.allMatches(file.readAsStringSync()))
          if (file.path.replaceAll(r'\', '/') != 'lib/core/core_service.dart')
            '${file.path}: ${m.group(0)}',
    ];
    expect(bad, isEmpty);
  });

  // spec: app-ui/ui-goes-through-the-core
  testWidgets('Nothing logged: Alice receives "hi" and the debug log names '
      'the kind of event but not "hi"', (tester) async {
    final log = <String>[];
    final saved = debugPrint;
    debugPrint = (message, {wrapWidth}) => log.add(message ?? '');

    final core = await openApp(tester, FakeCoreService(storedUser: alice));
    core.receive(bob, 'hi');
    core.emit(
      const CoreEvent.profileChanged(
        user: bob,
        displayName: 'Zed',
        about: 'secret about',
      ),
    );
    await settle(tester);
    await tester.enterText(find.byKey(const Key('composer')), 'yo there');
    await tap(tester, find.byTooltip('Send'));
    core.setStatus(core.messages.last.id, const SendState.accepted());
    await settle(tester);

    // Flutter checks debugPrint is restored before the test ends.
    debugPrint = saved;

    expect(find.text('hi'), findsOneWidget);
    expect(log, contains(startsWith('core event: message')));
    expect(log, contains(startsWith('core event: send-status')));
    for (final secret in [
      'hi',
      'Zed',
      'secret about',
      'Hidden group',
      'yo there',
    ]) {
      expect(
        log.where((line) => line.contains(secret)),
        isEmpty,
        reason: '"$secret" was logged',
      );
    }
  });
}
