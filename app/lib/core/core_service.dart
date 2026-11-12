// The one Dart door to the Rust core (flutter-app design D1, D3; app-ui: UI
// goes through the core). Screens take a CoreService and never import the
// bridge's `Core` or `frb_generated.dart`; they may use the bridge's plain
// types (`src/rust/api/types.dart`, `error.dart`). RustCoreService is the
// real one; widget tests use a fake (test/support/fake_core_service.dart).
//
// Decrypted text passes through here only to be shown: nothing in this
// class writes it to a file or a log.
import 'dart:async';

import 'package:flutter/foundation.dart';

import '../src/rust/api/types.dart';

export '../src/rust/api/error.dart' show CoreError, CoreErrorKind;
export '../src/rust/api/types.dart';

/// Where the core stands, for the app shell.
enum CoreState {
  /// Not opened yet, or opening.
  opening,

  /// Open; no user on this device yet (join or link next).
  unregistered,

  /// Open with a registered or linked user; the engine runs while the app
  /// is in the foreground.
  ready,

  /// Opening failed (see [CoreService.openError]).
  failed,
}

/// The app's view of the core: state as [ValueListenable]s, events as one
/// broadcast stream in the core's order, and one method per core call.
abstract class CoreService {
  final ValueNotifier<CoreState> _state = ValueNotifier(CoreState.opening);
  final ValueNotifier<ConnectionStatus> _connection = ValueNotifier(
    ConnectionStatus.disconnected,
  );
  final ValueNotifier<UserRef?> _user = ValueNotifier(null);
  final StreamController<CoreEvent> _events =
      StreamController<CoreEvent>.broadcast();
  Object? _openError;
  bool _running = false;

  /// Opening, unregistered, ready or failed.
  ValueListenable<CoreState> get state => _state;

  /// Connected, connecting or disconnected (app-ui: Foreground and
  /// background).
  ValueListenable<ConnectionStatus> get connection => _connection;

  /// This device's user, once registered or linked.
  ValueListenable<UserRef?> get user => _user;

  /// Every core event, in the core's order.
  Stream<CoreEvent> get events => _events.stream;

  /// Why opening failed, when [state] is [CoreState.failed].
  Object? get openError => _openError;

  /// Whether the engine is running (started and not stopped).
  bool get running => _running;

  /// Opens the core and, when this device has a user, starts it (app-ui:
  /// Foreground and background: the core starts when the app opens).
  Future<void> open() async {
    try {
      await openCore();
      _user.value = await userId();
      _state.value = _user.value == null
          ? CoreState.unregistered
          : CoreState.ready;
      if (_user.value != null) await start();
    } catch (error) {
      _openError = error;
      _state.value = CoreState.failed;
    }
  }

  /// Starts (or restarts) the engine: connect, catch up, stay connected.
  /// Does nothing until this device has a user.
  Future<void> start() async {
    if (_user.value == null) return;
    _connection.value = ConnectionStatus.connecting;
    _running = true;
    await startCore();
  }

  /// Stops the engine and closes the connection.
  Future<void> stop() async {
    if (!_running) return;
    _running = false;
    await stopCore();
    _connection.value = ConnectionStatus.disconnected;
  }

  /// Records the user after [register] or [completeLink] and starts.
  @protected
  Future<void> becameUser(UserRef user) async {
    _user.value = user;
    _state.value = CoreState.ready;
    await start();
  }

  /// Passes one core event on, keeping [connection] up to date.
  @protected
  void handleEvent(CoreEvent event) {
    if (event case CoreEvent_Connection(:final status)) {
      _connection.value = status;
    }
    _events.add(event);
  }

  /// Releases the notifiers and the stream.
  @mustCallSuper
  Future<void> dispose() async {
    await _events.close();
    _state.dispose();
    _connection.dispose();
    _user.dispose();
  }

  // --- Implemented by the real core and the fake ---

  /// Opens the core and subscribes to its events (via [handleEvent]).
  @protected
  Future<void> openCore();

  /// The core's start.
  @protected
  Future<void> startCore();

  /// The core's stop.
  @protected
  Future<void> stopCore();

  /// This device's user, once registered or linked.
  Future<UserRef?> userId();

  /// This device's id (hex), once registered or linked.
  Future<String?> deviceId();

  /// The user and relay address an invite names, to show before joining.
  /// Throws [CoreError] for text that is not an invite.
  InvitePreview parseInvite(String text);

  /// Whether [text] fits in one message (app-ui: Composer limit).
  bool textFits(String text);

  /// Signs up this device from an invite text, then starts.
  Future<UserRef> register(String inviteText);

  /// New device, step 1: the link request text to show.
  Future<String> linkRequest();

  /// New device, step 2: completes linking with the approval text, then
  /// starts.
  Future<UserRef> completeLink(String approvalText);

  /// Identity device: approves a link request and returns the approval
  /// text.
  Future<String> approveLink(String requestText);

  /// Revokes another device of this user (hex id).
  Future<void> revokeDevice(String device);

  /// The newest [limit] messages, oldest first.
  Future<List<ChatMessage>> history(int limit);

  /// One message by local id.
  Future<ChatMessage?> message(int id);

  /// Sends a text message; returns its local id.
  Future<int> sendText(String text);

  /// How to show [user]: "Ali (alice)", or the username alone.
  Future<String> displayName(UserRef user);

  /// [user]'s profile.
  Future<ProfileInfo> profile(UserRef user);

  /// Sets this user's display name and about text.
  Future<int> setProfile({String? displayName, String? about});

  /// The group's member devices.
  Future<List<MemberDevice>> members();

  /// The group's name.
  Future<String?> groupName();

  /// The group's epoch on this device.
  Future<BigInt?> epoch();

  /// Whether this device's user is the admin.
  Future<bool> isAdmin();

  /// Admin: creates the group.
  Future<void> createGroup();

  /// Admin: a new invite for [username].
  Future<InviteInfo> createInvite(String username);

  /// Admin: the unused invites.
  Future<List<InviteInfo>> listInvites();

  /// Admin: revokes an unused invite.
  Future<void> revokeInvite(String code);

  /// Admin: adds all devices of [user] to the group.
  Future<void> addUser(UserRef user);

  /// Admin: removes [user] from the group and the Pi.
  Future<void> removeUser(UserRef user);

  /// Admin: sets the group's name.
  Future<int> setGroupName(String? name);

  /// Admin: accepts [user]'s changed identity key.
  Future<bool> acceptIdentityKey(UserRef user);
}
