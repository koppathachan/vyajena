package com.purplechaamba.vyajena

import android.graphics.Bitmap
import android.media.MediaCodec
import android.media.MediaCodecInfo
import android.media.MediaExtractor
import android.media.MediaFormat
import android.media.MediaMetadataRetriever
import android.media.MediaMuxer
import io.flutter.plugin.common.MethodChannel
import java.io.File
import java.nio.ByteBuffer
import java.util.concurrent.Executors

/**
 * video-messages: Video normalization in a native plugin (design D3; spike S1).
 *
 * The Rust core cannot transcode H.264 on a phone, so a gallery clip (or a
 * recording) is normalized here with MediaCodec/MediaMuxer to the 480p/~1 Mbps
 * H.264/MP4 target, trimmed to the 30-second cap, and its first frame returned as
 * RGBA for the core's ThumbHash. The normalized file goes to the app's cache; Dart
 * reads its bytes, hands them to the core to encrypt, and deletes it. Nothing here
 * touches keys or ciphertext.
 *
 * The transcode runs a decode -> GL surface -> H.264 encode -> MP4 mux pipeline
 * (the GL blit lives in CodecSurfaces.kt). The MediaCodec/GL path is not
 * CI-unit-testable (it needs a real encoder); the Dart, core and bridge layers
 * are covered by their own tests, and a recorded clip is verified on-device.
 */
object VideoNormalize {
    const val CHANNEL = "chat.vyajena/video"

    /** The 480p long edge and ~1 Mbps target (protocol::video::NORMALIZE). */
    private const val LONG_EDGE = 854
    private const val BITRATE = 1_000_000
    private const val FRAME_RATE = 30
    private const val KEYFRAME_INTERVAL = 1
    private const val MIME = MediaFormat.MIMETYPE_VIDEO_AVC
    private const val AUDIO_MIME = MediaFormat.MIMETYPE_AUDIO_AAC

    /** The AAC bitrate the audio is re-encoded to when the source is not AAC. */
    private const val AUDIO_BITRATE = 128_000

    /** How long a codec dequeue waits for a buffer, in microseconds. */
    private const val TIMEOUT_US = 10_000L

    private val worker = Executors.newSingleThreadExecutor()

    fun handle(method: String, args: Map<*, *>?, result: MethodChannel.Result) {
        if (method != "normalize") {
            result.notImplemented()
            return
        }
        val path = args?.get("path") as? String
        if (path == null) {
            result.error("normalize", "no path", null)
            return
        }
        val maxDurationMs = (args["maxDurationMs"] as? Number)?.toLong() ?: 30_000L
        worker.execute {
            try {
                val out = normalize(path, maxDurationMs)
                // Flutter results must be delivered on the main thread.
                android.os.Handler(android.os.Looper.getMainLooper()).post {
                    result.success(out)
                }
            } catch (e: Exception) {
                android.os.Handler(android.os.Looper.getMainLooper()).post {
                    result.error("normalize", e.message ?: "transcode failed", null)
                }
            }
        }
    }

