package com.bandear.android

/**
 * The playback rules that stay on Android: how the `AudioTrack` is opened so the desk's longest
 * buffer fits without costing latency, and the bounds of the desk's buffer setting the control
 * channel reads.
 *
 * The rules between a packet and the track (the cushion, its adaptation, the playback backlog and
 * the cushion reported) are the shared core's (`crates/bandear-core`), reached through
 * [ReceiveSession].
 */
object PlaybackLatency {
    /** The playback track is always opened as interleaved stereo. */
    const val CHANNELS = 2

    /** The desk's buffer when it leaves the cushion to the phone (Auto); any other is fixed, in ms. */
    const val DESK_BUFFER_AUTO = 0

    /** The fixed buffer lengths the desk may set, in milliseconds (the server checks the same). */
    const val MIN_FIXED_BUFFER_MS = 5
    const val MAX_FIXED_BUFFER_MS = 500

    /**
     * The capacity to open the track with, in frames: the longest fixed buffer
     * ([MAX_FIXED_BUFFER_MS]) at [sampleRate], in whole packets of [packetFrames] so its last packet
     * fits too (just the frames when the packet size is unknown). The capacity is only the most the
     * buffer may be set to; what the track keeps queued is the cushion the shared core sets, so
     * a large capacity costs memory (about 94 KB at 48 kHz), not latency. This is the platform's
     * documented way to tune latency: open with room, then set the buffer size down.
     */
    fun trackCapacityFrames(sampleRate: Int, packetFrames: Int): Int {
        val frames = ceilDiv(MAX_FIXED_BUFFER_MS.toLong() * sampleRate.coerceAtLeast(0), 1_000L)
        if (packetFrames <= 0) return frames.toInt()
        return (ceilDiv(frames, packetFrames.toLong()) * packetFrames).toInt()
    }

    private fun ceilDiv(value: Long, divisor: Long): Long = (value + divisor - 1) / divisor

    /**
     * How far above the wanted buffer the platform may round it and the track still count as
     * shrunk: it sizes buffers in its own bursts (4 ms on a Galaxy S23).
     */
    const val SHRINK_TOLERANCE_MS = 5

    /**
     * Whether a track opened with room for the longest fixed buffer ([trackCapacityFrames]) keeps
     * Auto's latency, once its buffer was set to [wantedFrames] and it reports [appliedFrames].
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
