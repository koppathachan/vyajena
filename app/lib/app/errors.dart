// What the app says when a core call fails.
import '../core/core_service.dart';

/// A short, user-facing description of [error].
String errorText(Object error) {
  if (error is! CoreError) return '$error';
  return switch (error.kind) {
    CoreErrorKind.serverIdentity =>
      "The server's identity does not match the invite. Not connecting: "
          'someone may be pretending to be the server.',
    CoreErrorKind.network => 'Could not reach the server: ${error.message}',
    CoreErrorKind.offline => 'Not connected to the server right now.',
    CoreErrorKind.notAdmin => "Only the admin's devices can do this.",
    CoreErrorKind.notIdentityDevice =>
      'Only the phone you joined with can link devices.',
    CoreErrorKind.noGroup => 'This phone is not in the group yet.',
    CoreErrorKind.tooLong => 'The text is too long to send.',
    CoreErrorKind.imageUnsupported =>
      'That file is not a picture the app can send.',
    CoreErrorKind.imageTooLarge => 'That picture is too large to send.',
    CoreErrorKind.audioUnsupported =>
        'That recording cannot be sent. Try recording it again.',
    CoreErrorKind.audioTooLong =>
        'That voice message is too long to send. Keep it under two minutes.',
    CoreErrorKind.mediaExpired => 'This has expired.',
    CoreErrorKind.mediaNotOnDevice =>
      'This phone no longer has it to send again.',
    _ => error.message,
  };
}
