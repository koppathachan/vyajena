// app-ui: Composer limit. Text whose encoded content is over the core's
// MAX_CONTENT_BYTES is refused before sending (`textFits`); the text stays
// in the composer. Once this device was removed from the group the
// composer is disabled (app-ui: Notices in the chat, "Bob is removed").
//
// app-ui: Mention picker. Typing "@" at the start or after a space lists the
// other members and everyone; picking one inserts "@" + its name and
// records the mention by user id. app-ui: Reply to a message: while
// replying, the quote shows above the box with a cancel button.
//
// app-ui: Send an image. The attach button picks a picture, which shows as a
// small preview above the box; the text box then holds its caption (with the
// same mention picker) and Send hands the picture's bytes and the caption to
// the core. Nothing here reads the picture's pixels.
//
// app-ui: Record a voice message. With nothing typed the Send button is a
// microphone: press and hold to record, release to stop and get a preview (play,
// delete, send), slide left while holding to cancel. The recorder writes a WAV
// file and Send hands its bytes to the core, which makes the OGG/Opus; nothing
// here encodes or uploads audio. The temporary file is deleted once the clip is
// sent or thrown away.
import 'dart:async';
import 'dart:typed_data';

import 'package:flutter/material.dart';

import '../app/errors.dart';
import '../core/core_service.dart';
import 'audio_bubble.dart';
import 'mentions.dart';
import 'voice_playback.dart';
import 'voice_plugins.dart';

/// The reply the composer is writing: who and what it answers.
class ReplyDraft {
  const ReplyDraft({
    required this.id,
    required this.name,
    required this.quote,
    this.kind = ReplyKind.text,
    this.durationMs = 0,
    this.thumbhash = const [],
  });

  /// The original's local id.
  final int id;

  /// How to show its sender.
  final String name;

  /// Its text, cut for the bar (empty for a voice original).
  final String quote;

  /// What the original was, so the bar previews a voice or picture original
  /// (message-content: Reply quote describes the original's kind).
  final ReplyKind kind;

  /// For a voice original, its length in milliseconds (0 otherwise).
  final int durationMs;

  /// For a picture original, its ThumbHash bytes (empty otherwise), decoded to
  /// a blurred thumbnail in the reply bar.
  final List<int> thumbhash;
}

/// What the composer says about text that does not fit.
const tooLongText = 'Too long to send. Make it shorter.';

/// The longest voice message the app records: the core's two-minute limit
/// (`MAX_AUDIO_DURATION_MS`) less a second for what the recorder keeps
/// beyond the timer.
const voiceLimit = Duration(seconds: 119);

/// A recording shorter than this is a stray tap, not a message.
const voiceMinimum = Duration(seconds: 1);

/// How far left the finger slides to cancel a recording.
const voiceCancelDistance = 80.0;

/// What the composer says after a tap on the microphone.
const voiceHoldHint = 'Hold the microphone to record a voice message.';

/// What the composer says when the app may not use the microphone.
const microphoneDeniedText =
    'Allow the microphone in Settings to record a voice message.';

/// The message box and Send button.
class Composer extends StatefulWidget {
  const Composer({
    super.key,
    required this.core,
    required this.send,
    this.choices,
    this.reply,
    this.onCancelReply,
    this.pickImage,
    this.sendImage,
    this.sendAudio,
    this.onSendVideo,
    this.onRecordingStarted,
  });

  final CoreService core;

  /// Sends the text with its mentions (stores it in the core).
  final Future<void> Function(String text, List<MentionSpan> mentions) send;

  /// Lets the user choose a picture; its file bytes, or null for none. No
  /// attach button when unset.
  final Future<Uint8List?> Function()? pickImage;

  /// Sends the picked picture with its caption and the caption's mentions.
  final Future<void> Function(
    Uint8List picture,
    String? caption,
    List<MentionSpan> mentions,
  )?
  sendImage;

  /// Sends a recorded voice message: the WAV file's bytes, which the core
  /// encodes. No microphone button when unset.
  final Future<void> Function(Uint8List wav, {int? replyTo})? sendAudio;

  /// Runs the whole send-a-video flow (record or gallery-pick, trim, native
  /// normalize, then hand to the core). Owned by the chat screen so the
  /// composer stays free of the camera, picker and native plugin. No "Video"
  /// entry in the attachment sheet when unset (app-ui: Send a video).
  final Future<void> Function()? onSendVideo;

