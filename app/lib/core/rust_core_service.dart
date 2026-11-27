// CoreService over the real Rust core (flutter_rust_bridge). The only file
// in lib/ besides main.dart that touches the bridge's Core.
import 'dart:async';

import 'package:flutter/services.dart';

import '../src/rust/api/core.dart' as rust;
import '../src/rust/api/text.dart' as rust_text;
import 'core_service.dart';

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
    _subscription = core.events().listen(handleEvent);
  }

  @override
  Future<void> startCore() => _c.start();

  @override
  Future<void> stopCore() => _c.stop();

  @override
  Future<void> deleteAccountCore() => _c.deleteAccount();

  @override
  Future<void> reconnectNow() async {
    await _core?.reconnectNow();
  }

  @override
  Future<void> requestResync() async {
    await _core?.requestResync();
  }

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
  Future<void> setPushToken(PushPlatform platform, String token) =>
      _c.setPushToken(platform: platform, token: token);

  @override
  Future<void> clearPushToken() => _c.clearPushToken();

  @override
  Future<bool> hasUnsent() => _c.hasUnsent();

  @override
  Future<List<ChatMessage>> history(int limit) => _c.history(limit: limit);

  @override
  Future<ChatMessage?> message(int id) => _c.message(id: id);

  @override
  Future<int> sendText(String text) => _c.sendText(text: text);

  @override
  Future<int> sendMessage(
    String text, {
    List<MentionSpan> mentions = const [],
    int? replyTo,
  }) => _c.sendMessage(text: text, mentions: mentions, replyTo: replyTo);

  @override
  Future<List<int>> unseenMentions() async => [
    for (final id in await _c.unseenMentions()) id.toInt(),
  ];

  @override
  Future<bool> markMentionSeen(int id) => _c.markMentionSeen(id: id);

  @override
  Future<int> sendImage(
    Uint8List picked, {
    int chatId = 0,
    String? caption,
    List<MentionSpan> mentions = const [],
  }) async => (await _c.sendImageIn(
    chatId: chatId,
    picked: picked,
    caption: caption,
    mentions: mentions,
  )).toInt();

  @override
  Future<Uint8List> downloadImage(int id) => _c.downloadImage(id: id);

  @override
  Future<Uint8List?> cachedImage(int id) => _c.cachedImage(id: id);

  @override
  Future<int> resendImage(int id) async =>
      (await _c.resendImage(id: id)).toInt();

  @override
  Future<int> sendSticker(
    Uint8List rgba, {
    required StickerSource source,
    required int width,
    required int height,
    int chatId = 0,
  }) async => (await _c.sendStickerIn(
    chatId: chatId,
    source: source,
    rgba: rgba,
    width: width,
    height: height,
  )).toInt();

  @override
  bool thumbhashIsTransparent(Uint8List thumbhash) =>
      rust.thumbhashIsTransparent(thumbhash: thumbhash);

  @override
  Future<int> sendAudio(Uint8List wav, {int chatId = 0, int? replyTo}) async =>
      (await _c.sendAudioIn(
        chatId: chatId,
        wav: wav,
        replyTo: replyTo,
      )).toInt();

  @override
  Future<String> downloadAudio(int id) => _c.downloadAudio(id: id);

  @override
  Future<String?> cachedAudio(int id) => _c.cachedAudio(id: id);

  @override
  Future<int> resendAudio(int id) async =>
      (await _c.resendAudio(id: id)).toInt();

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
    String? caption,
    List<MentionSpan> mentions = const [],
  }) async => (await _c.sendVideoIn(
    chatId: chatId,
    mp4: mp4,
    durationMs: durationMs,
    width: width,
    height: height,
    firstFrameRgba: firstFrameRgba ?? Uint8List(0),
    frameWidth: frameWidth,
    frameHeight: frameHeight,
    caption: caption,
    mentions: mentions,
  )).toInt();

  @override
  Future<String> downloadVideo(int id) => _c.downloadVideo(id: id);

  @override
  Future<String?> cachedVideo(int id) => _c.cachedVideo(id: id);

  @override
  Future<int> resendVideo(int id) async =>
      (await _c.resendVideo(id: id)).toInt();

  @override
  Uint8List? thumbhashPreview(Uint8List thumbhash) =>
      rust.thumbhashPreview(thumbhash: thumbhash);

  @override
  Future<bool> dmsAvailable() => _c.dmsAvailable();

  @override
  Future<List<ChatSummary>> chats() => _c.chats();

  @override
  Future<int> openDm(UserRef user) => _c.openDm(user: user);

  @override
  Future<List<ChatMessage>> historyIn(int chatId, int limit) =>
      _c.historyIn(chatId: chatId, limit: limit);

  @override
  Future<int> sendMessageIn(
    int chatId,
    String text, {
    List<MentionSpan> mentions = const [],
    int? replyTo,
  }) => _c.sendMessageIn(
    chatId: chatId,
    text: text,
    mentions: mentions,
    replyTo: replyTo,
  );

  @override
  Future<int> deleteMessage(int chatId, int id) =>
      _c.deleteMessage(chatId: chatId, id: id);

  @override
  Future<ChatMessage> reveal(int chatId, int id) =>
      _c.reveal(chatId: chatId, id: id);

  @override
  Future<List<int>> unseenMentionsIn(int chatId) async => [
    for (final id in await _c.unseenMentionsIn(chatId: chatId)) id.toInt(),
  ];

  @override
  Future<void> markChatRead(int chatId) => _c.markChatRead(chatId: chatId);

  @override
  Future<int> chatReadMark(int chatId) => _c.chatReadMark(chatId: chatId);

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
  Future<BigInt?> epoch() => _c.epoch();

  @override
  Future<bool> isAdmin() => _c.isAdmin();

  @override
  Future<UserRef?> adminUser() => _c.adminUser();

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
  Future<bool> namedGroupsAvailable() => _c.namedGroupsAvailable();

  @override
  Future<int> createNamedGroup(String? name, List<UserRef> members) =>
      _c.createNamedGroup(name: name, members: members);

  @override
  Future<void> addToGroup(int chatId, UserRef user) =>
      _c.addToGroup(chatId: chatId, user: user);

  @override
  Future<void> removeFromGroup(int chatId, UserRef user) =>
      _c.removeFromGroup(chatId: chatId, user: user);

  @override
  Future<int> renameGroup(int chatId, String name) =>
      _c.renameGroup(chatId: chatId, name: name);

  @override
  Future<void> closeGroup(int chatId) => _c.closeGroup(chatId: chatId);

  @override
  Future<void> removeClosedDm(int chatId) => _c.removeClosedDm(chatId: chatId);

  @override
  Future<void> blockUser(UserRef user) => _c.blockUser(user: user);

  @override
  Future<void> unblockUser(UserRef user) => _c.unblockUser(user: user);

  @override
  Future<bool> isBlocked(UserRef user) => _c.isBlocked(user: user);

  @override
  Future<List<UserRef>> blockedUsers() => _c.blockedUsers();

  @override
  Future<void> reportMessage(int id, String reason) =>
      _c.reportMessage(id: id, reason: reason);

  @override
  Future<void> reportUser(UserRef user, String reason) =>
      _c.reportUser(user: user, reason: reason);

  @override
  Future<ReportView?> parseReport(String text) => _c.parseReport(text: text);

  @override
  Future<void> setConversationMuted(int chatId, bool muted) =>
      _c.setConversationMuted(chatId: chatId, muted: muted);

  @override
  Future<void> setAppMuted(bool muted) => _c.setAppMuted(muted: muted);

  @override
  Future<bool> appMuted() => _c.appMuted();

  @override
  Future<void> setPresence(bool online) => _c.setPresence(online: online);

  @override
  Future<PresenceStatus> loadMyPresence() => _c.myPresence();

  @override
  Future<PresenceStatus> presenceOf(UserRef user) => _c.presenceOf(user: user);

  @override
  Future<List<MemberDevice>> groupMembers(int chatId) =>
      _c.groupMembers(chatId: chatId);

  @override
  Future<bool> acceptIdentityKey(UserRef user) =>
      _c.acceptIdentityKey(user: user);

  @override
  Future<bool> acknowledgeIdentityKey(UserRef user) =>
      _c.acknowledgeIdentityKey(user: user);

  @override
  Future<List<UserRef>> identityRetrustPending() => _c.identityRetrustPending();
}