    private fun normalize(inputPath: String, maxDurationMs: Long): Map<String, Any?> {
        val retriever = MediaMetadataRetriever()
        retriever.setDataSource(inputPath)
        val srcWidth =
            retriever.extractMetadata(MediaMetadataRetriever.METADATA_KEY_VIDEO_WIDTH)
                ?.toInt() ?: 0
        val srcHeight =
            retriever.extractMetadata(MediaMetadataRetriever.METADATA_KEY_VIDEO_HEIGHT)
                ?.toInt() ?: 0
        val rotation =
            retriever.extractMetadata(MediaMetadataRetriever.METADATA_KEY_VIDEO_ROTATION)
                ?.toInt() ?: 0

        // The encoder's buffer keeps the SOURCE (stored, pre-rotation) aspect
        // ratio, scaled so its long edge is at most LONG_EDGE. The decoder
        // renders stored frames straight onto this surface in their stored
        // orientation, so the surface must match that orientation; sizing it to
        // the rotated (display) dimensions instead stretched a landscape frame
        // into a portrait buffer, which was the Android crop bug. Rotation is
        // carried as an MP4 orientation hint (see Transcoder) so the player
        // turns portrait/landscape back the right way — the same result
        // AVAssetExportSession gives on iOS by preserving the preferred
        // transform.
        val (encWidth, encHeight) = encodeSize(srcWidth, srcHeight)
        // What the clip looks like once the player applies the rotation hint:
        // the display size Dart lays the bubble out at. iOS reports the same, via
        // naturalSize.applying(preferredTransform).
        val (dispWidth, dispHeight) =
            if (rotation == 90 || rotation == 270) encHeight to encWidth else encWidth to encHeight

        val outFile = File.createTempFile("vyajena-video-", ".mp4")
        val transcoder =
            Transcoder(inputPath, outFile.absolutePath, encWidth, encHeight, rotation, maxDurationMs)
        val durationMs = transcoder.run()

        val frame = firstFrame(retriever)
        retriever.release()

        return mapOf(
            "path" to outFile.absolutePath,
            "durationMs" to durationMs.toInt(),
            "width" to dispWidth,
            "height" to dispHeight,
            "frameRgba" to (frame?.rgba ?: ByteArray(0)),
            "frameWidth" to (frame?.width ?: 0),
            "frameHeight" to (frame?.height ?: 0),
        )
    }

    /**
     * The encoder buffer size: the SOURCE (stored, pre-rotation) dimensions
     * scaled so the long edge is at most [LONG_EDGE], keeping the aspect ratio
     * exactly (scale to fit, never crop) with even sides (H.264 requires even
     * dimensions). Rotation is not applied here — it is carried as the muxer's
     * orientation hint — so these stay in the stored orientation the decoder
     * renders onto the encoder surface.
     */
    private fun encodeSize(w: Int, h: Int): Pair<Int, Int> {
        if (w <= 0 || h <= 0) return LONG_EDGE to even(LONG_EDGE * 9 / 16)
        val longEdge = maxOf(w, h)
        if (longEdge <= LONG_EDGE) return even(w) to even(h)
        val scale = LONG_EDGE.toDouble() / longEdge
        return even((w * scale).toInt()) to even((h * scale).toInt())
    }

    private fun even(n: Int): Int = if (n % 2 == 0) n else n + 1

    private class FirstFrame(val rgba: ByteArray, val width: Int, val height: Int)

    /** The first frame as RGBA, scaled so its long edge is at most 160 px. */
    private fun firstFrame(retriever: MediaMetadataRetriever): FirstFrame? {
        val bitmap = retriever.getFrameAtTime(0) ?: return null
        val longEdge = maxOf(bitmap.width, bitmap.height)
        val scale = if (longEdge > 160) 160.0 / longEdge else 1.0
        val w = (bitmap.width * scale).toInt().coerceAtLeast(1)
        val h = (bitmap.height * scale).toInt().coerceAtLeast(1)
        val small = Bitmap.createScaledBitmap(bitmap, w, h, true)
        val pixels = IntArray(w * h)
        small.getPixels(pixels, 0, w, 0, 0, w, h)
        val rgba = ByteArray(w * h * 4)
        for (i in pixels.indices) {
            val p = pixels[i]
            rgba[i * 4] = ((p shr 16) and 0xFF).toByte() // R
            rgba[i * 4 + 1] = ((p shr 8) and 0xFF).toByte() // G
            rgba[i * 4 + 2] = (p and 0xFF).toByte() // B
            rgba[i * 4 + 3] = ((p shr 24) and 0xFF).toByte() // A
        }
        return FirstFrame(rgba, w, h)
    }

