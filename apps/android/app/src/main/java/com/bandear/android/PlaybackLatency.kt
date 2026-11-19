package com.bandear.android

/**
 * The playback rules that stay on Android: whether the `AudioTrack`, opened with room for the desk's
 * longest buffer, keeps Auto's latency, and the desk's buffer setting the control channel reads.
 *
 * The rules between a packet and the track (the cushion, its adaptation, the playback backlog and
 * the cushion reported) are the shared core's (`crates/bandear-core`), reached through
 * [ReceiveSession]; so is the room the track opens with
 * ([com.bandear.android.core.playbackCapacityFrames]).
 */
object PlaybackLatency {
    /** The playback track is always opened as interleaved stereo. */
    const val CHANNELS = 2

    /** The desk's buffer when it leaves the cushion to the phone (Auto); any other is fixed, in ms. */
    const val DESK_BUFFER_AUTO = 0

    private fun ceilDiv(value: Long, divisor: Long): Long = (value + divisor - 1) / divisor

    /**
     * How far above the wanted buffer the platform may round it and the track still count as
     * shrunk: it sizes buffers in its own bursts (4 ms on a Galaxy S23).
     */
    const val SHRINK_TOLERANCE_MS = 5

    /**
     * Whether a track opened with room for the longest fixed buffer
     * ([com.bandear.android.core.playbackCapacityFrames]) keeps Auto's latency, once its buffer was
     * set to [wantedFrames] and it reports [appliedFrames].
     *
     * The large capacity only costs no latency if the platform honours the size set: kept when the
     * buffer came down to the wanted size within [SHRINK_TOLERANCE_MS] at [sampleRate], or at least
     * as low as a track opened at the platform minimum would queue ([minimumFrames], how the track
     * was opened before the desk could set a buffer), so it is never worse than before. Otherwise,
     * and when the size could not be set or read (an error code, 0), the caller reopens the track
     * at the minimum and lets fixed buffers go only as long as that track holds.
     */
    fun keepsLowLatency(wantedFrames: Int, appliedFrames: Int, minimumFrames: Int, sampleRate: Int): Boolean {
        if (appliedFrames <= 0) return false
        val tolerance = ceilDiv(SHRINK_TOLERANCE_MS.toLong() * sampleRate.coerceAtLeast(0), 1_000L)
        return appliedFrames.toLong() <= maxOf(wantedFrames.toLong() + tolerance, minimumFrames.toLong())
    }
}
