package com.bandear.android

/**
 * When a Connect asks for the notification permission that the playback notification
 * ([PlaybackService]) needs to show: Android 13 (API 33) made it a runtime permission.
 *
 * The first Connect asks, while it is not granted; after that the musician decides in the phone's
 * settings, so no later Connect asks again. The session plays in the background either way: without
 * the permission Android keeps the notification out of the shade and lists the app in its task
 * manager instead. Pure, so the rule is an ordinary JVM test; `MainActivity` asks.
 */
internal object NotificationPrompt {
    /** The first Android version whose notifications need the runtime permission. */
    const val FIRST_SDK = 33

    /** Whether a Connect on Android [sdk] asks, the permission [granted] or not, [askedBefore] or not. */
    fun shouldAsk(sdk: Int, granted: Boolean, askedBefore: Boolean): Boolean =
        sdk >= FIRST_SDK && !granted && !askedBefore
}
