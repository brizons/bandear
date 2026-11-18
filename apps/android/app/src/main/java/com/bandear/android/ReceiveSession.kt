package com.bandear.android

import com.bandear.android.core.DecodeError
import com.bandear.android.core.OutputAction
import com.bandear.android.core.OutputFormat
import com.bandear.android.core.OutputReadings
import com.bandear.android.core.PlaybackBufferSetting
import com.bandear.android.core.PlayerSettings
import com.bandear.android.core.ReceiveError
import com.bandear.android.core.ReceivePipeline
import com.bandear.android.core.ReceiverStats
import com.bandear.android.core.Wake
import com.bandear.android.core.WakeOutcome
import com.bandear.android.core.LinkStatsReport as CoreLinkStatsReport

private const val NANOS_PER_MILLISECOND = 1_000_000uL

/** The bytes of a PMON header, before the samples. */
private const val PMON_HEADER_BYTES = 20uL

/** The most datagrams read from what the socket already holds in one go, so a drain always ends. */
private const val MAX_DRAIN_DATAGRAMS = 512

/**
 * The audio output a [ReceiveSession] plays through: the app's `AudioTrack`, or a stand-in in the
 * tests. Called only from the receive thread.
 */
internal interface PlaybackOutput {
    /** The most frames the output can hold, as it reports it once open. */
    val capacityFrames: Long

    /** What the output says about itself now, read in one go. */
    fun readings(): OutputReadings

    /** Plays interleaved stereo PCM16 samples, two bytes each, little endian. */
    fun write(pcm16LeBytes: ByteArray)

    /** Asks the output to queue [frames] before it plays; answers the frames it applied, 0 when it cannot say. */
    fun setTargetFrames(frames: Long): Long

    /** Stops the output and gives back its resources. */
    fun close()
}

/**
 * One receive session between the socket and the audio output: the shared core's
 * [ReceivePipeline] (`crates/bandear-core`), called once per datagram (or receive timeout) of the
 * receive loop, and its outcomes acted on in the order the core lists them.
 *
 * Every rule between the datagram and the samples lives in the core: decoding, the reorder hold
 * and its playout, arrival timing, the link report, the cushion, the playback latency and the
 * phone's own volume protection. The session only hands over what a wake-up needs (the clock, the
 * output's readings, the settings) and does what the outcome says: send the report, write the
 * samples, set the cushion, show the stats, show the error, open the output when asked.
 *
 * The one rule that stays on Android is the clock-drift compensation ([DriftTrimmedOutput]): the
 * samples to write wait in a queue before the output, every datagram the socket already holds goes
 * through the core before any of them is written, and each is written only once the whole queue was
 * measured, trimmed when a surplus persists. So a desk clock faster than the phone's DAC leaves its
 * surplus where it is counted and given back, not unseen in the kernel's socket buffer.
 *
 * Time is [nanoTime], one monotonic clock for the whole session. [isRunning] is checked once each
 * receive answers and before each write, so what woke a stopped receive is never played. Close it
 * when the receiver stops: that closes the output and frees the core's state.
 */