  /// A recording began: the chat stops any clip that is playing.
  final VoidCallback? onRecordingStarted;

  /// What the mention picker offers; no picker when unset.
  final List<MentionChoice> Function()? choices;

  /// The reply being written, if any.
  final ReplyDraft? reply;

  /// Drops [reply].
  final VoidCallback? onCancelReply;

  @override
  State<Composer> createState() => _ComposerState();
}

class _ComposerState extends State<Composer> {
  final _text = TextEditingController();
  final _focus = FocusNode();
  final _mentions = ComposerMentions();
  String? _error;
  bool _sending = false;
  ({int at, String query})? _query;

  /// The picked picture waiting to be sent, whose caption is the text.
  Uint8List? _picture;

  /// A pick is in flight: the composer shows a placeholder at once so choosing
  /// a picture feels immediate even while the platform prepares its bytes.
  bool _attaching = false;

  // Voice recording (app-ui: Record a voice message).
  VoiceRecorder? _recorder;
  bool _held = false;
  bool _starting = false;
  bool _recording = false;
  double _slide = 0;
  Duration _elapsed = Duration.zero;
  Timer? _ticker;
  Offset _downAt = Offset.zero;

  /// The finished recording waiting in the preview, and its player.
  VoiceClip? _clip;
  VoicePlayback? _preview;

  bool get _willCancel => _slide >= voiceCancelDistance;

  void _changed(String text) {
    _mentions.edited(text);
    final cursor = _text.selection.baseOffset;
    final query = widget.choices == null
        ? null
        : mentionQuery(text, cursor < 0 ? text.length : cursor);
    setState(() {
      _query = query;
      _error = null;
    });
  }

  void _pick(MentionChoice choice) {
    final query = _query;
    if (query == null) return;
    final cursor = _text.selection.baseOffset < 0
        ? _text.text.length
        : _text.selection.baseOffset;
    final result = _mentions.insert(_text.text, query.at, cursor, choice);
    _text.value = TextEditingValue(
      text: result.text,
      selection: TextSelection.collapsed(offset: result.cursor),
    );
    setState(() => _query = null);
  }

  Widget _picker(BuildContext context) {
    final query = _query;
    final all = widget.choices;
    if (query == null || all == null) return const SizedBox.shrink();
    final choices = filterChoices(all(), query.query);
    if (choices.isEmpty) return const SizedBox.shrink();
    return ConstrainedBox(
      key: const Key('mention-picker'),
      constraints: const BoxConstraints(maxHeight: 220),
      child: Material(
        color: Theme.of(context).colorScheme.surfaceContainer,
        child: ListView(
          shrinkWrap: true,
          padding: EdgeInsets.zero,
          children: [
            for (final c in choices)
              ListTile(
                dense: true,
                leading: Icon(
                  c.user == null ? Icons.groups : Icons.alternate_email,
                ),
                title: Text(c.label),
                subtitle: Text(c.detail),
                onTap: () => _pick(c),
              ),
          ],
        ),
      ),
    );
  }

