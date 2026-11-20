package com.bandear.android

import android.content.Context
import android.net.wifi.WifiManager

/** The tag the lock carries in the phone's diagnostics (`adb shell dumpsys wifi`). */
private const val WIFI_LOCK_TAG = "BandEar:receive"

/**
 * The phone's Wi-Fi held in its low-latency mode while a receive session runs
 * (`WifiManager.WIFI_MODE_FULL_LOW_LATENCY`, API 29, the app's minimum).
 *
 * The mode keeps the radio out of power save, which can otherwise hold the desk's datagrams until
 * the radio wakes and deliver them in late bunches. Android applies it only while the phone is
 * connected to an access point, the app is in the foreground and the screen is on; otherwise the
 * lock is held but changes nothing. The older `WIFI_MODE_FULL_HIGH_PERF` is not a fallback: every
 * release the app runs on has this mode, and from API 34 the older one asks for this one anyway.
 */
internal class LowLatencyWifiLock(context: Context) {
    private val wifi: WifiManager? = context.applicationContext.getSystemService(WifiManager::class.java)

    /**
     * Takes a lock of its own for one session, so a session still ending never gives back a newer
     * one's; closing what it answers gives it back. A phone without Wi-Fi holds nothing.
     */
    fun hold(): AutoCloseable {
        val lock = wifi?.createWifiLock(WifiManager.WIFI_MODE_FULL_LOW_LATENCY, WIFI_LOCK_TAG)
            ?: return AutoCloseable {}
        // One release gives it back, however the session ends.
        lock.setReferenceCounted(false)
        lock.acquire()
        return AutoCloseable { lock.release() }
    }
}
