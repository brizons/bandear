package com.bandear.android

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * When a Connect asks for the notification permission the playback notification needs
 * ([NotificationPrompt]): on Android 13 and later, while it is not granted, and only the first time.
 */
class NotificationPromptTest {
    @Test
    fun theFirstConnectOnAndroid13AsksWhileThePermissionIsNotGranted() {
        assertTrue(NotificationPrompt.shouldAsk(sdk = 33, granted = false, askedBefore = false))
        assertTrue(NotificationPrompt.shouldAsk(sdk = 35, granted = false, askedBefore = false))
    }

    @Test
    fun beforeAndroid13NotificationsNeedNoPermission() {
        assertFalse(NotificationPrompt.shouldAsk(sdk = 32, granted = false, askedBefore = false))
        assertFalse(NotificationPrompt.shouldAsk(sdk = 29, granted = false, askedBefore = false))
    }

    @Test
    fun aGrantedPermissionIsNeverAskedFor() {
        assertFalse(NotificationPrompt.shouldAsk(sdk = 35, granted = true, askedBefore = false))
    }

    @Test
    fun onlyTheFirstConnectAsks() {
        assertFalse(NotificationPrompt.shouldAsk(sdk = 35, granted = false, askedBefore = true))
    }
}
