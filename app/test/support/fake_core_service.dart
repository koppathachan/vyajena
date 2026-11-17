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

  /// Every core call that acts, by name, in order (reads such as history,
  /// members or names are not recorded).
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

  /// What [epoch] returns: null while this device is not in the group.
  BigInt? epochValue = BigInt.one;

  /// What [displayName] returns, by `user@server`; else the username.
  Map<String, String> displayNames = {};

  /// What [profile] returns, by `user@server`.
  Map<String, ProfileInfo> profiles = {};

  /// Errors a call throws, by call name (as in [calls]).
  Map<String, Object> fails = {};

  /// The texts [sendText] and [sendMessage] stored, in order.
  final List<String> sent = [];

  /// What [unseenMentions] returns; [markMentionSeen] removes from it.
  List<int> unseen = [];

  /// The arguments of the last calls that take one, by call name.
  final Map<String, Object?> args = {};

  /// Runs inside [startCore], as the real core emits catch-up events right
  /// after it starts (before any screen has drawn).
  void Function()? onStart;

  int _nextId = 1;

  /// A new local id, after every id in [messages].
  int _newId() {
    for (final m in messages) {
      if (m.id >= _nextId) _nextId = m.id + 1;
    }
    return _nextId++;
  }

  /// Pushes [event] as if the core emitted it.
  void emit(CoreEvent event) => handleEvent(event);

  void _call(String name, [Object? arg]) {
    calls.add(name);
    args[name] = arg;
    final error = fails[name];
    if (error != null) throw error;
  }

  /// A received message from [sender], appended to [messages] and emitted
  /// as the core would. A message that [mentionsMe] is also listed in
  /// [unseen], as the core stores it.
  ChatMessage receive(
    UserRef sender,
    String text, {
    List<MentionSpan> mentions = const [],
    bool mentionsMe = false,
    ReplyQuote? reply,
    bool emitEvent = true,
  }) {
    final message = ChatMessage(
      id: _newId(),
      sender: sender,
      senderDevice: 'cd34',
      outgoing: false,
      ownUser: false,
      body: ChatBody.text(text: text),
      mentions: mentions,
      mentionsMe: mentionsMe,
      reply: reply,
      acceptedAtMs: 1,
    );
    messages.add(message);
    if (mentionsMe) unseen.add(message.id);
    if (emitEvent) emit(CoreEvent.message(message: message));
    return message;
  }

  /// Sets the status of own message [id] and emits it as the core would.
  void setStatus(int id, SendState status) {
    messages = [
      for (final m in messages)
        if (m.id == id)
          ChatMessage(
            id: m.id,
            sender: m.sender,
            senderDevice: m.senderDevice,
            outgoing: m.outgoing,
            ownUser: m.ownUser,
            body: m.body,
            mentions: m.mentions,
            mentionsMe: m.mentionsMe,
            reply: m.reply,
            sentAtMs: m.sentAtMs,
            acceptedAtMs: status is SendState_Accepted ? 2 : m.acceptedAtMs,
            status: status,
          )
        else
          m,
    ];
    emit(CoreEvent.sendStatus(id: id, status: status));
  }

  @override
  Future<void> openCore() async {
    calls.add('open');
    final error = openFails;
    if (error != null) throw error;
  }

  @override
  Future<void> startCore() async {
    calls.add('start');
    onStart?.call();
  }

  @override
  Future<void> stopCore() async => calls.add('stop');

  @override
  Future<void> reconnectNow() async => calls.add('reconnectNow');

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
    _call('register', inviteText);
    storedUser = newUser;
    await becameUser(newUser);
    return newUser;
  }

  @override
  Future<String> linkRequest() async {
    _call('linkRequest');
    return 'vyajena-link:request';
  }

  @override
  Future<UserRef> completeLink(String approvalText) async {
    _call('completeLink', approvalText);
    storedUser = newUser;
    await becameUser(newUser);
    return newUser;
  }

  @override
  Future<String> approveLink(String requestText) async {
    _call('approveLink', requestText);
    return 'vyajena-approval:approval';
  }

  @override
  Future<void> revokeDevice(String device) async => calls.add('revokeDevice');

  @override
  Future<void> setPushToken(PushPlatform platform, String token) async =>
      _call('setPushToken', (platform, token));

  @override
  Future<void> clearPushToken() async => _call('clearPushToken');

  @override
  Future<List<ChatMessage>> history(int limit) async {
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
    _call('sendText', text);
    return _storeOwn(text, const [], null);
  }

  @override
  Future<int> sendMessage(
    String text, {
    List<MentionSpan> mentions = const [],
    int? replyTo,
  }) async {
    _call('sendMessage', (text: text, mentions: mentions, replyTo: replyTo));
    ReplyQuote? reply;
    if (replyTo != null) {
      final original = await message(replyTo);
      if (original == null) {
        throw const CoreError(
          kind: CoreErrorKind.noSuchMessage,
          message: 'no such message',
        );
      }
      final body = original.body;
      reply = ReplyQuote(
        to: original.sender,
        quote: body is ChatBody_Text ? body.text : '',
        originalId: original.id,
      );
    }
    return _storeOwn(text, mentions, reply);
  }

  int _storeOwn(String text, List<MentionSpan> mentions, ReplyQuote? reply) {
    sent.add(text);
    final id = _newId();
    messages.add(
      ChatMessage(
        id: id,
        sender: storedUser ?? newUser,
        senderDevice: 'ab12',
        outgoing: true,
        ownUser: true,
        body: ChatBody.text(text: text),
        mentions: mentions,
        mentionsMe: false,
        reply: reply,
        sentAtMs: 1,
        status: const SendState.sending(),
      ),
    );
    return id;
  }

  @override
  Future<List<int>> unseenMentions() async => List.of(unseen);

  @override
  Future<bool> markMentionSeen(int id) async {
    _call('markMentionSeen', id);
    return unseen.remove(id);
  }

  static String _key(UserRef user) => '${user.username}@${user.serverId}';

  @override
  Future<String> displayName(UserRef user) async =>
      displayNames[_key(user)] ?? user.username;

  @override
  Future<ProfileInfo> profile(UserRef user) async =>
      profiles[_key(user)] ?? const ProfileInfo();

  @override
  Future<int> setProfile({String? displayName, String? about}) async {
    _call('setProfile', (displayName: displayName, about: about));
    final user = storedUser;
    if (user != null) {
      profiles[_key(user)] = ProfileInfo(
        displayName: displayName,
        about: about,
      );
    }
    return _nextId++;
  }

  @override
  Future<List<MemberDevice>> members() async => memberDevices;

  @override
  Future<String?> groupName() async => group;

  @override
  Future<BigInt?> epoch() async => epochValue;

  @override
  Future<bool> isAdmin() async => admin;

  @override
  Future<void> createGroup() async {
    _call('createGroup');
    epochValue = BigInt.zero;
  }

  @override
  Future<InviteInfo> createInvite(String username) async {
    _call('createInvite', username);
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
  Future<void> revokeInvite(String code) async {
    _call('revokeInvite', code);
    invites = [
      for (final i in invites)
        if (i.code != code) i,
    ];
  }

  @override
  Future<void> addUser(UserRef user) async => calls.add('addUser');

  @override
  Future<void> removeUser(UserRef user) async {
    _call('removeUser', user);
    memberDevices = [
      for (final d in memberDevices)
        if (_key(d.user) != _key(user)) d,
    ];
  }

  @override
  Future<int> setGroupName(String? name) async {
    _call('setGroupName', name);
    group = name;
    return _nextId++;
  }

  @override
  Future<bool> acceptIdentityKey(UserRef user) async {
    _call('acceptIdentityKey', user);
    return true;
  }
}
