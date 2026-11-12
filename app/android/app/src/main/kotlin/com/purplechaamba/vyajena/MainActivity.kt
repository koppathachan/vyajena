package com.purplechaamba.vyajena

import android.os.Bundle
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
    }

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "vyajena/platform")
            .setMethodCallHandler { call, result ->
                if (call.method == "dataDir") {
                    result.success(coreDataDir().path)
                } else {
                    result.notImplemented()
                }
            }
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
