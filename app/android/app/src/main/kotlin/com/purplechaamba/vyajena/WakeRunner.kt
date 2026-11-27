package com.purplechaamba.vyajena

import android.content.Context
import android.os.Handler
import android.os.Looper
import io.crates.keyring.Keyring
import io.flutter.FlutterInjector
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.embedding.engine.dart.DartExecutor
import io.flutter.plugin.common.MethodChannel
import java.io.File
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/**
 * Runs the app's side of a data wake (smarter-notifications; message-delivery:
 * Wake push payload is content-free and metadata-minimal). The server's push is
 * only "wake up": here the Rust core catches up on the device and Dart posts the
 * metadata-only notification. If the app's engine is alive it does the work; if
 * not, a headless engine runs `notificationWake`. FCM gives a pushed app only a
 * few seconds, so the wait is capped; past it the work is dropped and the next
 * app open catches up (design D5).
 */
object WakeRunner {
    private const val LIMIT_SECONDS = 9L

    /** Blocks the calling (FCM) thread until the wake is handled or the cap. */
    fun run(context: Context) {
        val app = context.applicationContext
        val done = CountDownLatch(1)
        val main = Handler(Looper.getMainLooper())
        var headless: FlutterEngine? = null
        main.post {
            val live = PushChannel.channel
            if (live != null) {
                // The app's engine is alive (open, or in the background): Dart
                // decides, and does nothing while the app is in front.
                live.invokeMethod("wake", null, object : MethodChannel.Result {
                    override fun success(result: Any?) = done.countDown()
                    override fun error(code: String, message: String?, details: Any?) = done.countDown()
                    override fun notImplemented() = done.countDown()
                })
            } else {
                headless = startHeadless(app, done)
                if (headless == null) done.countDown()
            }
        }
        done.await(LIMIT_SECONDS, TimeUnit.SECONDS)
        main.post { headless?.destroy() }
    }

    private fun startHeadless(app: Context, done: CountDownLatch): FlutterEngine? {
        return try {
            Keyring.initializeNdkContext(app)
            val loader = FlutterInjector.instance().flutterLoader()
            loader.startInitialization(app)
            loader.ensureInitializationComplete(app, null)
            val engine = FlutterEngine(app)
            val messenger = engine.dartExecutor.binaryMessenger
            // The core's data folder, the same one the app uses.
            MethodChannel(messenger, "vyajena/platform").setMethodCallHandler { call, result ->
                when (call.method) {
                    "dataDir" -> result.success(coreDataDir(app).path)
                    else -> result.notImplemented()
                }
            }
            MethodChannel(messenger, "vyajena/push").setMethodCallHandler { call, result ->
                when (call.method) {
                    "showNotifications" -> {
                        showNotifications(app, call.arguments as? Map<*, *>)
                        result.success(null)
                    }
                    "wakeDone" -> {
                        done.countDown()
                        result.success(null)
                    }
                    else -> result.notImplemented()
                }
            }
            engine.dartExecutor.executeDartEntrypoint(
                DartExecutor.DartEntrypoint(
                    loader.findAppBundlePath(),
                    "package:vyajena/push/wake_entry.dart",
                    "notificationWake",
                ),
            )
            engine
        } catch (error: Exception) {
            null
        }
    }

    /** The folder the Rust core keeps its database in (also MainActivity's). */
    fun coreDataDir(context: Context): File {
        val dir = File(context.noBackupFilesDir, "core")
        dir.mkdirs()
        return dir
    }

    /** Posts what Dart built: lines and a combined summary, words and ids only. */
    fun showNotifications(context: Context, arguments: Map<*, *>?) {
        if (arguments == null) return
        val lines = (arguments["lines"] as? List<*>)?.mapNotNull {
            val line = it as? Map<*, *> ?: return@mapNotNull null
            LocalNotifications.Line(
                line["key"] as? String ?: return@mapNotNull null,
                line["text"] as? String ?: return@mapNotNull null,
                (line["chatId"] as? Number)?.toInt(),
            )
        } ?: return
        LocalNotifications.show(
            context,
            lines,
            arguments["combined"] as? String ?: "",
            (arguments["singleChatId"] as? Number)?.toInt(),
        )
    }
}