    /**
     * A decode -> GL surface -> H.264 encode -> MP4 mux pipeline that downscales
     * to [outWidth]x[outHeight] and trims to [maxDurationMs]. The audio track is
     * passed through (already AAC on a phone recording) when present. Returns the
     * output's duration in milliseconds.
     *
     * The decoder renders each frame into an [OutputSurface] (a SurfaceTexture),
     * which is drawn onto the encoder's input [InputSurface] (its EGL window) at
     * the target size, carrying the frame's timestamp. That GL blit is what
     * actually feeds the encoder: without it the encoder is handed no frames and
     * emits only its codec-config, so the MP4 ends up with a passed-through audio
     * track but an empty video track — the "plays sound, no picture" bug. See
     * CodecSurfaces.kt.
     */
    private class Transcoder(
        private val inputPath: String,
        private val outputPath: String,
        private val outWidth: Int,
        private val outHeight: Int,
        private val rotation: Int,
        private val maxDurationMs: Long,
    ) {
        fun run(): Long {
            val extractor = MediaExtractor()
            extractor.setDataSource(inputPath)
            val videoTrack = selectTrack(extractor, "video/")
            require(videoTrack >= 0) { "no video track" }
            val inFormat = extractor.getTrackFormat(videoTrack)
            val srcDurationUs =
                if (inFormat.containsKey(MediaFormat.KEY_DURATION))
                    inFormat.getLong(MediaFormat.KEY_DURATION)
                else maxDurationMs * 1000
            val durationUs = minOf(srcDurationUs, maxDurationMs * 1000)

            val outFormat = MediaFormat.createVideoFormat(MIME, outWidth, outHeight).apply {
                setInteger(
                    MediaFormat.KEY_COLOR_FORMAT,
                    MediaCodecInfo.CodecCapabilities.COLOR_FormatSurface,
                )
                setInteger(MediaFormat.KEY_BIT_RATE, BITRATE)
                setInteger(MediaFormat.KEY_FRAME_RATE, FRAME_RATE)
                setInteger(MediaFormat.KEY_I_FRAME_INTERVAL, KEYFRAME_INTERVAL)
            }
            val encoder = MediaCodec.createEncoderByType(MIME)
            encoder.configure(outFormat, null, null, MediaCodec.CONFIGURE_FLAG_ENCODE)
            // The encoder's input is a Surface driven through OpenGL; InputSurface
            // owns the EGL context, and the decoder's frames are drawn onto it.
            val inputSurface = InputSurface(encoder.createInputSurface())
            inputSurface.makeCurrent()
            // Created after makeCurrent so its GL texture lives in the encoder's
            // context. The decoder renders into this SurfaceTexture.
            val outputSurface = OutputSurface()

            val decoder = MediaCodec.createDecoderByType(
                inFormat.getString(MediaFormat.KEY_MIME) ?: MIME
            )
            val muxer = MediaMuxer(outputPath, MediaMuxer.OutputFormat.MUXER_OUTPUT_MPEG_4)
            // The encoded frames stay in the source's stored orientation; the
            // orientation hint tells the player how to rotate them for display,
            // so a portrait recording (stored landscape + rotation 90/270) plays
            // upright instead of stretched. Must be set before muxer.start().
            if (rotation == 90 || rotation == 180 || rotation == 270) {
                muxer.setOrientationHint(rotation)
            }

            // Audio: the output must be AAC (video-messages: H.264 + AAC). A
            // recording's track is already AAC and is copied through unchanged
            // (no re-encode); a gallery clip may carry audio in another codec
            // (Opus, Vorbis, …) that an MP4 cannot hold, so it is decoded and
            // re-encoded to AAC — the same sound iOS's AVAssetExportSession
            // produces. Without this a non-AAC gallery clip arrives silent. A
            // clip with no audio track is muxed video-only.
            val audioExtractor = MediaExtractor()
            audioExtractor.setDataSource(inputPath)
            val audioTrack = selectTrack(audioExtractor, "audio/")
            val sourceAudioFormat =
                if (audioTrack >= 0) audioExtractor.getTrackFormat(audioTrack) else null
            val sourceAudioMime = sourceAudioFormat?.getString(MediaFormat.KEY_MIME)
            val passthrough = sourceAudioMime == AUDIO_MIME
            // Re-encoded up front so the muxer can add the AAC track (with its
            // codec-specific data) before it starts; the encoded frames are held
            // in memory — a few hundred KB for a ≤ 30 s clip — and written after
            // the video track.
            val reencoded =
                if (sourceAudioFormat != null && !passthrough)
                    ReencodedAudio.from(inputPath, durationUs)
                else null
            val audioMuxFormat = when {
                sourceAudioFormat == null -> null
                passthrough -> sourceAudioFormat
                else -> reencoded!!.format
            }
            if (passthrough) audioExtractor.selectTrack(audioTrack)

            extractor.selectTrack(videoTrack)
            // The decoder renders into the SurfaceTexture, not straight onto the
            // encoder surface — the GL blit in drive() does the surface-to-surface
            // copy and scaling.
            decoder.configure(inFormat, outputSurface.surface, null, 0)
            decoder.start()
            encoder.start()
            drive(
                extractor, decoder, encoder, inputSurface, outputSurface, muxer,
                durationUs, audioMuxFormat,
            ) { track ->
                if (passthrough) copyAudio(audioExtractor, muxer, track, durationUs)
                else reencoded!!.writeTo(muxer, track)
            }

            encoder.stop(); encoder.release()
            decoder.stop(); decoder.release()
            outputSurface.release()
            inputSurface.release()
            extractor.release()
            audioExtractor.release()
            muxer.stop(); muxer.release()
            return durationUs / 1000
        }

        private fun selectTrack(extractor: MediaExtractor, prefix: String): Int {
            for (i in 0 until extractor.trackCount) {
                val mime = extractor.getTrackFormat(i).getString(MediaFormat.KEY_MIME) ?: continue
                if (mime.startsWith(prefix)) return i
            }
            return -1
        }

        /**
         * Feeds the extractor's samples through decode -> GL -> encode up to
         * [durationUs], muxing the encoded H.264. Each decoded frame is rendered
         * into [outputSurface] (the decoder's output), drawn onto [inputSurface]
         * (the encoder's input) and swapped with its timestamp, which is what
         * actually drives the encoder.
         */
        private fun drive(
            extractor: MediaExtractor,
            decoder: MediaCodec,
            encoder: MediaCodec,
            inputSurface: InputSurface,
            outputSurface: OutputSurface,
            muxer: MediaMuxer,
            durationUs: Long,
            audioMuxFormat: MediaFormat?,
            writeAudio: (muxTrack: Int) -> Unit,
        ) {
            val info = MediaCodec.BufferInfo()
            var muxVideoTrack = -1
            var muxAudioTrack = -1
            var muxerStarted = false
            var inputDone = false
            var decoderDone = false
            var encoderDone = false
            while (!encoderDone) {
                // 1. Feed the decoder from the extractor, stopping at the trim point.
                if (!inputDone) {
                    val inIndex = decoder.dequeueInputBuffer(TIMEOUT_US)
                    if (inIndex >= 0) {
                        val buffer: ByteBuffer = decoder.getInputBuffer(inIndex)!!
                        val size = extractor.readSampleData(buffer, 0)
                        if (size < 0 || extractor.sampleTime > durationUs) {
                            decoder.queueInputBuffer(
                                inIndex, 0, 0, 0, MediaCodec.BUFFER_FLAG_END_OF_STREAM
                            )
                            inputDone = true
                        } else {
                            decoder.queueInputBuffer(inIndex, 0, size, extractor.sampleTime, 0)
                            extractor.advance()
                        }
                    }
                }

                // 2. Drain a decoded frame and blit it onto the encoder surface.
                if (!decoderDone) {
                    val decIndex = decoder.dequeueOutputBuffer(info, TIMEOUT_US)
                    if (decIndex >= 0) {
                        val eos = info.flags and MediaCodec.BUFFER_FLAG_END_OF_STREAM != 0
                        // A zero-size buffer carries no picture (e.g. the EOS
                        // marker); only real frames are rendered and swapped.
                        val render = info.size != 0
                        decoder.releaseOutputBuffer(decIndex, render)
                        if (render) {
                            outputSurface.awaitNewImage()
                            outputSurface.drawImage()
                            // MediaMuxer wants microseconds; EGL wants nanoseconds.
                            inputSurface.setPresentationTime(info.presentationTimeUs * 1000)
                            inputSurface.swapBuffers()
                        }
                        if (eos) {
                            decoderDone = true
                            encoder.signalEndOfInputStream()
                        }
                    }
                }

                // 3. Drain everything the encoder has ready, muxing the H.264.
                var encoderOutputAvailable = true
                while (encoderOutputAvailable) {
                    val outIndex = encoder.dequeueOutputBuffer(info, TIMEOUT_US)
                    when {
                        outIndex == MediaCodec.INFO_TRY_AGAIN_LATER ->
                            encoderOutputAvailable = false
                        outIndex == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED -> {
                            muxVideoTrack = muxer.addTrack(encoder.outputFormat)
                            // Every track must be added before the muxer starts;
                            // the AAC audio format is known up front (source track
                            // or the re-encode), so it joins here alongside video.
                            if (audioMuxFormat != null) {
                                muxAudioTrack = muxer.addTrack(audioMuxFormat)
                            }
                            muxer.start()
                            muxerStarted = true
                        }
                        outIndex >= 0 -> {
                            val encoded = encoder.getOutputBuffer(outIndex)!!
                            // The codec-config (SPS/PPS) is folded into the track
                            // format by addTrack, so it is not written as a sample.
                            if (info.flags and MediaCodec.BUFFER_FLAG_CODEC_CONFIG != 0) {
                                info.size = 0
                            }
                            if (muxerStarted && info.size > 0) {
                                muxer.writeSampleData(muxVideoTrack, encoded, info)
                            }
                            encoder.releaseOutputBuffer(outIndex, false)
                            if (info.flags and MediaCodec.BUFFER_FLAG_END_OF_STREAM != 0) {
                                encoderDone = true
                                encoderOutputAvailable = false
                            }
                        }
                    }
                }
            }
            // The video track is complete; now write the AAC audio (passed
            // through or re-encoded), trimmed to the same window. MediaMuxer
            // accepts each track's samples independently as long as they are in
            // order within the track.
            if (muxerStarted && muxAudioTrack >= 0) {
                writeAudio(muxAudioTrack)
            }
        }

        /**
         * Copies the already-AAC audio samples from [audioExtractor] into
         * [muxTrack] of [muxer] unchanged (no re-encode), up to [durationUs] so the
         * sound matches the trimmed video. The extractor is positioned at the start
         * of its selected audio track.
         */
        private fun copyAudio(
            audioExtractor: MediaExtractor,
            muxer: MediaMuxer,
            muxTrack: Int,
            durationUs: Long,
        ) {
            // A generous buffer for one AAC frame (a few hundred bytes in practice).
            val buffer = ByteBuffer.allocate(256 * 1024)
            val info = MediaCodec.BufferInfo()
            audioExtractor.seekTo(0, MediaExtractor.SEEK_TO_PREVIOUS_SYNC)
            while (true) {
                val size = audioExtractor.readSampleData(buffer, 0)
                val time = audioExtractor.sampleTime
                if (size < 0 || time < 0 || time > durationUs) break
                info.offset = 0
                info.size = size
                info.presentationTimeUs = time
                info.flags =
                    if (audioExtractor.sampleFlags and MediaExtractor.SAMPLE_FLAG_SYNC != 0) {
                        MediaCodec.BUFFER_FLAG_KEY_FRAME
                    } else {
                        0
                    }
                muxer.writeSampleData(muxTrack, buffer, info)
                if (!audioExtractor.advance()) break
            }
        }
    }

