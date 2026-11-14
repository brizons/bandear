package com.bandear.android.core

import java.nio.Buffer
import java.nio.ByteBuffer
import java.nio.ByteOrder

/**
 * Android's receive pipeline: the shared core's `ReceivePipeline` (`crates/bandear-core`), one
 * native call per wake-up of the receive loop, through the hand-written JNI functions in
 * `crates/bandear-ffi/src/receive_pipeline_jni`.
 *
 * It stands in for the UniFFI binding of the same pipeline ([ReceivePipeline], which iOS keeps
 * using), whose Kotlin side crosses JNA four times per call and lifts every value into new
 * objects. Here a wake-up goes in as numbers packed into a reused `LongArray` plus the `ByteArray`
 * the socket filled, and its outcome comes back encoded into a reused direct [ByteBuffer], read in
 * place by [WakeOutcomeReader]; a write's samples go to the audio output where they lie. Nothing
 * is allocated per packet. The layouts are documented once, in
 * `crates/bandear-ffi/src/receive_pipeline_jni/layout.rs`; this file mirrors them.
 *
 * One pipeline per receive session, called and closed from its receive thread only. An outcome
 * is valid until the next call. Close it when the session ends, which frees the core's state.
 */
class NativeReceivePipeline(
    startedNs: Long,
    initialOutcomeBytes: Int = INITIAL_OUTCOME_BYTES,
) : AutoCloseable {
    /** The core's pipeline behind the JNI functions; 0 once closed. */
    private var handle: Long = nativeCreate(startedNs)
    private val wakeWords = LongArray(WAKE_WORDS)
    private val readingsWords = LongArray(READINGS_WORDS)
    private var outcomeBuffer: ByteBuffer = outcomeBuffer(initialOutcomeBytes)
    private val outcome = WakeOutcomeReader()

    init {
        check(handle != 0L) { "the shared core could not create a receive pipeline" }
    }

    /** The bytes the outcome buffer holds: it grows when an outcome does not fit. */
    val outcomeCapacity: Int get() = outcomeBuffer.capacity()

    /** The first instant, in milliseconds, at which a wake-up reads the output for a report. */
    fun reportDueAtMs(): Long = nativeReportDueAtMs(open())

    /**
     * One wake-up: [wake], and the datagram it received, the first [length] bytes of [datagram],
     * or a null [datagram] when the receive timed out.
     */
    fun onWake(wake: WakeInput, datagram: ByteArray?, length: Int): WakeOutcomeReader {
        pack(wake)
        return read(nativeOnWake(open(), wakeWords, datagram, length, outcomeBuffer))
    }

    /**
     * The output the last outcome asked for opened, holding [capacityFrames] and reading [output]
     * now: the wake-up goes on with the packet that asked for it.
     */
    fun onOutputOpened(nowNs: Long, capacityFrames: Long, output: MutableOutputReadings): WakeOutcomeReader {
        pack(output, readingsWords, 0)
        return read(nativeOnOutputOpened(open(), nowNs, capacityFrames, readingsWords, outcomeBuffer))
    }

    /** The output the last outcome asked for did not open: the wake-up ends, the next packet asks again. */
    fun onOutputFailed(): WakeOutcomeReader = read(nativeOnOutputFailed(open(), outcomeBuffer))

    override fun close() {
        val closing = handle
        handle = 0L
        if (closing != 0L) nativeDestroy(closing)
    }

    private fun open(): Long {
        check(handle != 0L) { "the receive pipeline is closed" }
        return handle
    }

    /** The outcome a call answered [status] for, taken into a larger buffer when it did not fit. */
    private fun read(status: Int): WakeOutcomeReader {
        var size = checkStatus(status)
        if (size > outcomeBuffer.capacity()) {
            outcomeBuffer = outcomeBuffer(maxOf(size, outcomeBuffer.capacity() * 2))
            size = checkStatus(nativeTakeOutcome(open(), outcomeBuffer))
        }
        outcome.reset(outcomeBuffer, size)
        return outcome
    }

    private fun pack(wake: WakeInput) {
        val words = wakeWords
        words[0] = wake.startedNs
        words[1] = flag(wake.hasOutputAtStart)
        pack(wake.outputAtStart, words, 2)
        words[8] = flag(wake.hasAppliedTargetFrames)
        words[9] = wake.appliedTargetFrames
        words[10] = wake.receivedNs
        words[11] = flag(wake.hasOutputAtReceive)
        pack(wake.outputAtReceive, words, 12)
        words[18] = wake.volumePercent
        words[19] = wake.maxLevelPercent
        words[20] = flag(wake.muted)
        words[21] = flag(wake.serverOwnsMix)
        words[22] = flag(wake.hasFixedPlaybackBuffer)
        words[23] = wake.fixedPlaybackBufferMs.toLong()
        words[24] = wake.redundancyHoldPackets
    }

    private fun pack(readings: MutableOutputReadings, words: LongArray, first: Int) {
        words[first] = flag(readings.hasWritableFrames)
        words[first + 1] = readings.writableFrames
        words[first + 2] = readings.underrunCount
        words[first + 3] = flag(readings.hasPresentedFrame)
        words[first + 4] = readings.presentedFrame
        words[first + 5] = flag(readings.isPlaying)
    }

    // The JNI functions (`crates/bandear-ffi/src/receive_pipeline_jni/mod.rs`), bound by these names.
    private external fun nativeCreate(startedNs: Long): Long
    private external fun nativeDestroy(handle: Long)
    private external fun nativeReportDueAtMs(handle: Long): Long
    private external fun nativeOnWake(
        handle: Long,
        words: LongArray,
        datagram: ByteArray?,
        length: Int,
        outcome: ByteBuffer,
    ): Int
    private external fun nativeOnOutputOpened(
        handle: Long,
        nowNs: Long,
        capacityFrames: Long,
        readings: LongArray,
        outcome: ByteBuffer,
    ): Int
    private external fun nativeOnOutputFailed(handle: Long, outcome: ByteBuffer): Int
    private external fun nativeTakeOutcome(handle: Long, outcome: ByteBuffer): Int

    private companion object {
        init {
            // The library the UniFFI bindings load through JNA too; the JVM binds the functions
            // above from it.
            System.loadLibrary("bandear_ffi")
        }

        /** Room for a usual outcome (one 128-frame stereo packet takes about 650 bytes) many times over. */
        const val INITIAL_OUTCOME_BYTES = 16 * 1024

        const val WAKE_WORDS = 25
        const val READINGS_WORDS = 6

        // The statuses a call answers besides a size.
        const val STATUS_FAILED = -1
        const val STATUS_INVALID_ARGUMENT = -2
        const val STATUS_NO_PENDING_OUTCOME = -3

        fun outcomeBuffer(bytes: Int): ByteBuffer =
            ByteBuffer.allocateDirect(bytes.coerceAtLeast(OUTCOME_HEADER_BYTES)).order(ByteOrder.LITTLE_ENDIAN)

        fun flag(value: Boolean): Long = if (value) 1L else 0L

        fun checkStatus(status: Int): Int = when (status) {
            STATUS_FAILED -> throw IllegalStateException("the shared core failed inside its receive pipeline")
            STATUS_INVALID_ARGUMENT -> throw IllegalArgumentException("the shared core refused the receive pipeline's arguments")
            STATUS_NO_PENDING_OUTCOME -> throw IllegalStateException("no outcome was waiting for a larger buffer")
            else -> {
                check(status >= 0) { "unknown receive pipeline status $status" }
                status
            }
        }
    }
}

