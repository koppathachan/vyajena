package com.purplechaamba.vyajena

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import com.google.firebase.FirebaseApp
import com.google.firebase.messaging.FirebaseMessaging
import io.crates.keyring.Keyring
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel
import java.io.File

class MainActivity : FlutterActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        // The Rust core's Android Keystore store needs the app's Context
        // before Dart makes its first bridge call (flutter-app design D4).
        Keyring.initializeNdkContext(applicationContext)
        super.onCreate(savedInstanceState)
        createMessagesChannel()
    }

    private var pendingPermission: MethodChannel.Result? = null

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "vyajena/platform")
            .setMethodCallHandler { call, result ->
                when (call.method) {
                    "dataDir" -> result.success(coreDataDir().path)
                    "share" -> {
                        val text = call.argument<String>("text")
                        if (text == null) {
                            result.error("share", "no text", null)
                        } else {
                            share(text)
                            result.success(null)
                        }
                    }
                    else -> result.notImplemented()
                }
            }
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, Sticker.CHANNEL)
            .setMethodCallHandler { call, result -> Sticker.handle(call, result) }
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, VideoNormalize.CHANNEL)
            .setMethodCallHandler { call, result ->
                VideoNormalize.handle(call.method, call.arguments as? Map<*, *>, result)
            }
        val push = MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "vyajena/push")
        PushChannel.channel = push
        push.setMethodCallHandler { call, result ->
            when (call.method) {
                "ensurePermission" -> ensurePermission(result)
                "token" -> token(result)
                "deleteToken" -> deleteToken(result)
                "clearNotifications" -> {
                    getSystemService(NotificationManager::class.java)?.cancelAll()
                    result.success(null)
                }
                else -> result.notImplemented()
            }
        }
    }

    override fun cleanUpFlutterEngine(flutterEngine: FlutterEngine) {
        PushChannel.channel = null
        super.cleanUpFlutterEngine(flutterEngine)
    }

    // --- Push (app-ui: Push notifications; push design D8, D9) ---

    /** Firebase started from the build's google-services.json resources. */
    private fun pushReady(): Boolean = FirebaseApp.getApps(this).isNotEmpty()

    private fun notificationsAllowed(): Boolean =
        getSystemService(NotificationManager::class.java)?.areNotificationsEnabled() == true

    /** The channel FCM shows "New message" on (the manifest's default). */
    private fun createMessagesChannel() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val channel = NotificationChannel(
            "messages",
            "Messages",
            NotificationManager.IMPORTANCE_HIGH,
        )
        getSystemService(NotificationManager::class.java)?.createNotificationChannel(channel)
    }

    /** Asks once per install (Android 13+), then reports whether allowed. */
    private fun ensurePermission(result: MethodChannel.Result) {
        if (!pushReady()) {
            result.success(false)
            return
        }
        if (Build.VERSION.SDK_INT >= 33 &&
            checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) !=
            PackageManager.PERMISSION_GRANTED
        ) {
            val prefs = getSharedPreferences("push", MODE_PRIVATE)
            if (!prefs.getBoolean("asked", false) && pendingPermission == null) {
                prefs.edit().putBoolean("asked", true).apply()
                pendingPermission = result
                requestPermissions(
                    arrayOf(Manifest.permission.POST_NOTIFICATIONS),
                    NOTIFICATIONS_REQUEST,
                )
                return
            }
        }
        result.success(notificationsAllowed())
    }

    override fun onRequestPermissionsResult(
        requestCode: Int,
        permissions: Array<out String>,
        grantResults: IntArray,
    ) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode == NOTIFICATIONS_REQUEST) {
            pendingPermission?.success(notificationsAllowed())
            pendingPermission = null
        }
    }

    private fun token(result: MethodChannel.Result) {
        if (!pushReady()) {
            result.success(null)
            return
        }
        FirebaseMessaging.getInstance().token.addOnCompleteListener { task ->
            result.success(if (task.isSuccessful) task.result else null)
        }
    }

    private fun deleteToken(result: MethodChannel.Result) {
        if (!pushReady()) {
            result.success(null)
            return
        }
        FirebaseMessaging.getInstance().deleteToken().addOnCompleteListener {
            result.success(null)
        }
    }

    private companion object {
        const val NOTIFICATIONS_REQUEST = 7301
    }

    // The system share sheet for an invite, link request or link approval
    // the user chose to share (flutter-app design D5). Never message text.
    private fun share(text: String) {
        val send = Intent(Intent.ACTION_SEND).apply {
            type = "text/plain"
            putExtra(Intent.EXTRA_TEXT, text)
        }
        startActivity(Intent.createChooser(send, null))
    }

    // The folder the Rust core keeps its database in (app-ui: Data on the
    // phone): app-private storage that Android never backs up or transfers
    // (noBackupFilesDir), on top of allowBackup="false". Keys are not here:
    // the core keeps them in the Android Keystore.
    private fun coreDataDir(): File {
        val dir = File(noBackupFilesDir, "core")
        dir.mkdirs()
        return dir
    }
}
