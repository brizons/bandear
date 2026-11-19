package com.bandear.android

import com.bandear.android.core.DecodeError
import com.bandear.android.core.MutableOutputReadings
import com.bandear.android.core.NativeReceivePipeline
import com.bandear.android.core.OutputFormat
import com.bandear.android.core.ReceiveError
import com.bandear.android.core.WakeInput
import com.bandear.android.core.WakeOutcomeReader
import com.bandear.android.core.maximumMixChannels
import com.bandear.android.core.LinkStatsReport as CoreLinkStatsReport
import java.nio.ByteBuffer
import java.util.function.IntSupplier
import java.util.function.LongSupplier

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

    /** Reads what the output says about itself now into [readings], in one go. */
    fun read(readings: MutableOutputReadings)

    /**
     * Plays [size] bytes of interleaved stereo PCM16, two bytes each, little endian, from [pcm]'s
     * position on. Answers the bytes it took: [size] when it took them all, fewer on a short write,
     * or a negative error code (`AudioTrack`'s) when it took none.
     */
    fun write(pcm: ByteBuffer, size: Int): Int

    /** Asks the output to queue [frames] before it plays; answers the frames it applied, 0 when it cannot say. */
    fun setTargetFrames(frames: Long): Long

    /** Stops the output and gives back its resources. */
    fun close()
}

