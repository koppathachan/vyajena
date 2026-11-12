// A CoreService for widget tests (flutter-app design D7): no Rust, no Pi.
// Tests set its state, push events with [emit] and read [calls].
import 'package:vyajena/core/core_service.dart';

/// An in-memory core.
class FakeCoreService extends CoreService {
  FakeCoreService({this.storedUser, this.openFails});

  /// The user this device already has when the app opens.
  UserRef? storedUser;

  /// Thrown by [openCore], to show the failed state.
  Object? openFails;

  /// The user [register] and [completeLink] return.
  UserRef newUser = const UserRef(username: 'bob', serverId: 'pi');

  /// Every core call, by name, in order.
  final List<String> calls = [];

  /// What [history] returns.
  List<ChatMessage> messages = [];

  /// What [members] returns.
  List<MemberDevice> memberDevices = [];

  /// What [listInvites] returns.
  List<InviteInfo> invites = [];

  /// What [isAdmin] returns.
  bool admin = false;

  /// What [groupName] returns.
  String? group;

  /// The limit [textFits] applies, in UTF-16 code units.
  int maxTextLength = 1000;

  int _nextId = 1;

  /// Pushes [event] as if the core emitted it.
  void emit(CoreEvent event) => handleEvent(event);

  @override
  Future<void> openCore() async {
    calls.add('open');
    final error = openFails;
    if (error != null) throw error;
  }

  @override
  Future<void> startCore() async => calls.add('start');

  @override
  Future<void> stopCore() async => calls.add('stop');

  @override
  Future<UserRef?> userId() async => storedUser;

  @override
  Future<String?> deviceId() async => storedUser == null ? null : 'ab12';

  @override
  InvitePreview parseInvite(String text) {
    calls.add('parseInvite');
    if (!text.startsWith('vyajena-invite:')) {
      throw const CoreError(
        kind: CoreErrorKind.invalidPayload,
        message: 'not an invite',
      );
    }
    return InvitePreview(user: newUser, relayAddress: '127.0.0.1:8443');
  }

  @override
  bool textFits(String text) => text.length <= maxTextLength;

  @override
  Future<UserRef> register(String inviteText) async {
    calls.add('register');
    storedUser = newUser;
    await becameUser(newUser);
    return newUser;
  }

  @override
  Future<String> linkRequest() async {
    calls.add('linkRequest');
    return 'vyajena-link:request';
  }

  @override
  Future<UserRef> completeLink(String approvalText) async {
    calls.add('completeLink');
    storedUser = newUser;
    await becameUser(newUser);
    return newUser;
  }

  @override
  Future<String> approveLink(String requestText) async {
    calls.add('approveLink');
    return 'vyajena-approval:approval';
  }

  @override
  Future<void> revokeDevice(String device) async => calls.add('revokeDevice');

  @override
  Future<List<ChatMessage>> history(int limit) async {
    calls.add('history');
    final start = messages.length > limit ? messages.length - limit : 0;
    return messages.sublist(start);
  }

  @override
  Future<ChatMessage?> message(int id) async {
    for (final message in messages) {
      if (message.id == id) return message;
    }
    return null;
  }

  @override
  Future<int> sendText(String text) async {
    calls.add('sendText');
    return _nextId++;
  }

  @override
  Future<String> displayName(UserRef user) async => user.username;

  @override
  Future<ProfileInfo> profile(UserRef user) async => const ProfileInfo();

  @override
  Future<int> setProfile({String? displayName, String? about}) async {
    calls.add('setProfile');
    return _nextId++;
  }

  @override
  Future<List<MemberDevice>> members() async => memberDevices;

  @override
  Future<String?> groupName() async => group;

  @override
  Future<BigInt?> epoch() async => BigInt.one;

  @override
  Future<bool> isAdmin() async => admin;

  @override
  Future<void> createGroup() async => calls.add('createGroup');

  @override
  Future<InviteInfo> createInvite(String username) async {
    calls.add('createInvite');
    return InviteInfo(
      code: 'code-$username',
      user: UserRef(username: username, serverId: 'pi'),
      expiresAtMs: 0,
      text: 'vyajena-invite:$username',
    );
  }

  @override
  Future<List<InviteInfo>> listInvites() async => invites;

  @override
  Future<void> revokeInvite(String code) async => calls.add('revokeInvite');

  @override
  Future<void> addUser(UserRef user) async => calls.add('addUser');

  @override
  Future<void> removeUser(UserRef user) async => calls.add('removeUser');

  @override
  Future<int> setGroupName(String? name) async {
    calls.add('setGroupName');
    group = name;
    return _nextId++;
  }

  @override
  Future<bool> acceptIdentityKey(UserRef user) async {
    calls.add('acceptIdentityKey');
    return true;
  }
}