  Widget _replyBar(BuildContext context, ReplyDraft reply) {
    final scheme = Theme.of(context).colorScheme;
    final text = Theme.of(context).textTheme;
    return Container(
      key: const Key('reply-bar'),
      margin: const EdgeInsets.fromLTRB(12, 4, 8, 0),
      padding: const EdgeInsets.only(left: 8),
      decoration: BoxDecoration(
        border: Border(left: BorderSide(color: scheme.primary, width: 3)),
      ),
      child: Row(
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisSize: MainAxisSize.min,
              children: [
                Text(
                  'Replying to ${reply.name}',
                  style: text.labelSmall?.copyWith(color: scheme.primary),
                ),
                _replyPreview(context, reply),
              ],
            ),
          ),
          IconButton(
            tooltip: 'Cancel reply',
            onPressed: widget.onCancelReply,
            icon: const Icon(Icons.close),
          ),
        ],
      ),
    );
  }

  /// The one-line preview of what a reply answers: a mic glyph and the duration
  /// for a voice original, the quoted text otherwise (message-content: Reply
  /// quote describes the original's kind).
  Widget _replyPreview(BuildContext context, ReplyDraft reply) {
    final style = Theme.of(context).textTheme.bodySmall;
    switch (reply.kind) {
      case ReplyKind.audio:
        final length = clipLengthText(Duration(milliseconds: reply.durationMs));
        return Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            const Icon(Icons.mic, size: 16),
            const SizedBox(width: 4),
            Text('Voice message · $length', style: style),
          ],
        );
      case ReplyKind.image:
        final blur = reply.thumbhash.isEmpty
            ? null
            : widget.core.thumbhashPreview(Uint8List.fromList(reply.thumbhash));
        return Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            if (blur != null)
              ClipRRect(
                borderRadius: BorderRadius.circular(3),
                child: Image.memory(blur, width: 20, height: 20, fit: BoxFit.cover),
              )
            else
              const Icon(Icons.photo, size: 16),
            const SizedBox(width: 4),
            Flexible(
              child: Text(
                reply.quote.isEmpty ? 'Picture' : reply.quote,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: style,
              ),
            ),
          ],
        );
      case ReplyKind.text:
        return Text(
          reply.quote,
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
          style: style,
        );
    }
  }

  @override
  void didUpdateWidget(Composer old) {
    super.didUpdateWidget(old);
    // Swipe-to-reply (and the reply bar appearing) should raise the keyboard
    // and keep it up: when a reply begins, re-assert focus on the text box so
    // the user can answer the quote straight away instead of the keyboard
    // dropping as the bar slides in above it.
    if (widget.reply != null && old.reply == null) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted && !widget.core.removed.value) _focus.requestFocus();
      });
    }
  }

  @override
  void dispose() {
    _ticker?.cancel();
    _text.dispose();
    _focus.dispose();
    _preview?.dispose();
    final recorder = _recorder;
    if (recorder != null) {
      // Leaving mid-recording throws it away; a preview left behind goes too.
      unawaited(
        (_recording || _starting ? recorder.cancel() : Future<void>.value())
            .then((_) => recorder.dispose()),
      );
    }
    final clip = _clip;
    if (clip != null) unawaited(clip.delete());
    super.dispose();
  }

  // --- Voice ---

  void _micDown(PointerDownEvent event) {
    if (_recording || _starting || _sending) return;
    _held = true;
    _downAt = event.position;
    unawaited(_startRecording());
  }

  void _micMove(PointerMoveEvent event) {
    if (!_recording) return;
    final slide = (_downAt.dx - event.position.dx).clamp(0.0, 400.0);
    if (slide != _slide) setState(() => _slide = slide);
  }

  void _micUp(PointerUpEvent event) => _released(cancel: false);

  void _micCancel(PointerCancelEvent event) => _released(cancel: true);

  void _released({required bool cancel}) {
    _held = false;
    if (_recording) unawaited(_finishRecording(cancel: cancel || _willCancel));
  }

  Future<void> _startRecording() async {
    _starting = true;
    setState(() => _error = null);
    final recorder = _recorder ??= VoiceScope.of(context).newRecorder();
    try {
      if (!await recorder.hasPermission()) {
        if (mounted) setState(() => _error = microphoneDeniedText);
        return;
      }
      await recorder.start();
    } catch (error) {
      if (mounted) setState(() => _error = errorText(error));
      return;
    } finally {
      _starting = false;
    }
    if (!mounted) return;
    widget.onRecordingStarted?.call();
    setState(() {
      _recording = true;
      _slide = 0;
      _elapsed = Duration.zero;
    });
    const tick = Duration(milliseconds: 100);
    _ticker = Timer.periodic(tick, (_) {
      if (!_recording) return;
      setState(() => _elapsed += tick);
      if (_elapsed >= voiceLimit && _held) {
        // The longest message: it stops by itself and goes to the preview.
        _held = false;
        unawaited(_finishRecording(cancel: false));
      }
    });
    // Released while the recorder was starting (the permission prompt, say).
    if (!_held) unawaited(_finishRecording(cancel: _willCancel));
  }

  Future<void> _finishRecording({required bool cancel}) async {
    if (!_recording) return;
    _ticker?.cancel();
    final recorder = _recorder;
    if (recorder == null) return;
    setState(() {
      _recording = false;
      _slide = 0;
    });
    try {
      if (cancel) {
        await recorder.cancel();
        return;
      }
      final clip = await recorder.stop();
      if (clip == null || clip.length < voiceMinimum) {
        if (clip != null) await clip.delete();
        if (mounted) setState(() => _error = voiceHoldHint);
        return;
      }
      if (!mounted) {
        await clip.delete();
        return;
      }
      _preview?.dispose();
      setState(() {
        _clip = clip;
        _preview = VoicePlayback(VoiceScope.of(context).newPlayer);
      });
    } catch (error) {
      if (mounted) setState(() => _error = errorText(error));
    }
  }

  /// Throws the previewed recording away.
  Future<void> _deleteClip() async {
    final clip = _clip;
    if (clip == null) return;
    final preview = _preview;
    setState(() {
      _clip = null;
      _preview = null;
    });
    preview?.dispose();
    await clip.delete();
  }

  /// Hands the previewed recording to the core.
  Future<void> _sendClip() async {
    final clip = _clip;
    final sendAudio = widget.sendAudio;
    if (clip == null || sendAudio == null || _sending) return;
    setState(() {
      _error = null;
      _sending = true;
    });
    try {
      // Carry the pending reply, if any, so the voice message answers it
      // (message-content: Voice replies). The parent clears the reply bar.
      await sendAudio(await clip.read(), replyTo: widget.reply?.id);
      await _deleteClip();
    } catch (error) {
      // The preview stays, so the clip can be sent again or deleted.
      if (mounted) setState(() => _error = errorText(error));
    } finally {
      if (mounted) setState(() => _sending = false);
    }
  }

  /// The attachment sheet (app-shell: Conversation screen). Photo is the only
  /// kind a message can carry beyond a voice message today; video, audio
  /// files and files join the sheet as `video-messages` and later changes
  /// ship, and stay out of it until then.
  Future<void> _openAttachments() async {
    if (_sending || _attaching) return;
    final choice = await showModalBottomSheet<String>(
      context: context,
      builder: (sheet) => SafeArea(
        child: Column(
          key: const Key('attachment-sheet'),
          mainAxisSize: MainAxisSize.min,
          children: [
            if (widget.pickImage != null)
              ListTile(
                key: const Key('attach-photo'),
                leading: const Icon(Icons.image_outlined),
                title: const Text('Photo'),
                onTap: () => Navigator.of(sheet).pop('photo'),
              ),
            if (widget.onSendVideo != null)
              ListTile(
                key: const Key('attach-video'),
                leading: const Icon(Icons.videocam_outlined),
                title: const Text('Video'),
                onTap: () => Navigator.of(sheet).pop('video'),
              ),
          ],
        ),
      ),
    );
    if (choice == 'photo') {
      await _attach();
    } else if (choice == 'video') {
      await _sendVideo();
    }
  }

  Future<void> _sendVideo() async {
    final send = widget.onSendVideo;
    if (send == null || _sending) return;
    try {
      await send();
    } catch (error) {
      if (mounted) setState(() => _error = errorText(error));
    }
  }

  Future<void> _attach() async {
    final pick = widget.pickImage;
    if (pick == null || _sending || _attaching) return;
    // Show the placeholder before awaiting the pick, so the preview area fills
    // the instant the user chooses rather than after the bytes are ready.
    setState(() {
      _attaching = true;
      _error = null;
    });
    try {
      final picked = await pick();
      if (picked != null && mounted) setState(() => _picture = picked);
    } catch (error) {
      if (mounted) setState(() => _error = errorText(error));
    } finally {
      if (mounted) setState(() => _attaching = false);
    }
  }

  /// The placeholder shown while a pick is in flight, in the same place the
  /// picture preview will take, so choosing a picture feels immediate.
  Widget _attachingBar(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Container(
      key: const Key('picture-attaching'),
      margin: const EdgeInsets.fromLTRB(12, 4, 8, 0),
      child: Row(
        children: [
          ClipRRect(
            borderRadius: BorderRadius.circular(6),
            child: Container(
              width: 56,
              height: 56,
              color: scheme.surfaceContainerHighest,
              child: const Center(
                child: SizedBox(
                  width: 24,
                  height: 24,
                  child: CircularProgressIndicator(strokeWidth: 2),
                ),
              ),
            ),
          ),
          const SizedBox(width: 12),
          const Expanded(child: Text('Adding picture…')),
        ],
      ),
    );
  }

  Widget _pictureBar(BuildContext context, Uint8List picture) {
    final scheme = Theme.of(context).colorScheme;
    return Container(
      key: const Key('picture-preview'),
      margin: const EdgeInsets.fromLTRB(12, 4, 8, 0),
      child: Row(
        children: [
          ClipRRect(
            borderRadius: BorderRadius.circular(6),
            child: Image.memory(
              picture,
              width: 56,
              height: 56,
              fit: BoxFit.cover,
              // Decoded small: this is only the preview.
              cacheWidth: 112,
              errorBuilder: (_, _, _) => Container(
                width: 56,
                height: 56,
                color: scheme.surfaceContainerHighest,
                child: const Icon(Icons.image),
              ),
            ),
          ),
          const SizedBox(width: 12),
          const Expanded(child: Text('Picture to send')),
          IconButton(
            key: const Key('remove-picture'),
            tooltip: 'Remove the picture',
            onPressed: _sending ? null : () => setState(() => _picture = null),
            icon: const Icon(Icons.close),
          ),
        ],
      ),
    );
  }

  Future<void> _send() async {
    final text = _text.text;
    final picture = _picture;
    if ((text.trim().isEmpty && picture == null) || _sending) return;
    if (!widget.core.textFits(text)) {
      setState(() => _error = tooLongText);
      return;
    }
    setState(() {
      _error = null;
      _sending = true;
    });
    try {
      final sendImage = widget.sendImage;
      if (picture != null && sendImage != null) {
        // The caption is the text as typed, so the mention ranges still fit.
        final caption = text.trim().isEmpty ? null : text;
        await sendImage(
          picture,
          caption,
          caption == null ? [] : _mentions.spans,
        );
        if (mounted) setState(() => _picture = null);
      } else {
        await widget.send(text, _mentions.spans);
      }
      _text.clear();
      _mentions.clear();
      if (mounted) setState(() => _query = null);
    } catch (error) {
      if (mounted) setState(() => _error = errorText(error));
    } finally {
      if (mounted) setState(() => _sending = false);
    }
  }

  @override
  Widget build(BuildContext context) => ValueListenableBuilder<bool>(
    valueListenable: widget.core.removed,
    builder: (context, removed, _) => _box(context, removed),
  );

  Widget _box(BuildContext context, bool removed) => SafeArea(
    top: false,
    child: Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        _picker(context),
        if (widget.reply case final ReplyDraft reply when !removed)
          _replyBar(context, reply),
        if (_attaching && !removed)
          _attachingBar(context)
        else if (_picture case final Uint8List picture when !removed)
          _pictureBar(context, picture),
        _field(context, removed),
      ],
    ),
  );

  /// Whether the last button is the microphone: nothing typed or picked. A
  /// reply may be pending — a voice message can answer one (message-content:
  /// Voice replies), so the mic stays while the text field is empty.
  bool get _micShown =>
      widget.sendAudio != null && _text.text.isEmpty && _picture == null;

  Widget _field(BuildContext context, bool removed) {
    if (_clip != null && !removed) return _previewRow(context);
    return Padding(
      padding: const EdgeInsets.fromLTRB(12, 4, 8, 8),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          if (_recording)
            Expanded(child: _recordingInfo(context))
          else ...[
            // A picture or video goes as its own message, not as a reply.
            if ((widget.pickImage != null || widget.onSendVideo != null) &&
                widget.reply == null)
              IconButton(
                key: const Key('attach-image'),
                tooltip: 'Attach',
                onPressed: _sending || _attaching || removed
                    ? null
                    : _openAttachments,
                icon: const Icon(Icons.attach_file),
              ),
            Expanded(
              child: TextField(
                key: const Key('composer'),
                controller: _text,
                focusNode: _focus,
                enabled: !removed,
                minLines: 1,
                maxLines: 6,
                textCapitalization: TextCapitalization.sentences,
                decoration: InputDecoration(
                  hintText: removed
                      ? 'Removed from the group'
                      : _picture != null
                      ? 'Add a caption'
                      : 'Message',
                  border: const OutlineInputBorder(),
                  isDense: true,
                  errorText: _error,
                  errorMaxLines: 3,
                ),
                onChanged: _changed,
              ),
            ),
          ],
          const SizedBox(width: 4),
          if ((_micShown && !removed) || _recording)
            _micButton(context, removed)
          else
            IconButton.filled(
              tooltip: 'Send',
              onPressed: _sending || removed ? null : _send,
              icon: const Icon(Icons.send),
            ),
        ],
      ),
    );
  }

  /// The microphone: held to record. It keeps its place in the row while
  /// recording, so the finger that holds it keeps its pointer.
  Widget _micButton(BuildContext context, bool removed) {
    final scheme = Theme.of(context).colorScheme;
    final active = _recording;
    return Listener(
      key: const Key('record-voice'),
      onPointerDown: removed || _sending ? null : _micDown,
      onPointerMove: _micMove,
      onPointerUp: _micUp,
      onPointerCancel: _micCancel,
      child: Semantics(
        label: 'Hold to record a voice message',
        button: true,
        child: Container(
          width: 48,
          height: 48,
          decoration: BoxDecoration(
            shape: BoxShape.circle,
            color: active
                ? (_willCancel ? scheme.error : scheme.primary)
                : scheme.primary,
          ),
          child: Icon(
            active && _willCancel ? Icons.delete : Icons.mic,
            color: scheme.onPrimary,
          ),
        ),
      ),
    );
  }

  /// The strip shown while the mic is held: a red dot, the time, and the
  /// slide-to-cancel hint, which follows the finger.
  Widget _recordingInfo(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final cancelling = _willCancel;
    return SizedBox(
      key: const Key('recording-bar'),
      height: 48,
      child: Row(
        children: [
          Icon(Icons.fiber_manual_record, color: scheme.error, size: 16),
          const SizedBox(width: 8),
          Text(
            key: const Key('recording-time'),
            clipLengthText(_elapsed),
            style: Theme.of(context).textTheme.titleMedium,
          ),
          Expanded(
            child: Transform.translate(
              offset: Offset(-_slide.clamp(0.0, voiceCancelDistance), 0),
              child: Text(
                cancelling ? 'Release to cancel' : '‹ Slide to cancel',
                key: const Key('slide-to-cancel'),
                textAlign: TextAlign.center,
                style: TextStyle(
                  color: cancelling ? scheme.error : scheme.onSurfaceVariant,
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }

  /// The finished recording: play it, delete it, or send it.
  Widget _previewRow(BuildContext context) {
    final clip = _clip!;
    final preview = _preview!;
    final scheme = Theme.of(context).colorScheme;
    return Padding(
      padding: const EdgeInsets.fromLTRB(4, 4, 8, 8),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (_error case final error?)
            Padding(
              padding: const EdgeInsets.only(bottom: 4),
              child: Text(error, style: TextStyle(color: scheme.error)),
            ),
          Row(
            key: const Key('voice-preview'),
            children: [
              IconButton(
                key: const Key('preview-delete'),
                tooltip: 'Delete the recording',
                onPressed: _sending ? null : _deleteClip,
                icon: const Icon(Icons.delete_outline),
              ),
              AnimatedBuilder(
                animation: preview,
                builder: (context, _) {
                  final playing = preview.isPlaying(0);
                  final position = preview.positionOf(0);
                  final progress = clip.length.inMilliseconds == 0
                      ? 0.0
                      : (position.inMilliseconds / clip.length.inMilliseconds)
                            .clamp(0.0, 1.0);
                  return Expanded(
                    child: Row(
                      children: [
                        IconButton(
                          key: const Key('preview-play'),
                          tooltip: playing ? 'Pause' : 'Play',
                          onPressed: () => _previewToggle(preview, clip),
                          icon: Icon(playing ? Icons.pause : Icons.play_arrow),
                        ),
                        Expanded(
                          child: LinearProgressIndicator(value: progress),
                        ),
                        const SizedBox(width: 8),
                        Text(
                          key: const Key('preview-length'),
                          clipLengthText(
                            position > Duration.zero ? position : clip.length,
                          ),
                        ),
                      ],
                    ),
                  );
                },
              ),
              const SizedBox(width: 4),
              IconButton.filled(
                key: const Key('send-voice'),
                tooltip: 'Send voice message',
                onPressed: _sending ? null : _sendClip,
                icon: const Icon(Icons.send),
              ),
            ],
          ),
        ],
      ),
    );
  }

  Future<void> _previewToggle(VoicePlayback preview, VoiceClip clip) async {
    try {
      await preview.toggle(0, () async => clip.path);
    } catch (error) {
      if (mounted) setState(() => _error = errorText(error));
    }
  }
}
