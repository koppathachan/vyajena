# R8 rules for the release build (app-release design D6).
#
# The Rust core (libbridge.so) exports Java_io_crates_keyring_Keyring_*
# JNI functions, so the class name, its companion and its native method must
# survive shrinking unchanged; MainActivity calls it before the first bridge
# call to hand the core the app's Context for the Android Keystore.
-keep class io.crates.keyring.Keyring { *; }
-keep class io.crates.keyring.Keyring$Companion { *; }
-keepclasseswithmembernames,includedescriptorclasses class * {
    native <methods>;
}
