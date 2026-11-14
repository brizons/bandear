package com.bandear.android

import com.bandear.android.core.MixSnapshot as CoreMixSnapshot
import kotlin.math.roundToInt

/**
 * A complete, device-local picture of a mix: the four per-channel arrays plus the master volume,
 * ceiling and mute.
 *
 * The type is deliberately pure. It imports nothing from Android and performs no IO, so its [moreOfMe]
 * derivation is an ordinary JVM test. [moreOfMe] never mutates the receiver: it returns a new
 * snapshot computed from the stored values, which is what lets the UI turn the control off and
 * restore the previous mix with nothing to save or replay. Its JSON is the shared core's
 * ([toCoreSnapshot], [serializeBanks]).
 */
data class MixSnapshot(
    val channelGains: IntArray = IntArray(0),
    val channelPans: IntArray = IntArray(0),
    val channelMutes: BooleanArray = BooleanArray(0),
    val channelSolos: BooleanArray = BooleanArray(0),
    val groupLevels: IntArray = IntArray(0),
    val volumePercent: Int = 100,
    val maxLevelPercent: Int = 100,
    val muted: Boolean = false,
) {
    /**
     * Applies the `More of me` rule: the musician's channel is pushed to [MORE_OF_ME_CHANNEL_GAIN]
     * and every other channel is scaled by [MORE_OF_ME_OTHER_FACTOR]. Everything else is untouched.
     *
     * Returns `null` when no valid channel is designated, so the caller can do nothing and tell the
     * musician instead of guessing. The receiver itself is never modified.
     */
    fun moreOfMe(musicianChannel: Int?): MixSnapshot? {
        if (musicianChannel == null || musicianChannel !in channelGains.indices) return null
        val gains = IntArray(channelGains.size) { index ->
            if (index == musicianChannel) {
                MORE_OF_ME_CHANNEL_GAIN
            } else {
                (channelGains[index] * MORE_OF_ME_OTHER_FACTOR).roundToInt()
            }
        }
        return copy(channelGains = gains)
    }

    companion object {
        const val MORE_OF_ME_CHANNEL_GAIN = 100
        const val MORE_OF_ME_OTHER_FACTOR = 0.5
    }
}

/** The snapshot as the shared core keeps it. */
internal fun MixSnapshot.toCoreSnapshot() = CoreMixSnapshot(
    channelGains = channelGains.map(Int::toLong),
    channelPans = channelPans.map(Int::toLong),
    channelMutes = channelMutes.toList(),
    channelSolos = channelSolos.toList(),
    groupLevels = groupLevels.map(Int::toLong),
    volumePercent = volumePercent.toLong(),
    maxLevelPercent = maxLevelPercent.toLong(),
    muted = muted,
)

/** The shared core's snapshot in the app's own values. */
internal fun CoreMixSnapshot.toMixSnapshot() = MixSnapshot(
    channelGains = channelGains.toIntArray(),
    channelPans = channelPans.toIntArray(),
    channelMutes = channelMutes.toBooleanArray(),
    channelSolos = channelSolos.toBooleanArray(),
    groupLevels = groupLevels.toIntArray(),
    volumePercent = volumePercent.toInt(),
    maxLevelPercent = maxLevelPercent.toInt(),
    muted = muted,
)

/** Every value as an `Int`: the app's own values always fit one. */
private fun List<Long>.toIntArray() = IntArray(size) { this[it].toInt() }
