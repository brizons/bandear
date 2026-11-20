package com.bandear.android

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.util.Log
import com.bandear.android.ui.state.SessionKeepAlive

/**
 * The foreground service that keeps a monitor session playing with the screen locked or another
 * app in front.
 *
 * The session itself stays in `MonitorViewModel`, which starts and ends it. This service only keeps
 * the app in the foreground for it (type `mediaPlayback`), so that Android neither freezes the app
 * nor mutes its audio in the background, and it shows the ongoing notification that says so: the
 * server, a Disconnect action, and a tap that opens the app. Swiping the app away from Recents
 * finishes the Activity, which ends the session and, with it, this service.
 *
 * Everything here runs on the main thread: [ForegroundKeepAlive] starts the service from the Connect
 * that started the session and posts its end there.
 */
class PlaybackService : Service() {
    /** The latest start delivered: stopping with it leaves a newer start, not yet delivered, running. */
    private var lastStartId = 0

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        lastStartId = startId
        val kept = session
        // Android requires startForeground soon after startForegroundService, whatever follows. It may
        // refuse it for a plain start, the notification's Disconnect reaching a service no longer
        // running: that start just ends.
        try {
            startForeground(NOTIFICATION_ID, notification(this, kept?.server), ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK)
        } catch (error: Exception) {
            Log.w(TAG, "could not keep the session playing in the background", error)
            stopSelf(startId)
            return START_NOT_STICKY
        }
        running = this
        when {
            // The session ended before the service started.
            kept == null -> finish()
            intent?.action == ACTION_DISCONNECT -> kept.onDisconnect()
        }
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        if (running === this) running = null
        super.onDestroy()
    }

    private fun finish() {
        stopForeground(STOP_FOREGROUND_REMOVE)
        if (running === this) running = null
        stopSelfResult(lastStartId)
    }

    /** A session the service is kept for: its server, as the screens name it, and its Disconnect. */
    internal class KeptSession(val server: String, val onDisconnect: () -> Unit)

    internal companion object {
        private const val TAG = "BandEarPlayback"
        private const val CHANNEL_ID = "session"
        private const val NOTIFICATION_ID = 1
        private const val ACTION_DISCONNECT = "com.bandear.android.action.DISCONNECT"

        /** The session kept playing, or null; main thread only. */
        private var session: KeptSession? = null

        /** The service while it runs in the foreground; main thread only. */
        private var running: PlaybackService? = null

        /** Starts the service for [kept]; throws when the phone does not allow a foreground service now. */
        fun begin(context: Context, kept: KeptSession) {
            session = kept
            context.startForegroundService(Intent(context, PlaybackService::class.java))
        }

        /** Ends the service if [kept] is still the session it is kept for, never one started since. */
        fun end(kept: KeptSession) {
            if (session !== kept) return
            session = null
            running?.finish()
        }

        /** Shows the notification again, now that it may show: the permission was just granted. */
        fun refresh(context: Context) {
            val kept = session ?: return
            if (running == null) return
            context.getSystemService(NotificationManager::class.java)
                .notify(NOTIFICATION_ID, notification(context, kept.server))
        }

        private fun notification(context: Context, server: String?): Notification {
            val manager = context.getSystemService(NotificationManager::class.java)
            val channel = NotificationChannel(
                CHANNEL_ID,
                context.getString(R.string.playback_channel_name),
                NotificationManager.IMPORTANCE_LOW,
            )
            channel.description = context.getString(R.string.playback_channel_description)
            channel.setShowBadge(false)
            manager.createNotificationChannel(channel)
            val open = context.packageManager.getLaunchIntentForPackage(context.packageName)
                ?.let { PendingIntent.getActivity(context, 0, it, PendingIntent.FLAG_IMMUTABLE) }
            val disconnect = PendingIntent.getService(
                context,
                1,
                Intent(context, PlaybackService::class.java).setAction(ACTION_DISCONNECT),
                PendingIntent.FLAG_IMMUTABLE,
            )
            val builder = Notification.Builder(context, CHANNEL_ID)
                .setSmallIcon(R.drawable.ic_notification)
                .setContentTitle(context.getString(R.string.playback_notification_title))
                .setContentIntent(open)
                .setOngoing(true)
                .setShowWhen(false)
                .setCategory(Notification.CATEGORY_SERVICE)
                .addAction(
                    Notification.Action.Builder(null, context.getString(R.string.playback_disconnect), disconnect).build(),
                )
            if (server != null) builder.setContentText(context.getString(R.string.playback_notification_text, server))
            return builder.build()
        }
    }
}

/**
 * The production [SessionKeepAlive]: [PlaybackService] for each session.
 *
 * [start] runs on the main thread, from the Connect that started the session; [stop] may run on the
 * receive thread (a receiver that ended on its own ends the session there), so the service's end is
 * posted to the main thread, for the session it was asked for only. The ViewModel calls both under
 * its session lock, so they never overlap.
 */
internal class ForegroundKeepAlive(context: Context) : SessionKeepAlive {
    private val context = context.applicationContext
    private val main = Handler(Looper.getMainLooper())

    @Volatile private var kept: PlaybackService.KeptSession? = null

    override fun start(server: String, onDisconnect: () -> Unit) {
        val session = PlaybackService.KeptSession(server, onDisconnect)
        kept = session
        PlaybackService.begin(context, session)
    }

    override fun stop() {
        val session = kept ?: return
        kept = null
        main.post { PlaybackService.end(session) }
    }
}
