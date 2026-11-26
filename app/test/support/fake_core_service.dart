// A CoreService for widget tests (flutter-app design D7): no Rust, no server.
// Tests set its state, push events with [emit] and read [calls].
import 'dart:convert';
import 'dart:typed_data';

import 'package:vyajena/core/core_service.dart';

/// An in-memory core.
class FakeCoreService extends CoreService {
  FakeCoreService({this.storedUser, this.openFails});

  /// The user this device already has when the app opens.
  UserRef? storedUser;

  /// Thrown by [openCore], to show the failed state.
  Object? openFails;

  /// The user [register] and [completeLink] return.
  UserRef newUser = const UserRef(username: 'bob', serverId: 'chat');

  /// dev-onboarding: what [serverOnboarding] reports (default: invite-only).
  ServerOnboarding onboarding = const ServerOnboarding(
    serverId: 'chat',
    open: false,
    joinCodeRequired: false,
    maxUsers: 0,
    adminUsername: 'akhil',
  );

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

  /// A 1x1 PNG: what [thumbhashPreview] returns, so the blur is a real image,
  /// and the picture the fake's server holds.
  static final Uint8List blurPng = base64Decode(
    'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==',
  );

  /// The pictures this device has cached, by message id ([cachedImage]).
  final Map<int, Uint8List> cachedPictures = {};

  /// The pictures the server still has, by message id: what [downloadImage]
  /// downloads. A message not here has expired.
  final Map<int, Uint8List> piPictures = {};

  /// The pictures [sendImage] was given, in order.
  final List<Uint8List> sentPictures = [];

  /// The clips this device has cached, by message id: the path
  /// [cachedAudio] returns.
  final Map<int, String> cachedClips = {};

  /// The clips the server still has, by message id: what [downloadAudio]
  /// downloads. A message not here has expired.
  final Map<int, String> serverClips = {};

  /// The recordings [sendAudio] was given, in order.
  final List<Uint8List> sentRecordings = [];

  /// The clips this device has cached, by message id: the path [cachedVideo]
  /// returns.
  final Map<int, String> cachedVideoClips = {};

  /// The video clips the server still has, by message id: what [downloadVideo]
  /// downloads. A message not here has expired.
  final Map<int, String> serverVideoClips = {};

  /// The normalized clips [sendVideo] was given, in order.
  final List<Uint8List> sentVideos = [];

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

  /// A received picture from [sender], with an optional [caption]. The server
  /// still has it ([piPictures]) unless [expired]; it is not cached yet.
  ChatMessage receiveImage(
    UserRef sender, {
    String? caption,
    List<MentionSpan> mentions = const [],
    bool expired = false,
    int width = 640,
    int height = 480,
    int chatId = 0,
  }) {
    final id = _newId();
    final message = ChatMessage(
      id: id,
      chatId: chatId,
      sender: sender,
      senderDevice: 'cd34',
      outgoing: false,
      ownUser: false,
      body: ChatBody.image(
        image: ImageView(
          width: width,
          height: height,
          thumbhash: Uint8List.fromList([1, 2, 3]),
          size: BigInt.from(1200 * 1024),
          caption: caption,
          state: expired ? MediaStatus.expired : MediaStatus.notDownloaded,
        ),
      ),
      mentions: mentions,
      mentionsMe: false,
      acceptedAtMs: 1,
    );
    messages.add(message);
    if (!expired) piPictures[id] = blurPng;
    emit(CoreEvent.message(message: message));
    return message;
  }

  /// A received voice message from [sender]. The server still has its clip
  /// ([serverClips]) unless [expired]; it is not cached yet.
  ChatMessage receiveAudio(
    UserRef sender, {
    bool expired = false,
    int durationMs = 7000,
    int chatId = 0,
  }) {
    final id = _newId();
    final message = ChatMessage(
      id: id,
      chatId: chatId,
      sender: sender,
      senderDevice: 'cd34',
      outgoing: false,
      ownUser: false,
      body: ChatBody.audio(
        audio: AudioView(
          durationMs: durationMs,
          waveform: Uint8List.fromList([for (var i = 0; i < 64; i++) i * 4]),
          size: BigInt.from(21 * 1024),
          state: expired ? MediaStatus.expired : MediaStatus.notDownloaded,
        ),
      ),
      mentions: const [],
      mentionsMe: false,
      acceptedAtMs: 1,
    );
    messages.add(message);
    if (!expired) serverClips[id] = '/media/clip-$id.opus';
    emit(CoreEvent.message(message: message));
    return message;
  }

