package com.purplechaamba.vyajena

import android.os.Handler
import android.os.Looper
import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import io.flutter.plugin.common.MethodChannel

/**
 * FCM's callbacks (app-ui: Push notifications). The server's ping is a
 * notification message: Play services shows "New message" itself while the
 * app is in the background or closed, so nothing here handles it. A rotated
 * token goes to Dart while the app runs; otherwise the next start reads it.
 */
class VyajenaMessagingService : FirebaseMessagingService() {
    override fun onNewToken(token: String) {
        Handler(Looper.getMainLooper()).post {
            PushChannel.channel?.invokeMethod("tokenChanged", token)
        }
    }

    // In the foreground the app is connected and has the message already;
    // no notification is shown.
    override fun onMessageReceived(message: RemoteMessage) {}
}

/** The `vyajena/push` channel while a Flutter engine runs. */
object PushChannel {
    @Volatile
    var channel: MethodChannel? = null
}