internal class ReceiveSession(
    private val listener: PmonPlayer.Listener,
    private val mixState: ClientMixState,
    private val openOutput: (OutputFormat) -> PlaybackOutput,
    private val isRunning: () -> Boolean,
    private val nanoTime: () -> Long = System::nanoTime,
) : AutoCloseable {
    private val pipeline = ReceivePipeline(clockNs())
    private var output: DriftTrimmedOutput? = null

    /** When the next wake-up reads the output for a report, in milliseconds of [nanoTime]. */
    private var reportDueAtMs = pipeline.reportDueAtMs()

    /** What the output answered to the last cushion asked, for the next wake-up. */
    private var appliedTargetFrames: Long? = null

    /**
     * One wake-up: [receive] waits for a datagram and answers its bytes, or null when it timed out.
     * Then every datagram [receiveWaiting] answers without waiting (null once the socket holds no
     * more) goes through the core too, and what is queued plays, with what arrived during each write
     * drained the same way right after it. A failure past a receive is reported and the session goes
     * on; one in [receive] or [receiveWaiting] (the socket closing) ends it.
     */
    fun wake(receiveWaiting: () -> ByteArray? = { null }, receive: () -> ByteArray?) {
        val startedNs = clockNs()
        val atStart = readingsForReport(startedNs)
        val datagram = receive()
        if (!isRunning()) return
        offer(startedNs, atStart, datagram)
        drain(receiveWaiting)
        play(receiveWaiting)
    }

    /** Every datagram the socket already holds, each through the core as a wake-up of its own. */
    private fun drain(receiveWaiting: () -> ByteArray?) {
        repeat(MAX_DRAIN_DATAGRAMS) {
            val datagram = receiveWaiting() ?: return
            if (!isRunning()) return
            val startedNs = clockNs()
            offer(startedNs, readingsForReport(startedNs), datagram)
        }
    }

    /**
     * Writes what is queued, oldest first, and after each write drains what arrived while it
     * blocked: while a surplus lasts this runs for a long time, so the stop is checked before every
     * write, and the link report and the desk's hold ride on the drained datagrams' wake-ups. A
     * failed write is reported and leaves the rest queued for the next wake-up.
     */
    private fun play(receiveWaiting: () -> ByteArray?) {
        val queued = output ?: return
        while (isRunning()) {
            val played = try {
                queued.playNext()
            } catch (error: Exception) {
                listener.onError(error.message ?: "receiver error")
                false
            }
            if (!played) return
            drain(receiveWaiting)
        }
    }

    /** The output read for a report at [startedNs], only once one is due. */
    private fun readingsForReport(startedNs: ULong): OutputReadings? =
        output?.takeIf { (startedNs / NANOS_PER_MILLISECOND).toLong() >= reportDueAtMs }?.readings()

    /** One datagram (or timeout) through the core, started at [startedNs], and its outcome acted on. */
    private fun offer(startedNs: ULong, atStart: OutputReadings?, datagram: ByteArray?) {
        try {
            val applied = appliedTargetFrames
            appliedTargetFrames = null
            val outcome = pipeline.onWake(
                Wake(
                    startedNs = startedNs,
                    outputAtStart = atStart,
                    appliedTargetFrames = applied,
                    datagram = datagram,
                    receivedNs = clockNs(),
                    outputAtReceive = output?.readings(),
                    settings = settings(),
                ),
            )
            act(outcome)
        } catch (error: Exception) {
            listener.onError(error.message ?: "receiver error")
        }
    }

    override fun close() {
        try {
            output?.close()
        } finally {
            output = null
            pipeline.close()
        }
    }

    private fun act(first: WakeOutcome) {
        var outcome = first
        while (true) {
            outcome.report?.let { listener.onLinkStats(it.toLinkStatsReport()) }
            for (action in outcome.actions) {
                val target = checkNotNull(output) { "the core acted on an output that is not open" }
                when (action) {
                    is OutputAction.Write -> target.write(action.pcm16LeBytes)
                    is OutputAction.SetTargetFrames -> appliedTargetFrames = target.setTargetFrames(action.frames)
                }
            }
            outcome.stats.forEach(::show)
            outcome.error?.let { listener.onError(it.describe()) }
            reportDueAtMs = outcome.reportDueAtMs
            val format = outcome.openOutput ?: return
            outcome = open(format)
        }
    }

    /** Opens the output the core asked for, and goes on with the wake-up through it. */
    private fun open(format: OutputFormat): WakeOutcome {
        val opened = try {
            openOutput(format)
        } catch (error: Exception) {
            // The packet that asked is not played; the next one asks again.
            listener.onError(error.message ?: "could not open the audio output")
            return pipeline.onOutputFailed()
        }
        val queued = DriftTrimmedOutput(opened, format)
        output = queued
        return pipeline.onOutputOpened(clockNs(), queued.capacityFrames, queued.readings())
    }

    /** The settings now, read together once per wake-up. */
    private fun settings(): PlayerSettings = PlayerSettings(
        volumePercent = mixState.volumePercent.toLong(),
        maxLevelPercent = mixState.maxLevelPercent.toLong(),
        muted = mixState.muted,
        serverOwnsMix = listener.serverOwnsMix(),
        playbackBuffer = playbackBufferSetting(mixState.deskBufferMs),
        redundancyHoldPackets = mixState.redundancyHoldPackets.toLong(),
    )

    private fun show(stats: ReceiverStats) {
        // A value the core has not measured yet reads as 0, as the receiver always showed it.
        listener.onStats(
            packets = stats.packets,
            lost = stats.lost,
            sampleRate = stats.sampleRate.toInt(),
            packetsPerSecond = stats.packetsPerSecond,
            interArrivalMs = stats.interArrivalMs ?: 0.0,
            jitterMs = stats.jitterMs ?: 0.0,
            // The audio given up on before the output counts as discarded too.
            discardedSamples = stats.discardedSamples + (output?.discardedSamples ?: 0L),
            packetsWithDiscardedSamples =
                stats.packetsWithDiscardedSamples + (output?.packetsWithDiscardedSamples ?: 0L),
            playbackBacklogMs = stats.playbackBacklogMs ?: 0.0,
        )
    }

    private fun clockNs(): ULong = nanoTime().toULong()
}

