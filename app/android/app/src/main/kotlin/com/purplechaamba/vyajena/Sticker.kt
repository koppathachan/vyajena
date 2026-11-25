package com.purplechaamba.vyajena

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import com.google.mlkit.vision.common.InputImage
import com.google.mlkit.vision.segmentation.subject.SubjectSegmentation
import com.google.mlkit.vision.segmentation.subject.SubjectSegmenterOptions
import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel
import kotlin.math.max

// photo-sticker: on-device subject cutout on Android (Segmentation seam and RGBA
// contract; design S2). Produces a straight-alpha, 8-bit RGBA image (sRGB,
// R,G,B,A byte order, row stride width*4) with the background fully transparent
// and the subject keeping its colour, using ML Kit Subject Segmentation. The
// photo never leaves the device; the model is delivered on demand by Google Play
// services, not bundled in the APK. This plugin ONLY produces the RGBA — it never
// encodes, encrypts, uploads or touches keys; the Rust core does all of that.
//
// The method channel ("vyajena/sticker"):
// - "available" -> Bool: whether subject segmentation can run here.
// - "cutout({photo: bytes})" -> {rgba, width, height}, or a MethodChannel error
//   with code "unsupported" or "noSubject".
object Sticker {
    const val CHANNEL = "vyajena/sticker"

    // The long edge the input is scaled to before segmentation, matching the
    // core's MAX_IMAGE_EDGE, so the RGBA crossing the channel is bounded; the
    // core downscales and trims again regardless.
    private const val MAX_EDGE = 2048

    // A pixel counts as "subject" when its alpha is at least this; used only to
    // decide whether any subject was found at all.
    private const val SUBJECT_ALPHA = 128

    fun handle(call: MethodCall, result: MethodChannel.Result) {
        when (call.method) {
            "available" -> result.success(true)
            "cutout" -> cutout(call, result)
            else -> result.notImplemented()
        }
    }

    private fun cutout(call: MethodCall, result: MethodChannel.Result) {
        val photo = call.argument<ByteArray>("photo")
        if (photo == null) {
            result.error("noSubject", "no photo", null)
            return
        }
        // The picker already bakes the EXIF orientation into the JPEG, so the
        // contract's "orientation already applied" holds.
        val decoded = BitmapFactory.decodeByteArray(photo, 0, photo.size)
        if (decoded == null) {
            result.error("noSubject", "cannot read the photo", null)
            return
        }
        val bitmap = downscale(decoded)

        val options = SubjectSegmenterOptions.Builder()
            .enableForegroundBitmap()
            .build()
        val segmenter = SubjectSegmentation.getClient(options)
        val input = InputImage.fromBitmap(bitmap, 0)
        segmenter.process(input)
            .addOnSuccessListener { segmentation ->
                val foreground = segmentation.foregroundBitmap
                if (foreground == null) {
                    result.error("noSubject", "no subject found", null)
                } else {
                    try {
                        val rgba = straightAlphaRgba(foreground)
                        if (rgba == null) {
                            result.error("noSubject", "no subject found", null)
                        } else {
                            result.success(
                                mapOf(
                                    "rgba" to rgba,
                                    "width" to foreground.width,
                                    "height" to foreground.height,
                                ),
                            )
                        }
                    } catch (error: Throwable) {
                        result.error("noSubject", error.message, null)
                    }
                }
                segmenter.close()
            }
            .addOnFailureListener { error ->
                // A download or runtime failure of the API: treat as unsupported
                // so the app offers to send the whole photo instead.
                result.error("unsupported", error.message, null)
                segmenter.close()
            }
    }

    /** `bitmap` scaled down so its long edge is at most [MAX_EDGE]; never up. */
    private fun downscale(bitmap: Bitmap): Bitmap {
        val edge = max(bitmap.width, bitmap.height)
        if (edge <= MAX_EDGE) return bitmap
        val scale = MAX_EDGE.toDouble() / edge
        val w = (bitmap.width * scale).toInt().coerceAtLeast(1)
        val h = (bitmap.height * scale).toInt().coerceAtLeast(1)
        return Bitmap.createScaledBitmap(bitmap, w, h, true)
    }

    /**
     * `bitmap` (ARGB_8888, straight alpha) as R,G,B,A bytes with no row padding.
     * `null` when no pixel is opaque enough to be a subject.
     */
    private fun straightAlphaRgba(bitmap: Bitmap): ByteArray? {
        val width = bitmap.width
        val height = bitmap.height
        val argb = IntArray(width * height)
        bitmap.getPixels(argb, 0, width, 0, 0, width, height)
        val rgba = ByteArray(width * height * 4)
        var any = false
        var i = 0
        for (pixel in argb) {
            val a = (pixel ushr 24) and 0xFF
            rgba[i] = ((pixel ushr 16) and 0xFF).toByte() // R
            rgba[i + 1] = ((pixel ushr 8) and 0xFF).toByte() // G
            rgba[i + 2] = (pixel and 0xFF).toByte() // B
            rgba[i + 3] = a.toByte() // A (straight)
            if (a >= SUBJECT_ALPHA) any = true
            i += 4
        }
        return if (any) rgba else null
    }
}
