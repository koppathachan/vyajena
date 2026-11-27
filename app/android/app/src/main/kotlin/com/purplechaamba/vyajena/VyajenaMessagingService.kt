package com.purplechaamba.vyajena

import android.os.Handler
import android.os.Looper
import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import io.flutter.plugin.common.MethodChannel

/**
 * FCM's callbacks (app-ui: Push notifications; smarter-notifications). The
 * server's ping is a high-priority data message that carries nothing but a
 * wake flag: [onMessageReceived] runs the app's side of it ([WakeRunner]), which
 * catches up on the device and posts a metadata-only local notification. If the
 * wake cannot run (the app was force-stopped, or the phone kills it), nothing is
 * shown and the next app open catches up. A rotated token goes to Dart while the
 * app runs; otherwise the next start reads it.
 */
class VyajenaMessagingService : FirebaseMessagingService() {
    override fun onNewToken(token: String) {
        Handler(Looper.getMainLooper()).post {
            PushChannel.channel?.invokeMethod("tokenChanged", token)
        }
    }

    // FCM calls this on a worker thread and gives the app a few seconds.
    override fun onMessageReceived(message: RemoteMessage) {
        if (message.data["wake"] != "1") return
        WakeRunner.run(applicationContext)
    }
}

/** The `vyajena/push` channel while a Flutter engine runs. */
object PushChannel {
    @Volatile
    var channel: MethodChannel? = null
}
