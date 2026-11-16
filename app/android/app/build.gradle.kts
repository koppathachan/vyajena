import java.io.File
import java.util.Properties

plugins {
    id("com.android.application")
    // The Flutter Gradle Plugin must be applied after the Android and Kotlin Gradle plugins.
    id("dev.flutter.flutter-gradle-plugin")
}

/** The owner's upload key, read from outside the repo (app-release design D3). */
class UploadKey(
    val storeFile: File,
    val storePassword: String,
    val keyAlias: String,
    val keyPassword: String,
)

/** Why there is no upload key, when there is none. */
var uploadKeyProblem = ""

/**
 * Reads the key properties file named by the Gradle property
 * `vyajena.keyProperties` (default `~/.vyajena-release/key.properties`):
 * `storeFile` (absolute, or relative to that file), `keyAlias`, and
 * `storePassword`/`keyPassword`, which may instead come from the environment
 * (`VYAJENA_UPLOAD_STORE_PASSWORD`, `VYAJENA_UPLOAD_KEY_PASSWORD`, set from the
 * macOS Keychain by app/store/build-aab.sh) so no password sits in a file.
 */
val uploadKey: UploadKey? = run {
    val path = (project.findProperty("vyajena.keyProperties") as String?)
        ?: "${System.getProperty("user.home")}/.vyajena-release/key.properties"
    val file = File(path)
    if (!file.isFile) {
        uploadKeyProblem = "$path not found"
        return@run null
    }
    val props = Properties()
    file.inputStream().use { props.load(it) }
    fun value(name: String, env: String? = null): String? =
        props.getProperty(name)?.takeIf { it.isNotEmpty() }
            ?: env?.let { System.getenv(it) }?.takeIf { it.isNotEmpty() }
    val store = value("storeFile")
    val alias = value("keyAlias")
    val storePassword = value("storePassword", "VYAJENA_UPLOAD_STORE_PASSWORD")
    val keyPassword = value("keyPassword", "VYAJENA_UPLOAD_KEY_PASSWORD")
    if (store == null || alias == null || storePassword == null || keyPassword == null) {
        uploadKeyProblem = "$path lacks storeFile, keyAlias or a password"
        return@run null
    }
    val storeFile = File(store).let { if (it.isAbsolute) it else File(file.parentFile, store) }
    if (!storeFile.isFile) {
        uploadKeyProblem = "keystore ${storeFile.path} not found"
        return@run null
    }
    UploadKey(storeFile, storePassword, alias, keyPassword)
}

/**
 * The Firebase settings for push (push design D8), read from the owner's
 * `google-services.json` outside the repo (Gradle property
 * `vyajena.googleServices`, default `~/.vyajena-push/google-services.json`;
 * owner, 2026-11-16: not committed). They become the four string resources
 * the Google Services plugin would write, so Firebase starts itself. Without
 * the file the app builds and has no push (CI, emulators, contributors).
 */
val firebaseValues: Map<String, String>? = run {
    val path = (project.findProperty("vyajena.googleServices") as String?)
        ?: "${System.getProperty("user.home")}/.vyajena-push/google-services.json"
    val file = File(path)
    if (!file.isFile) return@run null
    val json = groovy.json.JsonSlurper().parse(file) as Map<*, *>
    val info = json["project_info"] as? Map<*, *>
        ?: throw GradleException("$path has no project_info")
    val client = (json["client"] as? List<*>).orEmpty().filterIsInstance<Map<*, *>>().firstOrNull {
        val clientInfo = it["client_info"] as? Map<*, *>
        val android = clientInfo?.get("android_client_info") as? Map<*, *>
        android?.get("package_name") == "com.purplechaamba.vyajena"
    } ?: throw GradleException("$path has no Android app com.purplechaamba.vyajena")
    val appId = (client["client_info"] as Map<*, *>)["mobilesdk_app_id"] as? String
    val apiKey = ((client["api_key"] as? List<*>)?.firstOrNull() as? Map<*, *>)?.get("current_key") as? String
    val sender = info["project_number"]?.toString()
    val projectId = info["project_id"] as? String
    if (appId == null || apiKey == null || sender == null || projectId == null) {
        throw GradleException("$path lacks mobilesdk_app_id, api_key, project_number or project_id")
    }
    mapOf(
        "google_app_id" to appId,
        "google_api_key" to apiKey,
        "gcm_defaultSenderId" to sender,
        "project_id" to projectId,
    )
}

android {
    namespace = "com.purplechaamba.vyajena"
    compileSdk = flutter.compileSdkVersion
    ndkVersion = flutter.ndkVersion

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    defaultConfig {
        // The owner's app id (purplechaamba.com), flutter-app design D1.
        applicationId = "com.purplechaamba.vyajena"
        // You can update the following values to match your application needs.
        // For more information, see: https://flutter.dev/to/review-gradle-config.
        minSdk = flutter.minSdkVersion
        targetSdk = flutter.targetSdkVersion
        versionCode = flutter.versionCode
        versionName = flutter.versionName
        firebaseValues?.forEach { (name, value) -> resValue("string", name, value) }
    }

    buildFeatures {
        resValues = true
    }

    signingConfigs {
        if (uploadKey != null) {
            create("release") {
                storeFile = uploadKey.storeFile
                storePassword = uploadKey.storePassword
                keyAlias = uploadKey.keyAlias
                keyPassword = uploadKey.keyPassword
            }
        }
    }

    buildTypes {
        release {
            // The owner's upload key only, never the debug key (app-release
            // design D3); a release build without it stops (see below).
            if (uploadKey != null) {
                signingConfig = signingConfigs.getByName("release")
            }
            // R8 keeps the JNI glue the Rust core needs (design D6).
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro",
            )
        }
    }
}

// app-release: Release signing keys stay with the owner. Any release task of
// this app needs the upload key; without it the build stops here instead of
// signing with the debug key. Debug and profile builds never need it.
gradle.taskGraph.whenReady {
    val wantsRelease = allTasks.any { it.project == project && it.name.endsWith("Release") }
    if (wantsRelease && uploadKey == null) {
        throw GradleException(
            "release signing: $uploadKeyProblem. See docs/release.md (the upload key " +
                "lives outside the repo, never in CI).",
        )
    }
    // push design D8: a store build without Firebase would ship without push.
    // -Pvyajena.allowNoPush=true is for throwaway release tests only.
    if (wantsRelease && firebaseValues == null && project.findProperty("vyajena.allowNoPush") != "true") {
        throw GradleException(
            "push: no ~/.vyajena-push/google-services.json (or -Pvyajena.googleServices). " +
                "See docs/release.md, \"Push\".",
        )
    }
}

dependencies {
    // FCM only; no Analytics (push design D8).
    implementation(platform("com.google.firebase:firebase-bom:35.2.0"))
    implementation("com.google.firebase:firebase-messaging")
}

kotlin {
    compilerOptions {
        jvmTarget = org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17
    }
}

flutter {
    source = "../.."
}
