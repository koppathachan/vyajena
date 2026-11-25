// The one Dart door to the Rust core (flutter-app design D1, D3; app-ui: UI
// goes through the core). Screens take a CoreService and never import the
// bridge's `Core` or `frb_generated.dart`; they may use the bridge's plain
// types (`src/rust/api/types.dart`, `error.dart`). RustCoreService is the
// real one; widget tests use a fake (test/support/fake_core_service.dart).
//
// Decrypted text passes through here only to be shown: nothing in this
// class writes it to a file or a log. The app's one debug log line is
// here, in [CoreService.handleEvent], and names the event's kind only
// (app-ui: UI goes through the core, "Nothing logged").
import 'dart:async';

import 'package:flutter/foundation.dart';

import '../src/rust/api/types.dart';
import 'event_label.dart';

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
  final ValueNotifier<bool> _removed = ValueNotifier(false);
  final StreamController<CoreEvent> _events =
      StreamController<CoreEvent>.broadcast();
  final List<Future<void> Function()> _beforeStart = [];
  Object? _openError;
  bool _running = false;

  /// Opening, unregistered, ready or failed.
  ValueListenable<CoreState> get state => _state;

  /// Connected, connecting or disconnected (app-ui: Foreground and
  /// background).
  ValueListenable<ConnectionStatus> get connection => _connection;

  /// This device's user, once registered or linked.
  ValueListenable<UserRef?> get user => _user;

  /// Whether this device was removed from the group (revoked): the core
  /// stopped connecting; the chat stays readable (app-ui: Notices in the
  /// chat, "Bob is removed").
  ValueListenable<bool> get removed => _removed;

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
    for (final hook in List.of(_beforeStart)) {
      await hook();
    }
    _connection.value = ConnectionStatus.connecting;
    _running = true;
    await startCore();
  }

  /// Runs [hook] before the engine starts, each time it starts: while the
  /// engine is stopped no message or notice can arrive, so the app-level
  /// chat model reads the history there before the first event.
  void addBeforeStart(Future<void> Function() hook) => _beforeStart.add(hook);

  /// Undoes [addBeforeStart].
  void removeBeforeStart(Future<void> Function() hook) =>
      _beforeStart.remove(hook);

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

  /// Passes one core event on, keeping [connection] up to date. The debug
  /// log names the event only, never its text (app-ui: UI goes through the
  /// core).
  @protected
  void handleEvent(CoreEvent event) {
    if (kDebugMode) debugPrint('core event: ${eventLabel(event)}');
    if (event case CoreEvent_Connection(:final status)) {
      _connection.value = status;
    }
    if (event is CoreEvent_Removed) _removed.value = true;
    _events.add(event);
  }

  /// Releases the notifiers and the stream.
  @mustCallSuper
  Future<void> dispose() async {
    await _events.close();
    _state.dispose();
    _connection.dispose();
    _user.dispose();
    _removed.dispose();
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

  /// The phone's network changed: the core skips its reconnect backoff
  /// wait if it is disconnected, and does nothing otherwise (app-ui:
  /// Foreground and background, "Network back"; qa-fixes design D6).
  Future<void> reconnectNow();

  /// Re-sync a chat the core signalled is out of sync: ask the server to
  /// re-add this wedged device (app-ui: Out-of-sync re-sync affordance). The
  /// held message keeps its place and is sent once the device re-syncs.
  Future<void> requestResync();

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

  /// Registers or replaces this device's push token on the server (app-ui: Push
  /// notifications). Throws when offline; the app retries on the next
  /// connect.
  Future<void> setPushToken(PushPlatform platform, String token);

  /// Clears this device's push token on the server.
  Future<void> clearPushToken();

  /// Whether any own message is still sending (client-runtime: Unsent
  /// messages query); the app asks it when it goes to the background.
  Future<bool> hasUnsent();

  /// The newest [limit] messages, oldest first.
  Future<List<ChatMessage>> history(int limit);

  /// One message by local id.
  Future<ChatMessage?> message(int id);

  /// Sends a text message; returns its local id.
  Future<int> sendText(String text);

  /// Sends a text message with [mentions] (ranges of [text] in UTF-16 code
  /// units, as Dart indexes strings), answering message [replyTo] if set;
  /// returns its local id (message-content: Mentions; Replies).
  Future<int> sendMessage(
    String text, {
    List<MentionSpan> mentions = const [],
    int? replyTo,
  });

  /// The local ids of messages whose mention of this user is not seen yet,
  /// oldest first (client-runtime: Unseen mentions).
  Future<List<int>> unseenMentions();

  /// Marks the mention in message [id] seen.
  Future<bool> markMentionSeen(int id);

  // --- Images (message-content: Image messages; app-ui: Send an image,
  // Image bubble and viewer). The core makes the WebP, encrypts it, uploads
  // it and decrypts what comes back: the app only passes the picked file in
  // and draws the WebP it gets out. ---

  /// Sends the picked image file [picked] to chat [chatId] with an optional
  /// [caption] and [mentions] (UTF-16 ranges of the caption); returns its
  /// local id. Like a text it is queued at once, connected or not, and goes out
  /// when the core can send it.
  Future<int> sendImage(
    Uint8List picked, {
    int chatId = 0,
    String? caption,
    List<MentionSpan> mentions = const [],
  });

  /// The picture of image message [id] as WebP bytes: from the core's cache
  /// on this device, else downloaded first. Throws [CoreError] with
  /// [CoreErrorKind.mediaExpired] when the server no longer has it.
  Future<Uint8List> downloadImage(int id);

  /// The picture of image message [id] if this device has it cached; never
  /// uses the network.
  Future<Uint8List?> cachedImage(int id);

  /// Resends the picture of own image message [id] as a new message; returns
  /// the new message's local id.
  Future<int> resendImage(int id);

  // --- Voice messages (message-content: Audio messages; app-ui: Record a voice
  // message, Play a voice message). The core makes the OGG/Opus, encrypts it,
  // uploads it and decrypts what comes back: the app only passes the
  // recorder's WAV in and plays the file path it gets out. ---

  /// Sends the recording [wav] (the file the recorder wrote) to chat [chatId];
  /// returns its local id. Throws [CoreError] with [CoreErrorKind.audioTooLong]
  /// for a clip over two minutes. Like an image it is queued at once,
  /// connected or not.
  Future<int> sendAudio(Uint8List wav, {int chatId = 0, int? replyTo});

  /// The path of voice message [id]'s clip, an OGG/Opus file in the core's media
  /// cache for a player to open: from the cache on this device, else downloaded
  /// first. The app never copies or keeps the file. Throws [CoreError] with
  /// [CoreErrorKind.mediaExpired] when the server no longer has it.
  Future<String> downloadAudio(int id);

  /// The path of voice message [id]'s clip if this device has it cached; never
  /// uses the network.
  Future<String?> cachedAudio(int id);

  /// Resends the clip of own voice message [id] as a new message; returns the
  /// new message's local id.
  Future<int> resendAudio(int id);

  // --- Video messages (message-content: Video messages; app-ui: Send a video,
  // Video bubble and player). The app's native plugin normalizes the clip to
  // H.264/MP4 and extracts the first frame; the core does only the blob crypto,
  // encrypts it, uploads it and decrypts what comes back. The app passes the
  // normalized [mp4] and the [firstFrameRgba] in and plays the file path it gets
  // out. ---

  /// Sends the already-normalized clip [mp4] (H.264/MP4 at the 480p/~1 Mbps
  /// target, ≤ 30 s) to chat [chatId]; returns its local id. [firstFrameRgba]
  /// ([frameWidth] x [frameHeight] RGBA) is the first frame the core turns into
  /// the bubble's ThumbHash; pass an empty list for no blur. Throws [CoreError]
  /// with [CoreErrorKind.videoTooLong] over 30 s, [CoreErrorKind.videoTooLarge]
  /// over the server's advertised cap, or [CoreErrorKind.notAVideo] for non-MP4
  /// bytes. Like an image it is queued at once, connected or not.
  Future<int> sendVideo(
    Uint8List mp4, {
    int chatId = 0,
    required int durationMs,
    required int width,
    required int height,
    Uint8List? firstFrameRgba,
    int frameWidth = 0,
    int frameHeight = 0,
  });

  /// The path of video message [id]'s clip, an MP4 file in the core's media
  /// cache for a player to open: from the cache on this device, else downloaded
  /// first. The app never copies or keeps the file. Throws [CoreError] with
  /// [CoreErrorKind.mediaExpired] when the server no longer has it.
  Future<String> downloadVideo(int id);

  /// The path of video message [id]'s clip if this device has it cached; never
  /// uses the network.
  Future<String?> cachedVideo(int id);

  /// Resends the clip of own video message [id] as a new message; returns the
  /// new message's local id.
  Future<int> resendVideo(int id);

  /// The blur a ThumbHash stands for, as a small PNG; null if the bytes are
  /// not a ThumbHash. Decoded by the core.
  Uint8List? thumbhashPreview(Uint8List thumbhash);

  // --- Chats (direct-messages; app-ui: Chat list, change `dms`) ---

  /// Whether the server knows DMs: "Message" is offered only then.
  Future<bool> dmsAvailable();

  /// The chats: the main group (chat 0) first, then DMs, newest first,
  /// closed ones last.
  Future<List<ChatSummary>> chats();

  /// The open DM with [user], opened first if there is none; its chat id.
  Future<int> openDm(UserRef user);

  /// The newest [limit] messages of chat [chatId], oldest first.
  Future<List<ChatMessage>> historyIn(int chatId, int limit);

  /// [sendMessage] in chat [chatId].
  Future<int> sendMessageIn(
    int chatId,
    String text, {
    List<MentionSpan> mentions = const [],
    int? replyTo,
  });

  /// [unseenMentions] in chat [chatId].
  Future<List<int>> unseenMentionsIn(int chatId);

  /// The user has seen chat [chatId] up to its newest message.
  Future<void> markChatRead(int chatId);

  /// The read mark of chat [chatId]: the local id of the newest message read
  /// last, or 0 when it was never read. The chat screen reads it on open to
  /// place the "New messages" divider (app-ui: Unread divider).
  Future<int> chatReadMark(int chatId);

  /// How to show [user]: "Ali (alice)", or the username alone.
  Future<String> displayName(UserRef user);

  /// [user]'s profile.
  Future<ProfileInfo> profile(UserRef user);

  /// Sets this user's display name and about text.
  Future<int> setProfile({String? displayName, String? about});

  /// The group's member devices.
  Future<List<MemberDevice>> members();

  /// The group's epoch on this device.
  Future<BigInt?> epoch();

  /// Whether this device's user is the admin.
  Future<bool> isAdmin();

  /// The admin's user, as the group names it (every member can tell); null
  /// before this device is in the group.
  Future<UserRef?> adminUser();

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

  /// Admin: removes [user] from the group and the server.
  Future<void> removeUser(UserRef user);

  /// Whether the server knows named groups (it lists protocol version 3):
  /// "New group", "Rename" and "Close group" are offered only then.
  Future<bool> namedGroupsAvailable();

  /// Admin: creates a named group called [name] (optional) and adds the
  /// devices of [members]; returns its chat id.
  Future<int> createNamedGroup(String? name, List<UserRef> members);

  /// Admin: adds all devices of [user] to the named group [chatId].
  Future<void> addToGroup(int chatId, UserRef user);

  /// Admin: removes [user] from the named group [chatId] (not from the lobby).
  Future<void> removeFromGroup(int chatId, UserRef user);

  /// Admin: sets the name of the named group [chatId].
  Future<int> renameGroup(int chatId, String name);

  /// Admin: closes the named group [chatId]; it becomes read-only.
  Future<void> closeGroup(int chatId);

  /// Removes the closed DM [chatId] from this user's devices: the core tells the
  /// server, then deletes the chat and its messages (message-deletion: Remove a
  /// closed DM). Throws if it is not a closed DM or the server cannot be
  /// reached; nothing is deleted then.
  Future<void> removeClosedDm(int chatId);

  /// The member devices of the named group [chatId].
  Future<List<MemberDevice>> groupMembers(int chatId);

  /// Admin: accepts [user]'s changed identity key.
  Future<bool> acceptIdentityKey(UserRef user);

  /// The owner re-trusts [user]'s changed safety number (admin-recovery): the
  /// re-trust prompt clears and the identity is verified again. On admin
  /// devices a key kept waiting is also accepted and the user re-added.
  Future<bool> acknowledgeIdentityKey(UserRef user);

  /// The users whose safety number changed and still await the owner's
  /// re-trust (admin-recovery: Admin re-key forces peer re-trust).
  Future<List<UserRef>> identityRetrustPending();
}
