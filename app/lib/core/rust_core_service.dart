// CoreService over the real Rust core (flutter_rust_bridge). The only file
// in lib/ besides main.dart that touches the bridge's Core.
import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

import '../src/rust/api/core.dart' as rust;
import '../src/rust/api/text.dart' as rust_text;
import 'core_service.dart';
import 'event_label.dart';

/// The platform channel that hands Dart the core's data folder (app-ui:
/// Data on the phone): iOS Application Support/core (no iCloud backup,
/// NSFileProtectionComplete), Android noBackupFilesDir/core.
const platformChannel = MethodChannel('vyajena/platform');

/// The real core.
class RustCoreService extends CoreService {
  rust.Core? _core;
  StreamSubscription<CoreEvent>? _subscription;

  rust.Core get _c {
    final core = _core;
    if (core == null) throw StateError('the core is not open');
    return core;
  }

  @override
  Future<void> openCore() async {
    final dataDir = await platformChannel.invokeMethod<String>('dataDir');
    if (dataDir == null) throw StateError('no data folder from the platform');
    final core = await rust.Core.open(dataDir: dataDir);
    _core = core;
    _subscription = core.events().listen((event) {
      // Names the event only, never its text (app-ui: UI goes through the
      // core).
      if (kDebugMode) debugPrint('core event: ${eventLabel(event)}');
      handleEvent(event);
    });
  }

  @override
  Future<void> startCore() => _c.start();

  @override
  Future<void> stopCore() => _c.stop();

  @override
  Future<void> dispose() async {
    await _subscription?.cancel();
    await super.dispose();
  }

  @override
  Future<UserRef?> userId() => _c.userId();

  @override
  Future<String?> deviceId() => _c.deviceId();

  @override
  InvitePreview parseInvite(String text) => rust_text.parseInvite(text: text);

  @override
  bool textFits(String text) => rust.textFits(text: text);

  @override
  Future<UserRef> register(String inviteText) async {
    final user = await _c.register(inviteText: inviteText);
    await becameUser(user);
    return user;
  }

  @override
  Future<String> linkRequest() => _c.linkRequest();

  @override
  Future<UserRef> completeLink(String approvalText) async {
    final user = await _c.completeLink(approvalText: approvalText);
    await becameUser(user);
    return user;
  }

  @override
  Future<String> approveLink(String requestText) =>
      _c.approveLink(requestText: requestText);

  @override
  Future<void> revokeDevice(String device) => _c.revokeDevice(device: device);

  @override
  Future<List<ChatMessage>> history(int limit) => _c.history(limit: limit);

  @override
  Future<ChatMessage?> message(int id) => _c.message(id: id);

  @override
  Future<int> sendText(String text) => _c.sendText(text: text);

  @override
  Future<String> displayName(UserRef user) => _c.displayName(user: user);

  @override
  Future<ProfileInfo> profile(UserRef user) => _c.profile(user: user);

  @override
  Future<int> setProfile({String? displayName, String? about}) =>
      _c.setProfile(displayName: displayName, about: about);

  @override
  Future<List<MemberDevice>> members() => _c.members();

  @override
  Future<String?> groupName() => _c.groupName();

  @override
  Future<BigInt?> epoch() => _c.epoch();

  @override
  Future<bool> isAdmin() => _c.isAdmin();

  @override
  Future<void> createGroup() => _c.createGroup();

  @override
  Future<InviteInfo> createInvite(String username) =>
      _c.createInvite(username: username);

  @override
  Future<List<InviteInfo>> listInvites() => _c.listInvites();

  @override
  Future<void> revokeInvite(String code) => _c.revokeInvite(code: code);

  @override
  Future<void> addUser(UserRef user) => _c.addUser(user: user);

  @override
  Future<void> removeUser(UserRef user) => _c.removeUser(user: user);

  @override
  Future<int> setGroupName(String? name) => _c.setGroupName(name: name);

  @override
  Future<bool> acceptIdentityKey(UserRef user) =>
      _c.acceptIdentityKey(user: user);
}
