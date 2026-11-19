package com.bandear.android

import com.bandear.android.core.LinkStatsReport as CoreLinkStatsReport

/**
 * What the phone experienced over one report interval: the counts the server classifies the link
 * from. [packetsReceived] counts the packets played, so a dropped duplicate or late packet is not
 * received twice, and received plus lost is the stretch of the stream the interval covered.
 *
 * The extras the phone announces with `stats.v2` ([com.bandear.android.core.ClientCapability.STATS_V2]),
 * in milliseconds, are
 * `NaN` when not measured:
 * - [jitterMs]: how much the packets' arrival varied over the interval, the mean distance of each
 *   packet's spacing from the smoothed spacing (the deviation the receiver's own jitter smooths).
 *   Not measured when no two packets arrived in the interval.
 * - [cushionMs]: the playback cushion the phone aims for at the interval's end (the buffer it asks
 *   the track for), not measured before the first packet opened the track.
 * - [backlogMs]: the audio queued for playback at the interval's end, as last read from the track.
 *
 * [recovered] (announced with `redundancy.recovered`,
 * [com.bandear.android.core.ClientCapability.REDUNDANCY_RECOVERED]) counts
 * the packets over the interval that were played after filling a gap: each arrived for the first
 * time after a newer one, while the receiver held that newer one back for the desk's delayed
 * copies (the shared core's reorder hold), which only a delayed redundant copy or Wi-Fi reordering does.
 * They are in [packetsReceived], not in [packetsLost]. A late packet that could not be played (no
 * hold, or past it) is not counted, nor is an immediate copy, which cannot be told from its
 * original.
 *
 * The shared core builds the report ([com.bandear.android.core.LinkStatsReport], where a value not
 * measured is null); [ReceiveSession] hands it over as this one, which the screens read, and the
 * control channel hands it back ([toCoreReport]) for the core to write its `stats` message.
 */
data class LinkStatsReport(
    val intervalMs: Long,
    val packetsReceived: Long,
    val packetsLost: Long,
    val underruns: Long,
    val receiveTimeouts: Long,
    val jitterMs: Double = Double.NaN,
    val cushionMs: Double = Double.NaN,
    val backlogMs: Double = Double.NaN,
    val recovered: Long = 0L,
)

/** The report as the shared core reads it: a value not measured (`NaN`) is null. */
internal fun LinkStatsReport.toCoreReport() = CoreLinkStatsReport(
    intervalMs = intervalMs,
    packetsReceived = packetsReceived,
    packetsLost = packetsLost,
    underruns = underruns,
    receiveTimeouts = receiveTimeouts,
    jitterMs = jitterMs.takeUnless { it.isNaN() },
    cushionMs = cushionMs.takeUnless { it.isNaN() },
    backlogMs = backlogMs.takeUnless { it.isNaN() },
    recovered = recovered,
)
