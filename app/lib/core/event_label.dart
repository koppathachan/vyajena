// A loggable name for a core event: its kind and ids, never message text,
// profile text or group names (app-ui: UI goes through the core).
import 'core_service.dart';

/// The event's kind and ids, safe to log.
String eventLabel(CoreEvent event) => switch (event) {
  CoreEvent_Message(:final message) => 'message id=${message.id}',
  CoreEvent_SendStatus(:final id, :final status) =>
    'send-status id=$id ${switch (status) {
      SendState_Sending() => 'sending',
      SendState_Accepted() => 'accepted',
      SendState_Failed() => 'failed',
      SendState_OutOfSync() => 'out-of-sync',
    }}',
  CoreEvent_ProfileChanged(:final user) =>
    'profile-changed ${user.username}@${user.serverId}',
  CoreEvent_Unreadable() => 'unreadable',
  CoreEvent_Gap() => 'gap',
  CoreEvent_ChatsChanged() => 'chats changed',
  CoreEvent_RemovedForInactivity() => 'removed-for-inactivity',
  CoreEvent_Rejoined() => 'rejoined',
  CoreEvent_OutOfSync(:final chatId) => 'out-of-sync chat=$chatId',
  CoreEvent_IdentityKeyChanged(:final user) =>
    'identity-key-changed ${user.username}@${user.serverId}',
  CoreEvent_ServerIdentityError() => 'server-identity-error',
  CoreEvent_Removed() => 'removed',
  CoreEvent_JoinedGroup() => 'joined-group',
  CoreEvent_MembersChanged() => 'members-changed',
  CoreEvent_CommitRefused() => 'commit-refused',
  CoreEvent_ReaddRequested() => 'readd-requested',
  CoreEvent_Connection(:final status) => 'connection ${status.name}',
  CoreEvent_Other() => 'other',
};