/**
 * What an audio output says about itself, read in one go: the core's `OutputReadings`, in fields
 * the output overwrites on every read, so reading allocates nothing. An optional value is a flag
 * and a number; the number is ignored while the flag is down.
 */
class MutableOutputReadings {
    /** Whether the output knows [writableFrames]: false for a blocking `AudioTrack.write`, which takes every frame. */
    var hasWritableFrames = false

    /** The frames the output takes now without blocking. */
    var writableFrames = 0L

    /** The times the output ran dry since it opened. */
    var underrunCount = 0L

    /** Whether the output knows [presentedFrame] yet. */
    var hasPresentedFrame = false

    /** The frame reaching the listener now, counted from the first frame written. */
    var presentedFrame = 0L

    /** Whether the output is reaching the listener (false while the system holds it stopped). */
    var isPlaying = false
}

/**
 * One wake-up of the receive loop, as [NativeReceivePipeline.onWake] sends it: the core's `Wake`
 * but its datagram, in fields the loop overwrites for every wake-up, so sending one allocates
 * nothing. An optional value is a flag and a number; the number is ignored while the flag is down.
 * Times are nanoseconds of one monotonic clock; the settings' defaults are the core's.
 */
class WakeInput {
    /** The clock when the wake-up started, before its receive: the link report is taken then. */
    var startedNs = 0L

    /** Whether [outputAtStart] was read: an output is open and a report is due. */
    var hasOutputAtStart = false
    val outputAtStart = MutableOutputReadings()

