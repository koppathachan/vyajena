// app-ui: Image bubble and viewer. An image message is drawn at the picture's
// aspect ratio: first the ThumbHash blur (decoded by the core), then, once the
// user taps it, the downloaded picture. A tap on the loaded picture opens it
// full screen. When the server no longer has the picture the blur stays with
// "Media expired — resend" (the sender can send it again from the message's
// long-press menu); a failed download can be tried again.
//
// The caption and its mentions are drawn by the message tile under this.
import 'dart:typed_data';

import 'package:flutter/material.dart';

import '../app/errors.dart';
import '../core/core_service.dart';

/// What an image that is gone from the server says.
const mediaExpiredText = 'Media expired — resend';

/// What a download that failed says.
const imageFailedText = "Couldn't load the picture. Tap to try again.";

/// The widest and tallest a picture is drawn in the chat.
const _maxWidth = 260.0;
const _maxHeight = 320.0;

/// The shortest long edge a bubble is drawn at: a tiny picture is scaled up
/// to this so its download button and notices fit and it can be tapped.
const _minLongEdge = 120.0;

/// The label under a download button: "1.2 MB", "340 KB".
String sizeText(BigInt bytes) {
  final kb = bytes.toInt() / 1024;
  return kb >= 1024
      ? '${(kb / 1024).toStringAsFixed(1)} MB'
      : '${kb.ceil()} KB';
}

/// The picture of one image message.
class ImageBubble extends StatefulWidget {
  const ImageBubble({
    super.key,
    required this.core,
    required this.messageId,
    required this.image,
  });

  final CoreService core;

  /// The message's local id, which names the picture to the core.
  final int messageId;

  final ImageView image;

  @override
  State<ImageBubble> createState() => _ImageBubbleState();
}

class _ImageBubbleState extends State<ImageBubble> {
  late final Uint8List? _blur = widget.core.thumbhashPreview(
    Uint8List.fromList(widget.image.thumbhash),
  );
  Uint8List? _picture;
  bool _loading = false;
  bool _expired = false;
  String? _error;

  @override
  void initState() {
    super.initState();
    _expired = widget.image.state == MediaStatus.expired;
    if (widget.image.state == MediaStatus.available) _showCached();
  }

  @override
  void didUpdateWidget(ImageBubble old) {
    super.didUpdateWidget(old);
    if (widget.image.state == MediaStatus.expired && _picture == null) {
      _expired = true;
    }
  }

  Future<void> _showCached() async {
    final cached = await widget.core.cachedImage(widget.messageId);
    if (mounted && cached != null) setState(() => _picture = cached);
  }

  Future<void> _tap() async {
    final picture = _picture;
    if (picture != null) {
      await Navigator.of(context).push(
        MaterialPageRoute<void>(builder: (_) => ImageViewer(picture: picture)),
      );
      return;
    }
    if (_loading || _expired) return;
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final downloaded = await widget.core.downloadImage(widget.messageId);
      if (mounted) setState(() => _picture = downloaded);
    } catch (error) {
      if (!mounted) return;
      if (error is CoreError && error.kind == CoreErrorKind.mediaExpired) {
        setState(() => _expired = true);
      } else {
        setState(() => _error = errorText(error));
      }
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  /// The size the picture is drawn at: its own shape, within the limits.
  Size _size() {
    final w = widget.image.width.toDouble();
    final h = widget.image.height.toDouble();
    final longEdge = w > h ? w : h;
    final scale = [
      1.0 < _minLongEdge / longEdge ? _minLongEdge / longEdge : 1.0,
      _maxWidth / w,
      _maxHeight / h,
    ].reduce((a, b) => a < b ? a : b);
    return Size(w * scale, h * scale);
  }

  Widget _backdrop(BuildContext context, Size size) {
    final picture = _picture;
    if (picture != null) {
      return Image.memory(
        picture,
        key: const Key('image-picture'),
        fit: BoxFit.cover,
        cacheWidth: (size.width * MediaQuery.devicePixelRatioOf(context))
            .round(),
        gaplessPlayback: true,
      );
    }
    final blur = _blur;
    if (blur != null) {
      return Image.memory(
        blur,
        key: const Key('image-blur'),
        fit: BoxFit.cover,
        filterQuality: FilterQuality.medium,
        gaplessPlayback: true,
      );
    }
    return ColoredBox(
      key: const Key('image-blur'),
      color: Theme.of(context).colorScheme.surfaceContainerHighest,
    );
  }

  Widget _overlay(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final text = Theme.of(context).textTheme;
    Widget strip(List<Widget> children) => Align(
      alignment: Alignment.bottomCenter,
      child: Container(
        width: double.infinity,
        color: scheme.scrim.withValues(alpha: 0.55),
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
        child: Wrap(
          alignment: WrapAlignment.center,
          crossAxisAlignment: WrapCrossAlignment.center,
          spacing: 8,
          children: children,
        ),
      ),
    );
    final onScrim = text.labelMedium?.copyWith(color: Colors.white);
    if (_picture != null) return const SizedBox.shrink();
    if (_loading) {
      return const Center(
        key: Key('image-loading'),
        child: CircularProgressIndicator(),
      );
    }
    if (_expired) {
      return strip([
        Text(mediaExpiredText, key: const Key('media-expired'), style: onScrim),
      ]);
    }
    if (_error case final error?) {
      return strip([
        Text(imageFailedText, key: const Key('image-failed'), style: onScrim),
        Text(error, style: onScrim, textAlign: TextAlign.center),
      ]);
    }
    return Center(
      key: const Key('image-download'),
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
        decoration: BoxDecoration(
          color: scheme.scrim.withValues(alpha: 0.55),
          borderRadius: BorderRadius.circular(20),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            const Icon(Icons.download, color: Colors.white, size: 18),
            const SizedBox(width: 6),
            Text(sizeText(widget.image.size), style: onScrim),
          ],
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final size = _size();
    return Semantics(
      label: _picture == null ? 'Picture, tap to load' : 'Picture',
      button: true,
      child: InkWell(
        key: const Key('image-bubble'),
        onTap: _tap,
        child: ClipRRect(
          borderRadius: BorderRadius.circular(8),
          child: SizedBox(
            width: size.width,
            height: size.height,
            child: Stack(
              fit: StackFit.expand,
              children: [_backdrop(context, size), _overlay(context)],
            ),
          ),
        ),
      ),
    );
  }
}

/// A picture full screen, which can be pinched and dragged.
class ImageViewer extends StatelessWidget {
  const ImageViewer({super.key, required this.picture});

  final Uint8List picture;

  @override
  Widget build(BuildContext context) => Scaffold(
    key: const Key('image-viewer'),
    backgroundColor: Colors.black,
    appBar: AppBar(
      backgroundColor: Colors.black,
      foregroundColor: Colors.white,
      elevation: 0,
    ),
    body: Center(
      child: InteractiveViewer(
        maxScale: 6,
        child: Image.memory(picture, fit: BoxFit.contain),
      ),
    ),
  );
}