    /**
     * A clip's audio decoded and re-encoded to AAC, held in memory. Used when the
     * source audio is not already AAC (a gallery clip may be Opus, Vorbis, …),
     * which an MP4 cannot hold — so without this the shared clip is silent. iOS's
     * AVAssetExportSession does the same re-encode; this is its Android match.
     * The encoded frames (a few hundred KB for a ≤ 30 s clip) are written into the
     * muxer's AAC track after the video track, by [writeTo].
     */
    private class ReencodedAudio private constructor(
        val format: MediaFormat,
        private val frames: List<Frame>,
    ) {
        private class Frame(val data: ByteArray, val ptsUs: Long, val flags: Int)

        /** Writes the buffered AAC frames into [muxTrack] of [muxer], in order. */
        fun writeTo(muxer: MediaMuxer, muxTrack: Int) {
            val info = MediaCodec.BufferInfo()
            for (frame in frames) {
                info.set(0, frame.data.size, frame.ptsUs, frame.flags)
                muxer.writeSampleData(muxTrack, ByteBuffer.wrap(frame.data), info)
            }
        }

        companion object {
            /**
             * Decodes the audio track of the clip at [inputPath] to PCM and
             * re-encodes it to AAC, trimmed to [durationUs]. The returned [format]
             * is the encoder's output format (with the AAC codec-specific data the
             * muxer needs).
             */
            fun from(inputPath: String, durationUs: Long): ReencodedAudio {
                val extractor = MediaExtractor()
                extractor.setDataSource(inputPath)
                var track = -1
                for (i in 0 until extractor.trackCount) {
                    val mime = extractor.getTrackFormat(i).getString(MediaFormat.KEY_MIME)
                    if (mime?.startsWith("audio/") == true) {
                        track = i
                        break
                    }
                }
                require(track >= 0) { "no audio track to re-encode" }
                extractor.selectTrack(track)
                val inFormat = extractor.getTrackFormat(track)
                val sampleRate = inFormat.getInteger(MediaFormat.KEY_SAMPLE_RATE)
                val channelCount = inFormat.getInteger(MediaFormat.KEY_CHANNEL_COUNT)
                val bytesPerUs = sampleRate.toDouble() * channelCount * 2 / 1_000_000.0

                val decoder = MediaCodec.createDecoderByType(
                    inFormat.getString(MediaFormat.KEY_MIME) ?: AUDIO_MIME
                )
                decoder.configure(inFormat, null, null, 0)
                decoder.start()

                val outFormat = MediaFormat.createAudioFormat(
                    AUDIO_MIME, sampleRate, channelCount,
                ).apply {
                    setInteger(
                        MediaFormat.KEY_AAC_PROFILE,
                        MediaCodecInfo.CodecProfileLevel.AACObjectLC,
                    )
                    setInteger(MediaFormat.KEY_BIT_RATE, AUDIO_BITRATE)
                }
                val encoder = MediaCodec.createEncoderByType(AUDIO_MIME)
                encoder.configure(outFormat, null, null, MediaCodec.CONFIGURE_FLAG_ENCODE)
                encoder.start()

                val frames = ArrayList<Frame>()
                var encoderFormat: MediaFormat? = null
                val info = MediaCodec.BufferInfo()
                var extractorDone = false
                var decoderDone = false
                var encoderEosSent = false
                var encoderDone = false
                // A decoded PCM buffer waiting to be handed to the encoder; the
                // decoder output is copied out (and released) so the encoder can be
                // fed over several iterations without stalling the decoder.
                var pending: ByteBuffer? = null
                var pendingPts = 0L

                while (!encoderDone) {
                    // 1. Feed the decoder from the extractor, up to the trim point.
                    if (!extractorDone) {
                        val inIndex = decoder.dequeueInputBuffer(TIMEOUT_US)
                        if (inIndex >= 0) {
                            val buf = decoder.getInputBuffer(inIndex)!!
                            val size = extractor.readSampleData(buf, 0)
                            if (size < 0 || extractor.sampleTime > durationUs) {
                                decoder.queueInputBuffer(
                                    inIndex, 0, 0, 0, MediaCodec.BUFFER_FLAG_END_OF_STREAM,
                                )
                                extractorDone = true
                            } else {
                                decoder.queueInputBuffer(
                                    inIndex, 0, size, extractor.sampleTime, 0,
                                )
                                extractor.advance()
                            }
                        }
                    }

                    // 2. Take one decoded PCM buffer (when none is pending).
                    if (!decoderDone && pending == null) {
                        val outIndex = decoder.dequeueOutputBuffer(info, TIMEOUT_US)
                        if (outIndex >= 0) {
                            if (info.flags and MediaCodec.BUFFER_FLAG_END_OF_STREAM != 0) {
                                decoderDone = true
                            } else if (info.size > 0) {
                                val pcm = decoder.getOutputBuffer(outIndex)!!
                                val copy = ByteArray(info.size)
                                pcm.position(info.offset)
                                pcm.limit(info.offset + info.size)
                                pcm.get(copy)
                                pending = ByteBuffer.wrap(copy)
                                pendingPts = info.presentationTimeUs
                            }
                            decoder.releaseOutputBuffer(outIndex, false)
                        }
                    }

                    // 3. Feed the pending PCM into the encoder as inputs free up.
                    if (pending != null) {
                        val inIndex = encoder.dequeueInputBuffer(TIMEOUT_US)
                        if (inIndex >= 0) {
                            val dst = encoder.getInputBuffer(inIndex)!!
                            val src = pending!!
                            val n = minOf(src.remaining(), dst.capacity())
                            val slice = ByteArray(n)
                            src.get(slice)
                            dst.clear()
                            dst.put(slice)
                            encoder.queueInputBuffer(inIndex, 0, n, pendingPts, 0)
                            pendingPts += (n / bytesPerUs).toLong()
                            if (!src.hasRemaining()) pending = null
                        }
                    } else if (decoderDone && !encoderEosSent) {
                        // All PCM consumed: tell the encoder the stream is over.
                        val inIndex = encoder.dequeueInputBuffer(TIMEOUT_US)
                        if (inIndex >= 0) {
                            encoder.queueInputBuffer(
                                inIndex, 0, 0, pendingPts, MediaCodec.BUFFER_FLAG_END_OF_STREAM,
                            )
                            encoderEosSent = true
                        }
                    }

                    // 4. Drain the AAC the encoder has ready, buffering it.
                    var available = true
                    while (available) {
                        val outIndex = encoder.dequeueOutputBuffer(info, TIMEOUT_US)
                        when {
                            outIndex == MediaCodec.INFO_TRY_AGAIN_LATER -> available = false
                            outIndex == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED ->
                                encoderFormat = encoder.outputFormat
                            outIndex >= 0 -> {
                                val out = encoder.getOutputBuffer(outIndex)!!
                                if (info.flags and MediaCodec.BUFFER_FLAG_CODEC_CONFIG != 0) {
                                    info.size = 0
                                }
                                if (info.size > 0) {
                                    val bytes = ByteArray(info.size)
                                    out.position(info.offset)
                                    out.limit(info.offset + info.size)
                                    out.get(bytes)
                                    frames.add(Frame(bytes, info.presentationTimeUs, info.flags))
                                }
                                encoder.releaseOutputBuffer(outIndex, false)
                                if (info.flags and MediaCodec.BUFFER_FLAG_END_OF_STREAM != 0) {
                                    encoderDone = true
                                    available = false
                                }
                            }
                        }
                    }
                }

                decoder.stop(); decoder.release()
                encoder.stop(); encoder.release()
                extractor.release()
                return ReencodedAudio(encoderFormat ?: outFormat, frames)
            }
        }
    }
}