/**
 * One receive session between the socket and the audio output: the shared core's receive pipeline
 * (`crates/bandear-core`), called once per datagram (or receive timeout) of the receive loop through
 * [NativeReceivePipeline], and its outcomes acted on in the order the core lists them.
 *
 * Every rule between the datagram and the samples lives in the core: decoding, the reorder hold
 * and its playout, arrival timing, the link report, the cushion, the playback latency and the
 * phone's own volume protection. The session only hands over what a wake-up needs (the clock, the
 * output's readings, the settings) and does what the outcome says: send the report, write the
 * samples, set the cushion, show the stats, show the error, open the output when asked. A wake-up
 * allocates almost nothing of its own: the datagram stays in the receive buffer, the wake-up's
 * values in one reused [WakeInput], and the samples are copied from the outcome's buffer into
 * buffers the output reuses (its queue keeps one small entry per packet).
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
    private val nanoTime: LongSupplier = LongSupplier(System::nanoTime),
) : AutoCloseable {
    private val pipeline = NativeReceivePipeline(nanoTime.asLong)
    private var output: DriftTrimmedOutput? = null

    /** When the next wake-up reads the output for a report, in milliseconds of [nanoTime]. */
    private var reportDueAtMs = pipeline.reportDueAtMs()

    /** What the output answered to the last cushion asked, for the next wake-up. */
    private var hasAppliedTargetFrames = false
    private var appliedTargetFrames = 0L

    /** The wake-up handed to the core, overwritten for each one. */
    private val wake = WakeInput()

    /** The output's readings once it opened. */
    private val openedReadings = MutableOutputReadings()

    /**
     * One wake-up: [receive] waits for a datagram, puts it at the start of [buffer] and answers its
     * length, or [TIMED_OUT] when nothing came. Then every datagram [receiveWaiting] puts there
     * without waiting (it answers [NOTHING_WAITING] once the socket holds no more) goes through the
     * core too, and what is queued plays, with what arrived during each write drained the same way
     * right after it. A failure past a receive is reported and the session goes on; one in
     * [receive] or [receiveWaiting] (the socket closing) ends it.
     */
    fun wake(buffer: ByteArray, receiveWaiting: IntSupplier = NOTHING_WAITS, receive: IntSupplier) {
        start()
        val length = receive.asInt
        if (!isRunning()) return
        offer(buffer, length)
        drain(buffer, receiveWaiting)
        play(buffer, receiveWaiting)
    }

    /** Every datagram the socket already holds, each through the core as a wake-up of its own. */
    private fun drain(buffer: ByteArray, receiveWaiting: IntSupplier) {
        repeat(MAX_DRAIN_DATAGRAMS) {
            val length = receiveWaiting.asInt
            if (length < 0 || !isRunning()) return
            start()
            offer(buffer, length)
        }
    }

    /**
     * Writes what is queued, oldest first, and after each write drains what arrived while it
     * blocked: while a surplus lasts this runs for a long time, so the stop is checked before every
     * write, and the link report and the desk's hold ride on the drained datagrams' wake-ups (one
     * due while nothing arrives waits until the queue, at most [MAX_QUEUED_FRAMES], has played). A
     * failed write is reported and leaves the rest queued for the next wake-up.
     */
    private fun play(buffer: ByteArray, receiveWaiting: IntSupplier) {
        val queued = output ?: return
        while (isRunning()) {
            val played = try {
                queued.playNext()
            } catch (error: Exception) {
                listener.onError(error.message ?: "receiver error")
                false
            }
            if (!played) return
            drain(buffer, receiveWaiting)
        }
    }

    /** Starts a wake-up now: its clock, and the output read for a report once one is due. */
    private fun start() {
        val startedNs = nanoTime.asLong
        wake.startedNs = startedNs
        val atStart = output?.takeIf { (startedNs.toULong() / NANOS_PER_MILLISECOND).toLong() >= reportDueAtMs }
        wake.hasOutputAtStart = atStart != null
        atStart?.read(wake.outputAtStart)
    }

    /** The datagram of [length] bytes at the start of [buffer] (or a timeout) through the core, and its outcome acted on. */
    private fun offer(buffer: ByteArray, length: Int) {
        try {
            wake.hasAppliedTargetFrames = hasAppliedTargetFrames
            wake.appliedTargetFrames = appliedTargetFrames
            hasAppliedTargetFrames = false
            wake.receivedNs = nanoTime.asLong
            val atReceive = output
            wake.hasOutputAtReceive = atReceive != null
            atReceive?.read(wake.outputAtReceive)
            readSettings()
            val datagram = if (length == TIMED_OUT) null else buffer
            act(pipeline.onWake(wake, datagram, length.coerceAtLeast(0)))
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

    private fun act(first: WakeOutcomeReader) {
        var outcome = first
        while (true) {
            var format: OutputFormat? = null
            while (outcome.next()) {
                when (outcome.kind) {
                    WakeOutcomeReader.REPORT -> listener.onLinkStats(outcome.report().toLinkStatsReport())
                    WakeOutcomeReader.WRITE -> openedOutput().write(outcome.pcm(), outcome.pcmSize)
                    WakeOutcomeReader.SET_TARGET_FRAMES -> {
                        appliedTargetFrames = openedOutput().setTargetFrames(outcome.targetFrames)
                        hasAppliedTargetFrames = true
                    }
                    WakeOutcomeReader.STATS -> show(outcome.stats)
                    WakeOutcomeReader.ERROR -> listener.onError(outcome.error().describe())
                    WakeOutcomeReader.OPEN_OUTPUT -> format = outcome.openOutput()
                }
            }
            reportDueAtMs = outcome.reportDueAtMs
            outcome = open(format ?: return)
        }
    }

    private fun openedOutput(): PlaybackOutput = checkNotNull(output) { "the core acted on an output that is not open" }

    /** Opens the output the core asked for, and goes on with the wake-up through it. */
    private fun open(format: OutputFormat): WakeOutcomeReader {
        val opened = try {
            openOutput(format)
        } catch (error: Exception) {
            // The packet that asked is not played; the next one asks again.
            listener.onError(error.message ?: "could not open the audio output")
            return pipeline.onOutputFailed()
        }
        val queued = DriftTrimmedOutput(opened, format)
        output = queued
        val nowNs = nanoTime.asLong
        queued.read(openedReadings)
        return pipeline.onOutputOpened(nowNs, queued.capacityFrames, openedReadings)
    }

    /** The settings now, read together once per wake-up. */
    private fun readSettings() {
        wake.volumePercent = mixState.volumePercent.toLong()
        wake.maxLevelPercent = mixState.maxLevelPercent.toLong()
        wake.muted = mixState.muted
        wake.serverOwnsMix = listener.serverOwnsMix()
        val deskBufferMs = mixState.deskBufferMs
        wake.hasFixedPlaybackBuffer = deskBufferMs != PlaybackLatency.DESK_BUFFER_AUTO
        wake.fixedPlaybackBufferMs = deskBufferMs.coerceIn(0, UShort.MAX_VALUE.toInt())
        wake.redundancyHoldPackets = mixState.redundancyHoldPackets.toLong()
    }

    private fun show(stats: WakeOutcomeReader.Stats) {
        // A value the core has not measured yet reads as 0, as the receiver always showed it.
        listener.onStats(
            packets = stats.packets,
            lost = stats.lost,
            sampleRate = stats.sampleRate.toInt(),
            packetsPerSecond = stats.packetsPerSecond,
            interArrivalMs = stats.interArrivalMs(notMeasured = 0.0),
            jitterMs = stats.jitterMs(notMeasured = 0.0),
            // The audio given up on before the output counts as discarded too.
            discardedSamples = stats.discardedSamples + (output?.discardedSamples ?: 0L),
            packetsWithDiscardedSamples =
                stats.packetsWithDiscardedSamples + (output?.packetsWithDiscardedSamples ?: 0L),
            playbackBacklogMs = stats.playbackBacklogMs(notMeasured = 0.0),
        )
    }

    companion object {
        /** What a receive answers when nothing came before its timeout. */
        const val TIMED_OUT = -1

        /** What a receive of what already waits answers once the socket holds nothing more. */
        const val NOTHING_WAITING = -1

        /** A socket that never holds anything more: no drain. */
        private val NOTHING_WAITS = IntSupplier { NOTHING_WAITING }
    }
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

/**
 * The most channels a packet may announce, the shared core's [maximumMixChannels]: read once, so a
 * stream of refused packets costs no binding call each on the receive thread.
 */
private val channelLimit: Long by lazy { maximumMixChannels() }

/** What the receiver shows for a wake-up the core ended early: the copy stays on each platform. */
internal fun ReceiveError.describe(): String = when (this) {
    is ReceiveError.Undecodable -> reason.describe()
    is ReceiveError.TooManyChannels -> "PMON packet announces $channels channels, the limit is $channelLimit"
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
