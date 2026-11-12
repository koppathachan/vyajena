package io.crates.keyring

import android.content.Context

// Hands the app's Context to the Rust core's ndk-context, which the
// android-native-keyring-store crate needs (flutter-app design D4). The JNI
// function is exported by libbridge.so; its name fixes this package and class.
// Platform glue only: no key passes through here.
class Keyring {
    companion object {
        init {
            System.loadLibrary("bridge")
        }

        external fun initializeNdkContext(context: Context)
    }
}
