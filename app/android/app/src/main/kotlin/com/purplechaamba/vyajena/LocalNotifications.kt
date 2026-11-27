package com.purplechaamba.vyajena

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.os.Build
import androidx.core.app.NotificationCompat

/**
 * The local notifications built from the core's metadata-only summary
 * (smarter-notifications; local-notifications: The device builds the
 * notification locally). The words come from Dart (`notification_text.dart`), so
 * iOS and Android show the same thing: this only posts them, one per
 * conversation inside one Android group, plus the group's summary line. Nothing
 * here knows a sender or a message body.
 */
object LocalNotifications {
    const val CHANNEL = "messages"
    private const val GROUP = "vyajena-messages"
    private const val SUMMARY_ID = 1
    const val EXTRA_TAPPED = "vyajena.notificationTapped"
    const val EXTRA_CHAT = "vyajena.notificationChat"

    /** One line to show; [chatId] is the conversation a tap opens, if any. */
    class Line(val key: String, val text: String, val chatId: Int?)

    fun ensureChannel(context: Context) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val manager = context.getSystemService(NotificationManager::class.java) ?: return
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL, "Messages", NotificationManager.IMPORTANCE_HIGH),
        )
    }

    /** The id of the notification for [key]: stable, so a repeat replaces it. */
    private fun idOf(key: String): Int = 2 + (key.hashCode() and 0x3fffffff)

    private fun opening(context: Context, chatId: Int?, requestCode: Int): PendingIntent {
        val intent = Intent(context, MainActivity::class.java).apply {
            action = "com.purplechaamba.vyajena.OPEN"
            putExtra(EXTRA_TAPPED, true)
            if (chatId != null) putExtra(EXTRA_CHAT, chatId)
            flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP or
                Intent.FLAG_ACTIVITY_CLEAR_TOP
        }
        return PendingIntent.getActivity(
            context,
            requestCode,
            intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
    }

    private fun base(context: Context, text: String, tap: PendingIntent) =
        NotificationCompat.Builder(context, CHANNEL)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentText(text)
            .setContentIntent(tap)
            .setAutoCancel(true)
            .setGroup(GROUP)
            .setPriority(NotificationCompat.PRIORITY_HIGH)

    /**
     * Shows [lines] as the app's notifications, replacing what was shown, with
     * [combined] as the group's summary line and [singleChat] as what a tap on
     * the summary opens. Does nothing without notification permission.
     */
    fun show(context: Context, lines: List<Line>, combined: String, singleChat: Int?) {
        val manager = context.getSystemService(NotificationManager::class.java) ?: return
        if (!manager.areNotificationsEnabled()) return
        ensureChannel(context)
        val keep = lines.map { idOf(it.key) }.toSet() + SUMMARY_ID
        // What an earlier wake showed and this one no longer does.
        manager.activeNotifications
            .filter { it.packageName == context.packageName && it.id !in keep }
            .forEach { manager.cancel(it.id) }
        for (line in lines) {
            val id = idOf(line.key)
            manager.notify(
                id,
                base(context, line.text, opening(context, line.chatId, id)).build(),
            )
        }
        manager.notify(
            SUMMARY_ID,
            base(context, combined, opening(context, singleChat, SUMMARY_ID))
                .setGroupSummary(true)
                .build(),
        )
    }

    /** Removes the notification of the conversation [chatId], and the summary if it was the last. */
    fun clear(context: Context, chatId: Int) {
        val manager = context.getSystemService(NotificationManager::class.java) ?: return
        manager.cancel(idOf("chat-$chatId"))
        val rest = manager.activeNotifications.count {
            it.packageName == context.packageName && it.id != SUMMARY_ID
        }
        if (rest == 0) manager.cancel(SUMMARY_ID)
    }

    fun clearAll(context: Context) {
        context.getSystemService(NotificationManager::class.java)?.cancelAll()
    }
}