    /** Whether the previous outcome asked for a cushion, which [appliedTargetFrames] answers. */
    var hasAppliedTargetFrames = false

    /** The frames the output applied for that cushion, 0 when it could not say. */
    var appliedTargetFrames = 0L

    /** The clock once the receive answered: arrivals and plays are timed by it. */
    var receivedNs = 0L

    /** Whether [outputAtReceive] was read: an output is open. */
    var hasOutputAtReceive = false
    val outputAtReceive = MutableOutputReadings()

    /** The phone's master volume, in percent. */
    var volumePercent = 100L

    /** The highest level the phone plays, in percent of full scale. */
    var maxLevelPercent = 100L
    var muted = false

    /** True once the desk acknowledged a mix: the phone then leaves the audio as it comes. */
    var serverOwnsMix = false

    /** Whether the desk pinned its playback buffer to [fixedPlaybackBufferMs]; Auto otherwise. */
    var hasFixedPlaybackBuffer = false
    var fixedPlaybackBufferMs = 0

    /** The packets held back for the desk's delayed copies. */
    var redundancyHoldPackets = 0L
}

/**
 * One outcome of [NativeReceivePipeline], read in place from the buffer the core encoded it into:
 * its records one at a time with [next], in the order the platform acts on them (the report, the
 * output actions, the stats, the error, the output to open), each read through the accessors of
 * its [kind]. Reading a write's samples, a cushion or the stats allocates nothing; the report, the
 * error and the output format, which come at most once a second or once a session, are built as
 * the bindings' own types. Valid until the pipeline's next call.
 */
class WakeOutcomeReader internal constructor() {
    private var buffer: ByteBuffer = EMPTY
    private var size = 0
    private var recordsLeft = 0
    private var record = 0
    private var nextRecord = 0

    /** The first instant, in milliseconds, at which the next wake-up reads the output for a report. */
    var reportDueAtMs = 0L
        private set

    /** The current record's kind: [REPORT], [WRITE], [SET_TARGET_FRAMES], [STATS], [ERROR] or [OPEN_OUTPUT]. */
    var kind = NONE
        private set

    /** The current [STATS] record. */
    val stats = Stats()

    internal fun reset(buffer: ByteBuffer, size: Int) {
        check(size in OUTCOME_HEADER_BYTES..buffer.capacity()) { "an outcome of $size bytes" }
        val version = buffer.getInt(0)
        check(version == LAYOUT_VERSION) {
            "outcome layout $version, expected $LAYOUT_VERSION: the app and its native library disagree"
        }
        this.buffer = buffer
        this.size = size
        recordsLeft = buffer.getInt(4)
        reportDueAtMs = buffer.getLong(8)
        record = OUTCOME_HEADER_BYTES
        nextRecord = OUTCOME_HEADER_BYTES
        kind = NONE
    }

    /** Moves to the next record; false once every record was read. */
    fun next(): Boolean {
        if (recordsLeft == 0) {
            kind = NONE
            return false
        }
        val at = nextRecord
        check(at + RECORD_HEAD_BYTES <= size) { "a record past the outcome's end" }
        val length = buffer.getInt(at + 4)
        check(length >= RECORD_HEAD_BYTES && length % FIELD_BYTES == 0 && length <= size - at) {
            "a record of $length bytes at $at"
        }
        record = at
        nextRecord = at + length
        recordsLeft--
        kind = buffer.getInt(at)
        return true
    }

    /** A [REPORT]: the link report due when the wake-up started, for the desk. */
    fun report(): LinkStatsReport {
        expect(REPORT)
        return LinkStatsReport(
            intervalMs = field(1),
            packetsReceived = field(2),
            packetsLost = field(3),
            underruns = field(4),
            receiveTimeouts = field(5),
            jitterMs = if (isPresent(0)) real(6) else null,
            cushionMs = if (isPresent(1)) real(7) else null,
            backlogMs = if (isPresent(2)) real(8) else null,
            recovered = field(9),
        )
    }

    /** A [WRITE]'s byte count: interleaved stereo PCM16, little endian. */
    val pcmSize: Int
        get() {
            expect(WRITE)
            val bytes = field(0)
            check(bytes in 0..(nextRecord - record - RECORD_HEAD_BYTES - FIELD_BYTES)) { "a write of $bytes bytes" }
            return bytes.toInt()
        }

    /**
     * A [WRITE]'s samples: the outcome's buffer, positioned at them, [pcmSize] bytes long; an
     * output may move its position as it reads.
     */
    fun pcm(): ByteBuffer {
        expect(WRITE)
        // Through Buffer, whose limit(int) and position(int) every Android release has; ByteBuffer's
        // covariant overrides of them are newer than some the app runs on.
        val view: Buffer = buffer
        view.limit(buffer.capacity())
        view.position(record + RECORD_HEAD_BYTES + FIELD_BYTES)
        return buffer
    }

