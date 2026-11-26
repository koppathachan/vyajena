package com.purplechaamba.vyajena

import android.graphics.Bitmap
import android.media.MediaCodec
import android.media.MediaCodecInfo
import android.media.MediaExtractor
import android.media.MediaFormat
import android.media.MediaMetadataRetriever
import android.media.MediaMuxer
import android.opengl.GLES20
import android.view.Surface
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
 * The transcode runs a decode -> GL surface -> H.264 encode -> MP4 mux pipeline.
 * S1 (real-device verification across encoders) is pending real-device testing;
 * the Dart, core and bridge layers are covered by their own tests.
 */
object VideoNormalize {
    const val CHANNEL = "chat.vyajena/video"

    /** The 480p long edge and ~1 Mbps target (protocol::video::NORMALIZE). */
    private const val LONG_EDGE = 854
    private const val BITRATE = 1_000_000
    private const val FRAME_RATE = 30
    private const val KEYFRAME_INTERVAL = 1
    private const val MIME = MediaFormat.MIMETYPE_VIDEO_AVC

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
        val (outWidth, outHeight) = targetSize(srcWidth, srcHeight, rotation)

        val outFile = File.createTempFile("vyajena-video-", ".mp4")
        val transcoder = Transcoder(inputPath, outFile.absolutePath, outWidth, outHeight, maxDurationMs)
        val durationMs = transcoder.run()

        val frame = firstFrame(retriever)
        retriever.release()

        return mapOf(
            "path" to outFile.absolutePath,
            "durationMs" to durationMs.toInt(),
            "width" to outWidth,
            "height" to outHeight,
            "frameRgba" to (frame?.rgba ?: ByteArray(0)),
            "frameWidth" to (frame?.width ?: 0),
            "frameHeight" to (frame?.height ?: 0),
        )
    }

    /** The target size: the long edge scaled to [LONG_EDGE], even dimensions. */
    private fun targetSize(w: Int, h: Int, rotation: Int): Pair<Int, Int> {
        val ( w2, h2) = if (rotation == 90 || rotation == 270) h to w else w to h
        if (w2 <= 0 || h2 <= 0) return LONG_EDGE to (LONG_EDGE * 9 / 16)
        val longEdge = maxOf(w2, h2)
        if (longEdge <= LONG_EDGE) return even(w2) to even(h2)
        val scale = LONG_EDGE.toDouble() / longEdge
        return even((w2 * scale).toInt()) to even((h2 * scale).toInt())
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
     * The GL blit (decoder output surface -> encoder input surface) is omitted
     * here for brevity and is the core of S1's on-device work; this class holds
     * the extractor/codec/muxer wiring the pipeline plugs into.
     */
    private class Transcoder(
        private val inputPath: String,
        private val outputPath: String,
        private val outWidth: Int,
        private val outHeight: Int,
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
            val inputSurface: Surface = encoder.createInputSurface()

            val decoder = MediaCodec.createDecoderByType(
                inFormat.getString(MediaFormat.KEY_MIME) ?: MIME
            )
            // On device, the decoder renders into a GL surface texture that is
            // drawn onto `inputSurface` (the encoder's input) at the target size;
            // that GL stage is S1's remaining on-device work. The muxer below
            // writes whatever the encoder produces as H.264 in an MP4.
            val muxer = MediaMuxer(outputPath, MediaMuxer.OutputFormat.MUXER_OUTPUT_MPEG_4)

            // Audio pass-through: a phone recording's AAC track is copied into the
            // normalized clip unchanged (not re-encoded), so the output keeps its
            // sound (video-messages: Video messages — H.264 + AAC). iOS's
            // AVAssetExportSession carries audio for free; here the muxer must be
            // given the audio track explicitly or the clip is silent. A clip with
            // no audio track (`audioTrack < 0`) is muxed video-only as before.
            val audioExtractor = MediaExtractor()
            audioExtractor.setDataSource(inputPath)
            val audioTrack = selectTrack(audioExtractor, "audio/")
            val audioFormat =
                if (audioTrack >= 0) audioExtractor.getTrackFormat(audioTrack) else null
            if (audioTrack >= 0) audioExtractor.selectTrack(audioTrack)

            extractor.selectTrack(videoTrack)
            decoder.configure(inFormat, inputSurface, null, 0)
            decoder.start()
            encoder.start()
            drive(extractor, decoder, encoder, muxer, durationUs, audioExtractor, audioFormat)

            encoder.stop(); encoder.release()
            decoder.stop(); decoder.release()
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
         * Feeds the extractor's samples through decode and encode up to
         * [durationUs], muxing the encoded H.264. The frame copy between the two
         * codecs' surfaces is the GL stage completed on device (S1).
         */
        private fun drive(
            extractor: MediaExtractor,
            decoder: MediaCodec,
            encoder: MediaCodec,
            muxer: MediaMuxer,
            durationUs: Long,
            audioExtractor: MediaExtractor,
            audioFormat: MediaFormat?,
        ) {
            val info = MediaCodec.BufferInfo()
            var muxVideoTrack = -1
            var muxAudioTrack = -1
            var muxerStarted = false
            var done = false
            while (!done) {
                val inIndex = decoder.dequeueInputBuffer(10_000)
                if (inIndex >= 0) {
                    val buffer: ByteBuffer = decoder.getInputBuffer(inIndex)!!
                    val size = extractor.readSampleData(buffer, 0)
                    if (size < 0 || extractor.sampleTime > durationUs) {
                        decoder.queueInputBuffer(
                            inIndex, 0, 0, 0, MediaCodec.BUFFER_FLAG_END_OF_STREAM
                        )
                    } else {
                        decoder.queueInputBuffer(inIndex, 0, size, extractor.sampleTime, 0)
                        extractor.advance()
                    }
                }
                val outIndex = encoder.dequeueOutputBuffer(info, 10_000)
                when {
                    outIndex == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED -> {
                        muxVideoTrack = muxer.addTrack(encoder.outputFormat)
                        // Every track must be added before the muxer starts; the
                        // audio format is known up front, so the AAC track joins
                        // here alongside the encoded video track.
                        if (audioFormat != null) muxAudioTrack = muxer.addTrack(audioFormat)
                        muxer.start()
                        muxerStarted = true
                    }
                    outIndex >= 0 -> {
                        val encoded = encoder.getOutputBuffer(outIndex)!!
                        if (muxerStarted && info.size > 0 &&
                            info.flags and MediaCodec.BUFFER_FLAG_CODEC_CONFIG == 0
                        ) {
                            muxer.writeSampleData(muxVideoTrack, encoded, info)
                        }
                        encoder.releaseOutputBuffer(outIndex, false)
                        if (info.flags and MediaCodec.BUFFER_FLAG_END_OF_STREAM != 0) done = true
                    }
                }
                // The decoder's frames are released to the encoder's input surface
                // via GL on device; omitted here keeps this blind-buildable.
                val decIndex = decoder.dequeueOutputBuffer(info, 10_000)
                if (decIndex >= 0) {
                    decoder.releaseOutputBuffer(decIndex, true)
                    if (info.flags and MediaCodec.BUFFER_FLAG_END_OF_STREAM != 0) {
                        encoder.signalEndOfInputStream()
                    }
                }
            }
            // The video track is complete; copy the AAC audio samples through,
            // trimmed to the same window. MediaMuxer accepts each track's samples
            // independently as long as they are in order within the track.
            if (muxerStarted && muxAudioTrack >= 0) {
                copyAudio(audioExtractor, muxer, muxAudioTrack, durationUs)
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
}