/** The desk's buffer as [ClientMixState.deskBufferMs] holds it, as the core takes it. */
internal fun playbackBufferSetting(deskBufferMs: Int): PlaybackBufferSetting =
    if (deskBufferMs == PlaybackLatency.DESK_BUFFER_AUTO) {
        PlaybackBufferSetting.Auto
    } else {
        PlaybackBufferSetting.Fixed(deskBufferMs.coerceIn(0, UShort.MAX_VALUE.toInt()).toUShort())
    }

/** The core's link report as the control channel and the screens read it: `NaN` when not measured. */
internal fun CoreLinkStatsReport.toLinkStatsReport(): LinkStatsReport = LinkStatsReport(
    intervalMs = intervalMs,
    packetsReceived = packetsReceived,
    packetsLost = packetsLost,
    underruns = underruns,
    receiveTimeouts = receiveTimeouts,
    jitterMs = jitterMs ?: Double.NaN,
    cushionMs = cushionMs ?: Double.NaN,
    backlogMs = backlogMs ?: Double.NaN,
    recovered = recovered,
)

/** What the receiver shows for a wake-up the core ended early: the copy stays on each platform. */
internal fun ReceiveError.describe(): String = when (this) {
    is ReceiveError.Undecodable -> reason.describe()
    is ReceiveError.TooManyChannels -> "PMON packet announces $channels channels, the limit is $MAX_CHANNELS"
    is ReceiveError.SampleRateChanged -> "sample rate changed from $from to $to"
}

private fun DecodeError.describe(): String = when (this) {
    is DecodeError.HeaderTooShort -> "packet is shorter than the PMON header"
    is DecodeError.UnknownMagic -> "invalid PMON magic"
    is DecodeError.UnsupportedVersion -> "unsupported PMON version"
    DecodeError.ZeroChannels -> "PMON packet has no channels"
    is DecodeError.InvalidSampleRate -> "PMON packet has an invalid sample rate"
    is DecodeError.PayloadLengthMismatch ->
        "PCM payload length does not match sample count: expected " +
            "${PMON_HEADER_BYTES + declaredSampleCount * 2uL}, got ${PMON_HEADER_BYTES + payloadByteCount}"
    is DecodeError.SampleCountNotDivisibleByChannels ->
        "PMON packet has $sampleCount samples, not whole frames of $channels channels"
    is DecodeError.SampleBufferTooSmall ->
        "PMON packet has $sampleCount samples, more than the receiver holds ($capacity)"
}