    /** A [SET_TARGET_FRAMES]: the cushion to ask the output for, in frames. */
    val targetFrames: Long
        get() {
            expect(SET_TARGET_FRAMES)
            return field(0)
        }

    /** An [ERROR]: why the wake-up ended before it played everything due. */
    fun error(): ReceiveError {
        expect(ERROR)
        val first = field(1)
        val second = field(2)
        val undecodable = when (val code = field(0).toInt()) {
            1 -> DecodeError.HeaderTooShort(first.toULong())
            2 -> DecodeError.UnknownMagic(ByteArray(4) { (first ushr (it * 8)).toByte() })
            3 -> DecodeError.UnsupportedVersion(first.toUByte())
            4 -> DecodeError.ZeroChannels
            5 -> DecodeError.InvalidSampleRate(first.toUInt())
            6 -> DecodeError.PayloadLengthMismatch(first.toULong(), second.toULong())
            7 -> DecodeError.SampleCountNotDivisibleByChannels(first.toULong(), second.toUByte())
            8 -> DecodeError.SampleBufferTooSmall(first.toULong(), second.toULong())
            9 -> return ReceiveError.TooManyChannels(first)
            10 -> return ReceiveError.SampleRateChanged(first, second)
            else -> throw IllegalStateException("unknown receive error $code")
        }
        return ReceiveError.Undecodable(undecodable)
    }

    /** An [OPEN_OUTPUT]: open an output in this format, then answer the pipeline whether it opened. */
    fun openOutput(): OutputFormat {
        expect(OPEN_OUTPUT)
        return OutputFormat(sampleRate = field(0), packetFrames = field(1))
    }

    /** The current [STATS] record: the receiver's metrics after a packet played. */
    inner class Stats internal constructor() {
        val packets: Long get() = statsField(1)
        val lost: Long get() = statsField(2)
        val sampleRate: Long get() = statsField(3)
        val packetsPerSecond: Double get() = java.lang.Double.longBitsToDouble(statsField(4))

        /** The smoothed spacing between arrivals, or [notMeasured] before a second arrival. */
        fun interArrivalMs(notMeasured: Double): Double = optionalReal(0, 5, notMeasured)

        /** The smoothed jitter around that spacing, or [notMeasured]. */
        fun jitterMs(notMeasured: Double): Double = optionalReal(1, 6, notMeasured)
        val discardedSamples: Long get() = statsField(7)
        val packetsWithDiscardedSamples: Long get() = statsField(8)

        /** The audio queued ahead of the listener, or [notMeasured] before the output said what it presented. */
        fun playbackBacklogMs(notMeasured: Double): Double = optionalReal(2, 9, notMeasured)
        val outputPlaying: Boolean get() = statsField(10) != 0L

        private fun statsField(index: Int): Long {
            expect(STATS)
            return field(index)
        }

        private fun optionalReal(bit: Int, index: Int, notMeasured: Double): Double =
            if (statsField(0) and (1L shl bit) != 0L) java.lang.Double.longBitsToDouble(statsField(index)) else notMeasured
    }

    private fun expect(expected: Int) {
        check(kind == expected) { "record $kind read as $expected" }
    }

    /** The current record's field [index], counted from 0 after its head. */
    private fun field(index: Int): Long {
        val at = record + RECORD_HEAD_BYTES + index * FIELD_BYTES
        check(at + FIELD_BYTES <= nextRecord) { "field $index past record $kind's end" }
        return buffer.getLong(at)
    }

    private fun real(index: Int): Double = java.lang.Double.longBitsToDouble(field(index))

    private fun isPresent(bit: Int): Boolean = field(0) and (1L shl bit) != 0L

    companion object {
        /** No record: before [next], or after the last. */
        const val NONE = 0
        const val REPORT = 1
        const val WRITE = 2
        const val SET_TARGET_FRAMES = 3
        const val STATS = 4
        const val ERROR = 5
        const val OPEN_OUTPUT = 6

        private const val LAYOUT_VERSION = 1
        private const val RECORD_HEAD_BYTES = 8
        private const val FIELD_BYTES = 8
        private val EMPTY: ByteBuffer = ByteBuffer.allocate(0)
    }
}

/** The bytes of an outcome's header: its layout version, its record count, `report_due_at_ms`. */
private const val OUTCOME_HEADER_BYTES = 16