  /// A received video from [sender]. The server still has its clip
  /// ([serverVideoClips]) unless [expired]; it is not cached yet.
  ChatMessage receiveVideo(
    UserRef sender, {
    bool expired = false,
    int durationMs = 12000,
    int width = 854,
    int height = 480,
    int chatId = 0,
  }) {
    final id = _newId();
    final message = ChatMessage(
      id: id,
      chatId: chatId,
      sender: sender,
      senderDevice: 'cd34',
      outgoing: false,
      ownUser: false,
      body: ChatBody.video(
        video: VideoView(
          width: width,
          height: height,
          durationMs: durationMs,
          thumbhash: Uint8List.fromList([1, 2, 3]),
          size: BigInt.from(4 * 1024 * 1024),
          state: expired ? MediaStatus.expired : MediaStatus.notDownloaded,
        ),
      ),
      mentions: const [],
      mentionsMe: false,
      acceptedAtMs: 1,
    );
    messages.add(message);
    if (!expired) serverVideoClips[id] = '/media/clip-$id.mp4';
    emit(CoreEvent.message(message: message));
    return message;
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
    int chatId = 0,
  }) {
    final message = ChatMessage(
      id: _newId(),
      chatId: chatId,
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
            chatId: m.chatId,
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

  /// Moves the picture of image message [id] to [state] ([MediaStatus]) and
  /// emits it as the core's status change does; the next history read shows it.
  void setMediaState(int id, MediaStatus state) {
    messages = [
      for (final m in messages)
        if (m.id == id && m.body is ChatBody_Image)
          ChatMessage(
            id: m.id,
            chatId: m.chatId,
            sender: m.sender,
            senderDevice: m.senderDevice,
            outgoing: m.outgoing,
            ownUser: m.ownUser,
            body: ChatBody.image(
              image: ImageView(
                width: (m.body as ChatBody_Image).image.width,
                height: (m.body as ChatBody_Image).image.height,
                thumbhash: (m.body as ChatBody_Image).image.thumbhash,
                size: (m.body as ChatBody_Image).image.size,
                caption: (m.body as ChatBody_Image).image.caption,
                state: state,
              ),
            ),
            mentions: m.mentions,
            mentionsMe: m.mentionsMe,
            reply: m.reply,
            sentAtMs: m.sentAtMs,
            acceptedAtMs: m.acceptedAtMs,
            status: m.status,
          )
        else
          m,
    ];
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
  Future<void> deleteAccountCore() async => _call('deleteAccount');

  @override
  Future<void> reconnectNow() async => calls.add('reconnectNow');

  @override
  Future<void> requestResync() async => calls.add('requestResync');

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
  Future<ServerOnboarding> serverOnboarding(String handleText) async {
    calls.add('serverOnboarding');
    return onboarding;
  }

  @override
  Future<UserRef> registerOpen(
    String handleText,
    String username,
    String joinCode,
  ) async {
    _call('registerOpen', '$username/$joinCode');
    final user = UserRef(username: username, serverId: onboarding.serverId);
    storedUser = user;
    await becameUser(user);
    return user;
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

  /// What [hasUnsent] answers.
  bool unsent = false;

  @override
  Future<bool> hasUnsent() async => unsent;

  @override
  Future<List<ChatMessage>> history(int limit) => historyIn(0, limit);

  @override
  Future<List<ChatMessage>> historyIn(int chatId, int limit) async {
    final inChat = [
      for (final m in messages)
        // moderation: A blocked user is hidden — a blocked sender's messages are
        // omitted from the displayed history (own messages are never hidden).
        if (m.chatId == chatId && (m.ownUser || !blocked.containsKey(_key(m.sender))))
          m,
    ];
    final start = inChat.length > limit ? inChat.length - limit : 0;
    return inChat.sublist(start);
  }

  @override
  Future<ChatMessage?> message(int id) async {
    for (final message in messages) {
      if (message.id == id) return message;
    }
    return null;
  }

  /// Whether a [deleteMessage] marks the tombstone "by the admin"; tests set it
  /// for an admin deleting another user's message in a group.
  bool deleteByAdmin = false;

  /// Originals kept so a DM [reveal] can return them.
  final Map<int, ChatMessage> revealable = {};

  @override
  Future<int> deleteMessage(int chatId, int id) async {
    _call('deleteMessage', (chatId: chatId, id: id));
    final index = messages.indexWhere((m) => m.id == id);
    if (index == -1) {
      throw const CoreError(
        kind: CoreErrorKind.noSuchMessage,
        message: 'no such message',
      );
    }
    final original = messages[index];
    // A DM (a non-main chat in the tests) keeps the original for Reveal.
    final isDm = chatId != 0;
    if (isDm) revealable[id] = original;
    messages[index] = ChatMessage(
      id: original.id,
      chatId: original.chatId,
      sender: original.sender,
      senderDevice: original.senderDevice,
      outgoing: original.outgoing,
      ownUser: original.ownUser,
      body: ChatBody.deleted(byAdmin: deleteByAdmin, revealable: isDm),
      mentions: const [],
      mentionsMe: false,
      reply: null,
      sentAtMs: original.sentAtMs,
      acceptedAtMs: original.acceptedAtMs,
      status: original.status,
    );
    emit(CoreEvent.message(message: messages[index]));
    return _newId();
  }

  @override
  Future<ChatMessage> reveal(int chatId, int id) async {
    _call('reveal', (chatId: chatId, id: id));
    final original = revealable[id];
    if (original == null) {
      throw const CoreError(
        kind: CoreErrorKind.noSuchMessage,
        message: 'no such message',
      );
    }
    return original;
  }

  @override
  Future<int> sendText(String text) async {
    _call('sendText', text);
    return _storeOwn(text, const [], null, 0);
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
        kind: switch (body) {
          ChatBody_Image() => ReplyKind.image,
          ChatBody_Audio() => ReplyKind.audio,
          _ => ReplyKind.text,
        },
        durationMs: body is ChatBody_Audio ? body.audio.durationMs : 0,
        thumbhash: body is ChatBody_Image ? body.image.thumbhash : Uint8List(0),
        originalId: original.id,
        originalDeleted: body is ChatBody_Deleted,
      );
    }
    return _storeOwn(text, mentions, reply, 0);
  }

  @override
  Future<int> sendMessageIn(
    int chatId,
    String text, {
    List<MentionSpan> mentions = const [],
    int? replyTo,
  }) async {
    _call('sendMessageIn', (chatId: chatId, text: text, mentions: mentions));
    return _storeOwn(text, mentions, null, chatId);
  }

  int _storeOwn(
    String text,
    List<MentionSpan> mentions,
    ReplyQuote? reply,
    int chatId,
  ) {
    sent.add(text);
    final id = _newId();
    messages.add(
      ChatMessage(
        id: id,
        chatId: chatId,
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
  Future<int> sendImage(
    Uint8List picked, {
    int chatId = 0,
    String? caption,
    List<MentionSpan> mentions = const [],
  }) async {
    _call('sendImage', (chatId: chatId, caption: caption, mentions: mentions));
    sentPictures.add(picked);
    final id = _newId();
    cachedPictures[id] = picked;
    messages.add(
      ChatMessage(
        id: id,
        chatId: chatId,
        sender: storedUser ?? newUser,
        senderDevice: 'ab12',
        outgoing: true,
        ownUser: true,
        body: ChatBody.image(
          image: ImageView(
            width: 640,
            height: 480,
            thumbhash: Uint8List.fromList([1, 2, 3]),
            size: BigInt.from(1200 * 1024),
            caption: caption,
            state: MediaStatus.available,
          ),
        ),
        mentions: mentions,
        mentionsMe: false,
        sentAtMs: 1,
        status: const SendState.sending(),
      ),
    );
    return id;
  }

  @override
  Future<Uint8List> downloadImage(int id) async {
    _call('downloadImage', id);
    final cached = cachedPictures[id];
    if (cached != null) return cached;
    final picture = piPictures[id];
    if (picture == null) {
      throw const CoreError(
        kind: CoreErrorKind.mediaExpired,
        message: 'the picture has expired',
      );
    }
    cachedPictures[id] = picture;
    return picture;
  }

  @override
  Future<Uint8List?> cachedImage(int id) async => cachedPictures[id];

  @override
  Future<int> resendImage(int id) async {
    _call('resendImage', id);
    final picture = cachedPictures[id];
    if (picture == null) {
      throw const CoreError(
        kind: CoreErrorKind.mediaNotOnDevice,
        message: 'not on this device',
      );
    }
    return sendImage(picture);
  }

  /// The ThumbHash the fake marks as transparent, so
  /// [thumbhashIsTransparent] reports a sticker.
  static final Uint8List stickerThumbhash = Uint8List.fromList([7, 7, 7]);

  /// The cutouts [sendSticker] was given, in order.
  final List<({StickerSource source, int width, int height, Uint8List rgba})>
  sentStickers = [];

  @override
  Future<int> sendSticker(
    Uint8List rgba, {
    required StickerSource source,
    required int width,
    required int height,
    int chatId = 0,
  }) async {
    _call('sendSticker', (chatId: chatId, source: source, w: width, h: height));
    sentStickers.add((
      source: source,
      width: width,
      height: height,
      rgba: rgba,
    ));
    final id = _newId();
    cachedPictures[id] = rgba;
    messages.add(
      ChatMessage(
        id: id,
        chatId: chatId,
        sender: storedUser ?? newUser,
        senderDevice: 'ab12',
        outgoing: true,
        ownUser: true,
        body: ChatBody.image(
          image: ImageView(
            width: width,
            height: height,
            thumbhash: stickerThumbhash,
            size: BigInt.from(40 * 1024),
            state: MediaStatus.available,
          ),
        ),
        mentions: const [],
        mentionsMe: false,
        sentAtMs: 1,
        status: const SendState.sending(),
      ),
    );
    return id;
  }

  /// A received sticker (a transparent image) from [sender].
  ChatMessage receiveSticker(UserRef sender, {int chatId = 0}) {
    final id = _newId();
    final message = ChatMessage(
      id: id,
      chatId: chatId,
      sender: sender,
      senderDevice: 'cd34',
      outgoing: false,
      ownUser: false,
      body: ChatBody.image(
        image: ImageView(
          width: 240,
          height: 240,
          thumbhash: stickerThumbhash,
          size: BigInt.from(40 * 1024),
          state: MediaStatus.notDownloaded,
        ),
      ),
      mentions: const [],
      mentionsMe: false,
      acceptedAtMs: 1,
    );
    messages.add(message);
    piPictures[id] = blurPng;
    emit(CoreEvent.message(message: message));
    return message;
  }

  @override
  bool thumbhashIsTransparent(Uint8List thumbhash) =>
      thumbhash.length == stickerThumbhash.length &&
      thumbhash.every((b) => b == stickerThumbhash.first);

  @override
  Future<int> sendAudio(Uint8List wav, {int chatId = 0, int? replyTo}) async {
    _call('sendAudio', chatId);
    sentRecordings.add(wav);
    final id = _newId();
    cachedClips[id] = '/media/clip-$id.opus';
    ReplyQuote? reply;
    if (replyTo != null) {
      ChatMessage? original;
      for (final m in messages) {
        if (m.id == replyTo) {
          original = m;
          break;
        }
      }
      if (original != null) {
        final body = original.body;
        reply = ReplyQuote(
          to: original.sender,
          quote: body is ChatBody_Text ? body.text : '',
          kind: switch (body) {
            ChatBody_Image() => ReplyKind.image,
            ChatBody_Audio() => ReplyKind.audio,
            _ => ReplyKind.text,
          },
          durationMs: body is ChatBody_Audio ? body.audio.durationMs : 0,
          thumbhash: body is ChatBody_Image
              ? body.image.thumbhash
              : Uint8List(0),
          originalId: original.id,
          originalDeleted: body is ChatBody_Deleted,
        );
      }
    }
    messages.add(
      ChatMessage(
        id: id,
        chatId: chatId,
        sender: storedUser ?? newUser,
        senderDevice: 'ab12',
        outgoing: true,
        ownUser: true,
        body: ChatBody.audio(
          audio: AudioView(
            durationMs: 3000,
            waveform: Uint8List.fromList([10, 80, 255, 120]),
            size: BigInt.from(9 * 1024),
            state: MediaStatus.available,
          ),
        ),
        mentions: const [],
        mentionsMe: false,
        reply: reply,
        sentAtMs: 1,
        status: const SendState.sending(),
      ),
    );
    return id;
  }

  @override
  Future<String> downloadAudio(int id) async {
    _call('downloadAudio', id);
    final cached = cachedClips[id];
    if (cached != null) return cached;
    final clip = serverClips[id];
    if (clip == null) {
      throw const CoreError(
        kind: CoreErrorKind.mediaExpired,
        message: 'the voice message has expired',
      );
    }
    cachedClips[id] = clip;
    return clip;
  }

  @override
  Future<String?> cachedAudio(int id) async => cachedClips[id];

  @override
  Future<int> resendAudio(int id) async {
    _call('resendAudio', id);
    if (cachedClips[id] == null) {
      throw const CoreError(
        kind: CoreErrorKind.mediaNotOnDevice,
        message: 'not on this device',
      );
    }
    return sendAudio(Uint8List(0));
  }

  @override
  Future<int> sendVideo(
    Uint8List mp4, {
    int chatId = 0,
    required int durationMs,
    required int width,
    required int height,
    Uint8List? firstFrameRgba,
    int frameWidth = 0,
    int frameHeight = 0,
  }) async {
    _call('sendVideo', (chatId: chatId, durationMs: durationMs));
    sentVideos.add(mp4);
    final id = _newId();
    cachedVideoClips[id] = '/media/clip-$id.mp4';
    messages.add(
      ChatMessage(
        id: id,
        chatId: chatId,
        sender: storedUser ?? newUser,
        senderDevice: 'ab12',
        outgoing: true,
        ownUser: true,
        body: ChatBody.video(
          video: VideoView(
            width: width,
            height: height,
            durationMs: durationMs,
            thumbhash: Uint8List.fromList([1, 2, 3]),
            size: BigInt.from(mp4.length),
            state: MediaStatus.available,
          ),
        ),
        mentions: const [],
        mentionsMe: false,
        sentAtMs: 1,
        status: const SendState.sending(),
      ),
    );
    return id;
  }

  @override
  Future<String> downloadVideo(int id) async {
    _call('downloadVideo', id);
    final cached = cachedVideoClips[id];
    if (cached != null) return cached;
    final clip = serverVideoClips[id];
    if (clip == null) {
      throw const CoreError(
        kind: CoreErrorKind.mediaExpired,
        message: 'the video has expired',
      );
    }
    cachedVideoClips[id] = clip;
    return clip;
  }

  @override
  Future<String?> cachedVideo(int id) async => cachedVideoClips[id];

  @override
  Future<int> resendVideo(int id) async {
    _call('resendVideo', id);
    if (cachedVideoClips[id] == null) {
      throw const CoreError(
        kind: CoreErrorKind.mediaNotOnDevice,
        message: 'not on this device',
      );
    }
    return sendVideo(
      Uint8List(0),
      durationMs: 12000,
      width: 854,
      height: 480,
    );
  }

  @override
  Uint8List? thumbhashPreview(Uint8List thumbhash) => blurPng;

  @override
  Future<List<int>> unseenMentions() => unseenMentionsIn(0);

  @override
  Future<List<int>> unseenMentionsIn(int chatId) async => [
    for (final m in messages)
      if (m.chatId == chatId && unseen.contains(m.id)) m.id,
  ];

  /// What [dmsAvailable] returns.
  bool dms = true;

  /// The DMs [chats] lists after the group, as the core lists them.
  List<ChatSummary> dmChats = [];

  @override
  Future<bool> dmsAvailable() async => dms;

  /// The conversations muted individually (notification-prefs: Mute a
  /// conversation). Chat id 0 is the lobby.
  final Set<int> mutedChats = {};

  /// Whether whole-app mute is on (notification-prefs: Mute the whole app).
  bool appMutedValue = false;

  /// `summary` with its `muted` flag taken from [mutedChats].
  ChatSummary _withMuteState(ChatSummary summary) => ChatSummary(
    chatId: summary.chatId,
    kind: summary.kind,
    unread: summary.unread,
    unseenMentions: summary.unseenMentions,
    lastMessageAtMs: summary.lastMessageAtMs,
    muted: mutedChats.contains(summary.chatId),
  );

  @override
  Future<List<ChatSummary>> chats() async => [
    if (epochValue != null)
      ChatSummary(
        chatId: 0,
        kind: const ChatKindInfo.lobby(),
        unread: 0,
        unseenMentions: (await unseenMentionsIn(0)).length,
        muted: mutedChats.contains(0),
      ),
    for (final c in groupChats) _withMuteState(c),
    // moderation: A blocked user is hidden — a DM with a blocked peer is hidden
    // from the chat list; it reappears on unblock.
    for (final c in dmChats)
      if (!_peerBlocked(c)) _withMuteState(c),
  ];

  /// Whether chat summary [c] is a DM with a blocked peer.
  bool _peerBlocked(ChatSummary c) => switch (c.kind) {
    ChatKindInfo_Dm(:final peer) => blocked.containsKey(_key(peer)),
    _ => false,
  };

  /// What [setConversationMuted] and [setAppMuted] throw, to test a failure.
  Object? setMuteError;

  @override
  Future<void> setConversationMuted(int chatId, bool muted) async {
    _call('setConversationMuted', [chatId, muted]);
    final error = setMuteError;
    if (error != null) throw error;
    if (muted) {
      mutedChats.add(chatId);
    } else {
      mutedChats.remove(chatId);
    }
    emit(const CoreEvent.mutePrefsChanged());
  }

  @override
  Future<void> setAppMuted(bool muted) async {
    _call('setAppMuted', muted);
    final error = setMuteError;
    if (error != null) throw error;
    appMutedValue = muted;
    emit(const CoreEvent.mutePrefsChanged());
  }

  @override
  Future<bool> appMuted() async => appMutedValue;

  // --- Moderation (block + report; change `report-and-block`) ---

  /// The users blocked on this device, keyed by `username@serverId`
  /// (moderation: Block a user).
  final Map<String, UserRef> blocked = {};

  /// The last report sent via [reportMessage] / [reportUser], for tests.
  ReportView? lastReport;

  @override
  Future<void> blockUser(UserRef user) async {
    _call('blockUser', user);
    blocked[_key(user)] = user;
  }

  @override
  Future<void> unblockUser(UserRef user) async {
    _call('unblockUser', user);
    blocked.remove(_key(user));
  }

  @override
  Future<bool> isBlocked(UserRef user) async => blocked.containsKey(_key(user));

  @override
  Future<List<UserRef>> blockedUsers() async => blocked.values.toList();

  @override
  Future<void> reportMessage(int id, String reason) async {
    _call('reportMessage', (id: id, reason: reason));
    final message = messages.firstWhere((m) => m.id.toInt() == id);
    final isDm =
        message.chatId.toInt() != 0 &&
        dmChats.any((c) => c.chatId == message.chatId.toInt());
    lastReport = ReportView(
      reported: message.sender,
      reason: reason,
      kind: isDm
          ? ReportKindInfo.dm(
              quote: switch (message.body) {
                ChatBody_Text(:final text) => text,
                _ => '',
              },
            )
          : const ReportKindInfo.group(groupId: '', msgId: ''),
    );
  }

  @override
  Future<void> reportUser(UserRef user, String reason) async {
    _call('reportUser', (user: user, reason: reason));
    lastReport = ReportView(
      reported: user,
      reason: reason,
      kind: const ReportKindInfo.user(),
    );
  }

  @override
  Future<ReportView?> parseReport(String text) async {
    final lines = text.split('\n');
    if (lines.isEmpty || lines.first.trim() != '[vyajena-report v1]') return null;
    String? reportedText;
    var reason = '';
    String? kindTag;
    var groupId = '';
    var msgId = '';
    var quote = '';
    for (final line in lines.skip(1)) {
      final i = line.indexOf(':');
      if (i < 0) continue;
      final key = line.substring(0, i).trim();
      final value = line.substring(i + 1).trim();
      switch (key) {
        case 'reported':
          reportedText = value;
        case 'reason':
          reason = value;
        case 'kind':
          kindTag = value;
        case 'group':
          groupId = value;
        case 'msg':
          msgId = value;
        case 'quote':
          quote = value;
      }
    }
    final at = reportedText?.indexOf('@') ?? -1;
    if (reportedText == null || at <= 0 || at == reportedText.length - 1) {
      return null;
    }
    final reported = UserRef(
      username: reportedText.substring(0, at),
      serverId: reportedText.substring(at + 1),
    );
    final kind = switch (kindTag) {
      'group' => ReportKindInfo.group(groupId: groupId, msgId: msgId),
      'dm' => ReportKindInfo.dm(quote: quote),
      'user' => const ReportKindInfo.user(),
      _ => null,
    };
    if (kind == null) return null;
    return ReportView(reported: reported, reason: reason, kind: kind);
  }

  // --- Presence (manual-presence; change `manual-presence`) ---

  /// This user's own manual flag (manual-presence). Offline by default.
  PresenceStatus selfPresence = PresenceStatus.offline;

  /// The flag this device "knows" for each peer, keyed by `username@serverId`.
  /// A user not present here is [PresenceStatus.unknown] (no dot).
  final Map<String, PresenceStatus> peerPresence = {};

  static String _presenceKey(UserRef user) => '${user.username}@${user.serverId}';

  @override
  Future<void> setPresence(bool online) async {
    _call('setPresence', online);
    selfPresence = online ? PresenceStatus.online : PresenceStatus.offline;
    emit(const CoreEvent.presenceChanged(user: null));
  }

  @override
  Future<PresenceStatus> loadMyPresence() async => selfPresence;

  @override
  Future<PresenceStatus> presenceOf(UserRef user) async =>
      peerPresence[_presenceKey(user)] ?? PresenceStatus.unknown;

  /// Test helper: set a peer's flag and emit the matching event.
  void setPeerPresence(UserRef user, PresenceStatus status) {
    peerPresence[_presenceKey(user)] = status;
    emit(CoreEvent.presenceChanged(user: user));
  }

  /// What [namedGroupsAvailable] returns.
  bool namedGroups = true;

  /// The named groups [chats] lists after the lobby, as the core lists them.
  List<ChatSummary> groupChats = [];

  /// What [groupMembers] returns, by chat id.
  Map<int, List<MemberDevice>> groupMemberDevices = {};

  @override
  Future<bool> namedGroupsAvailable() async => namedGroups;

  @override
  Future<int> createNamedGroup(String? name, List<UserRef> members) async {
    _call('createNamedGroup', [name, members]);
    final chatId = 100 + groupChats.length;
    groupChats.add(
      ChatSummary(
        chatId: chatId,
        kind: ChatKindInfo.group(name: name, closed: false),
        unread: 0,
        unseenMentions: 0,
        muted: false,
      ),
    );
    emit(const CoreEvent.chatsChanged());
    return chatId;
  }

  @override
  Future<void> addToGroup(int chatId, UserRef user) async =>
      _call('addToGroup', [chatId, user]);

  @override
  Future<void> removeFromGroup(int chatId, UserRef user) async {
    _call('removeFromGroup', [chatId, user]);
    groupMemberDevices[chatId] = [
      for (final d in groupMemberDevices[chatId] ?? const <MemberDevice>[])
        if (_key(d.user) != _key(user)) d,
    ];
  }

  @override
  Future<int> renameGroup(int chatId, String name) async {
    _call('renameGroup', [chatId, name]);
    groupChats = [
      for (final c in groupChats)
        if (c.chatId == chatId)
          ChatSummary(
            chatId: c.chatId,
            kind: ChatKindInfo.group(
              name: name,
              closed: (c.kind as ChatKindInfo_Group).closed,
            ),
            unread: c.unread,
            unseenMentions: c.unseenMentions,
            muted: c.muted,
          )
        else
          c,
    ];
    emit(const CoreEvent.chatsChanged());
    return _nextId++;
  }

  /// What `removeClosedDm` throws, to test a failed removal.
  Object? removeClosedDmError;

  @override
  Future<void> removeClosedDm(int chatId) async {
    _call('removeClosedDm', chatId);
    final error = removeClosedDmError;
    if (error != null) throw error;
    dmChats = [
      for (final c in dmChats)
        if (c.chatId != chatId) c,
    ];
    emit(const CoreEvent.chatsChanged());
  }

  @override
  Future<void> closeGroup(int chatId) async {
    _call('closeGroup', chatId);
    groupChats = [
      for (final c in groupChats)
        if (c.chatId == chatId)
          ChatSummary(
            chatId: c.chatId,
            kind: ChatKindInfo.group(
              name: (c.kind as ChatKindInfo_Group).name,
              closed: true,
            ),
            unread: c.unread,
            unseenMentions: c.unseenMentions,
            muted: c.muted,
          )
        else
          c,
    ];
    emit(const CoreEvent.chatsChanged());
  }

  @override
  Future<List<MemberDevice>> groupMembers(int chatId) async =>
      groupMemberDevices[chatId] ?? const [];

  @override
  Future<int> openDm(UserRef user) async {
    _call('openDm', user);
    for (final c in dmChats) {
      if (c.kind case ChatKindInfo_Dm(
        :final peer,
      ) when peer.username == user.username && peer.serverId == user.serverId) {
        return c.chatId;
      }
    }
    final chatId = dmChats.length + 1;
    dmChats.add(
      ChatSummary(
        chatId: chatId,
        kind: ChatKindInfo.dm(peer: user, waiting: true, closed: false),
        unread: 0,
        unseenMentions: 0,
        muted: false,
      ),
    );
    emit(const CoreEvent.chatsChanged());
    return chatId;
  }

  /// The read mark per chat, as `mark_chat_read` keeps it: the newest
  /// message id read in that chat. Tests preset it to open a chat with
  /// earlier messages already read.
  final Map<int, int> readMarks = {};

  @override
  Future<void> markChatRead(int chatId) async {
    _call('markChatRead', chatId);
    final inChat = [
      for (final m in messages)
        if (m.chatId == chatId) m.id,
    ];
    if (inChat.isNotEmpty) {
      readMarks[chatId] = inChat.reduce((a, b) => a > b ? a : b);
    }
  }

  @override
  Future<int> chatReadMark(int chatId) async => readMarks[chatId] ?? 0;

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
  Future<BigInt?> epoch() async => epochValue;

  @override
  Future<bool> isAdmin() async => admin;

  /// What [adminUser] returns; when null, this user if [admin], else nobody.
  UserRef? adminUserRef;

  @override
  Future<UserRef?> adminUser() async =>
      adminUserRef ?? (admin ? user.value : null);

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
      user: UserRef(username: username, serverId: 'chat'),
      // A week out, as the server mints (invite_ttl, 7 days by default), so the
      // screen can show when it expires.
      expiresAtMs: DateTime.now()
          .add(const Duration(days: 7))
          .millisecondsSinceEpoch,
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
  Future<bool> acceptIdentityKey(UserRef user) async {
    _call('acceptIdentityKey', user);
    return true;
  }

  /// Users the fake reports as awaiting re-trust (admin-recovery).
  List<UserRef> retrustPending = [];

  @override
  Future<bool> acknowledgeIdentityKey(UserRef user) async {
    _call('acknowledgeIdentityKey', user);
    retrustPending = [
      for (final u in retrustPending)
        if (_key(u) != _key(user)) u,
    ];
    return true;
  }

  @override
  Future<List<UserRef>> identityRetrustPending() async {
    _call('identityRetrustPending');
    return retrustPending;
  }
}
