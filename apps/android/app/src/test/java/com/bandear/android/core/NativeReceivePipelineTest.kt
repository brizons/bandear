package com.bandear.android.core

import java.nio.ByteBuffer
import java.nio.ByteOrder
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Drives the shared receive pipeline through the hand-written JNI functions
 * ([NativeReceivePipeline], `crates/bandear-ffi/src/receive_pipeline_jni`) and the host build of
 * the core, the way Android's receive loop does: a datagram asks for the output, the output opens,
 * later datagrams come out as the PCM bytes to write, a timeout carries the link report. The
 * pipeline's rules are tested in Rust; these tests prove each call crosses JNI, and each record
 * decodes in Kotlin, as the loop expects.
 */
class NativeReceivePipelineTest {
    @Test
    fun aPacketAsksForTheOutputThenPlaysThroughItOnceOpened() {
        NativeReceivePipeline(0L).use { pipeline ->
            assertEquals(3_000L, pipeline.reportDueAtMs())

            val first = records(receive(pipeline, packet(1uL, seed = 0), atMs = 0))
            assertEquals(listOf(OutputFormat(sampleRate = 48_000L, packetFrames = FRAMES)), first)

            val opened = pipeline.onOutputOpened(0L, 24_000L, playing())
            assertEquals(1_000L, opened.reportDueAtMs)
            assertEquals(
                listOf(
                    "target ${2 * FRAMES}",
                    Write(PmonFixtures.littleEndian(PmonFixtures.samples(0))),
                    "stats 1",
                ),
                records(opened),
            )

            val second = records(receive(pipeline, packet(2uL, seed = 1), atMs = 10))
            assertEquals(listOf(Write(PmonFixtures.littleEndian(PmonFixtures.samples(1))), "stats 2"), second)
            // The byte order, read by hand: 0x0102, -2, Short.MIN_VALUE, Short.MAX_VALUE.
            assertEquals(
                listOf<Byte>(0x02, 0x01, 0xFE.toByte(), 0xFF.toByte(), 0x00, 0x80.toByte(), 0xFF.toByte(), 0x7F),
                (second[0] as Write).bytes.take(8),
            )
        }
    }

    @Test
    fun theStatsOfAPlayedPacketReadWithEveryValue() {
        NativeReceivePipeline(0L).use { pipeline ->
            open(pipeline)
            val outcome = receive(pipeline, packet(2uL, seed = 1), atMs = 3)
            while (outcome.next() && outcome.kind != WakeOutcomeReader.STATS) Unit

            val stats = outcome.stats
            assertEquals(2L, stats.packets)
            assertEquals(0L, stats.lost)
            assertEquals(48_000L, stats.sampleRate)
            assertEquals(3.0, stats.interArrivalMs(notMeasured = -1.0), 1e-9)
            assertEquals(0L, stats.discardedSamples)
            assertEquals(0L, stats.packetsWithDiscardedSamples)
            assertTrue(stats.outputPlaying)
            // The output never said what it presented.
            assertEquals(-1.0, stats.playbackBacklogMs(notMeasured = -1.0), 0.0)
        }
    }

    @Test
    fun aTimeoutPlaysNothingAndCarriesTheReportOnceDue() {
        NativeReceivePipeline(0L).use { pipeline ->
            open(pipeline)

            // The output applied a little more than the cushion asked.
            val wake = wake(atMs = 1_000, atStart = playing(underruns = 2)).apply {
                hasAppliedTargetFrames = true
                appliedTargetFrames = 300L
            }
            val timeout = pipeline.onWake(wake, null, 0)
            assertEquals(2_000L, timeout.reportDueAtMs)
            val report = records(timeout).single() as LinkStatsReport
            assertEquals(1L, report.packetsReceived)
            assertEquals(0L, report.packetsLost)
            assertEquals(2L, report.underruns)
            // The cushion the output applied, 300 frames at 48 kHz.
            assertEquals(6.25, report.cushionMs!!, 1e-9)
            // No two packets arrived in the interval.
            assertNull(report.jitterMs)
        }
    }

    @Test
    fun onlyTheReceivedLengthOfTheBufferIsTheDatagram() {
        NativeReceivePipeline(0L).use { pipeline ->
            val datagram = packet(1uL, seed = 0)
            // One byte short of the datagram, in a buffer that holds all of it and more.
            val outcome = pipeline.onWake(wake(atMs = 0), received(datagram), datagram.size - 1)

            assertEquals(
                listOf(ReceiveError.Undecodable(DecodeError.PayloadLengthMismatch(256uL, 511uL))),
                records(outcome),
            )
        }
    }

    @Test
    fun everyRefusalCrossesWithItsFields() {
        val stereo = PmonFixtures.samples(0)
        val cases = listOf(
            ByteArray(5) { 'P'.code.toByte() } to ReceiveError.Undecodable(DecodeError.HeaderTooShort(5uL)),
            datagram(version = 2) to ReceiveError.Undecodable(DecodeError.UnsupportedVersion(2u)),
            datagram(channels = 0, samples = ShortArray(0)) to ReceiveError.Undecodable(DecodeError.ZeroChannels),
            datagram(sampleRate = 0) to ReceiveError.Undecodable(DecodeError.InvalidSampleRate(0u)),
            datagram(samples = stereo.copyOf(3)) to
                ReceiveError.Undecodable(DecodeError.SampleCountNotDivisibleByChannels(3uL, 2u)),
            datagram(channels = 33, samples = ShortArray(33)) to ReceiveError.TooManyChannels(33L),
        )
        for ((datagram, expected) in cases) {
            NativeReceivePipeline(0L).use { pipeline ->
                assertEquals("$expected", listOf(expected), records(receive(pipeline, datagram, atMs = 0)))
            }
        }

        NativeReceivePipeline(0L).use { pipeline ->
            val notPmon = packet(1uL, seed = 0)
            notPmon[3] = 'X'.code.toByte()
            val error = records(receive(pipeline, notPmon, atMs = 0)).single() as ReceiveError.Undecodable
            assertArrayEquals("PMOX".toByteArray(), (error.reason as DecodeError.UnknownMagic).magic)
        }

        NativeReceivePipeline(0L).use { pipeline ->
            open(pipeline)
            assertEquals(
                listOf(ReceiveError.SampleRateChanged(48_000L, 44_100L)),
                records(receive(pipeline, datagram(sequence = 2uL, sampleRate = 44_100), atMs = 3)),
            )
        }
    }

    @Test
    fun aFailedOpenEndsTheWakeUpAndTheNextPacketAsksAgain() {
        NativeReceivePipeline(0L).use { pipeline ->
            receive(pipeline, packet(1uL, seed = 0), atMs = 0)

            assertEquals(emptyList<Any>(), records(pipeline.onOutputFailed()))

            assertEquals(
                listOf(OutputFormat(sampleRate = 48_000L, packetFrames = FRAMES)),
                records(receive(pipeline, packet(2uL, seed = 1), atMs = 3)),
            )
        }
    }

    @Test
    fun anOutcomeLargerThanItsBufferGrowsTheBufferAndArrivesWhole() {
        // Room for the first outcome (the output to open, 40 bytes) and no more.
        NativeReceivePipeline(0L, initialOutcomeBytes = 48).use { pipeline ->
            receive(pipeline, packet(1uL, seed = 0), atMs = 0)
            assertEquals(48, pipeline.outcomeCapacity)

            val opened = records(pipeline.onOutputOpened(0L, 24_000L, playing()))

            assertEquals(
                listOf("target ${2 * FRAMES}", Write(PmonFixtures.littleEndian(PmonFixtures.samples(0))), "stats 1"),
                opened,
            )
            // Header, cushion, write (its head, count and 512 bytes), stats.
            assertTrue("${pipeline.outcomeCapacity}", pipeline.outcomeCapacity >= 16 + 16 + 528 + 96)

            // The grown buffer serves the next calls.
            assertEquals(
                listOf(Write(PmonFixtures.littleEndian(PmonFixtures.samples(1))), "stats 2"),
                records(receive(pipeline, packet(2uL, seed = 1), atMs = 3)),
            )
        }
    }

    @Test
    fun aLengthOutsideTheReceivedBufferIsRefusedAndThePipelineGoesOn() {
        NativeReceivePipeline(0L).use { pipeline ->
            val datagram = packet(1uL, seed = 0)
            for (length in listOf(datagram.size + 1, -1)) {
                assertThrows(IllegalArgumentException::class.java) {
                    pipeline.onWake(wake(atMs = 0), datagram, length)
                }
            }

            assertEquals(
                listOf(OutputFormat(sampleRate = 48_000L, packetFrames = FRAMES)),
                records(receive(pipeline, datagram, atMs = 0)),
            )
        }
    }

    @Test
    fun aClosedPipelineRefusesEveryCall() {
        val pipeline = NativeReceivePipeline(0L)
        pipeline.close()
        // Closing again is harmless.
        pipeline.close()

        assertThrows(IllegalStateException::class.java) { pipeline.reportDueAtMs() }
        assertThrows(IllegalStateException::class.java) { pipeline.onWake(wake(atMs = 0), null, 0) }
        assertThrows(IllegalStateException::class.java) { pipeline.onOutputFailed() }
    }

    // Helpers.

    /** A write's bytes, compared by content. */
    private data class Write(val bytes: List<Byte>) {
        constructor(bytes: ByteArray) : this(bytes.toList())
    }

    /** Every record of [outcome], as a value to compare: the bindings' types, or a short text. */
    private fun records(outcome: WakeOutcomeReader): List<Any> {
        val records = mutableListOf<Any>()
        while (outcome.next()) {
            records += when (outcome.kind) {
                WakeOutcomeReader.REPORT -> outcome.report()
                WakeOutcomeReader.WRITE -> {
                    val bytes = ByteArray(outcome.pcmSize)
                    outcome.pcm().get(bytes)
                    Write(bytes)
                }
                WakeOutcomeReader.SET_TARGET_FRAMES -> "target ${outcome.targetFrames}"
                WakeOutcomeReader.STATS -> "stats ${outcome.stats.packets}"
                WakeOutcomeReader.ERROR -> outcome.error()
                WakeOutcomeReader.OPEN_OUTPUT -> outcome.openOutput()
                else -> "unknown record ${outcome.kind}"
            }
        }
        return records
    }

    /** The first packet, played through an output opened as it asks. */
    private fun open(pipeline: NativeReceivePipeline) {
        receive(pipeline, packet(1uL, seed = 0), atMs = 0)
        records(pipeline.onOutputOpened(0L, 24_000L, playing()))
    }

    /** A wake-up at [atMs] that received [datagram], left in a socket-sized buffer. */
    private fun receive(pipeline: NativeReceivePipeline, datagram: ByteArray, atMs: Long): WakeOutcomeReader =
        pipeline.onWake(wake(atMs), received(datagram), datagram.size)

    /** [datagram] at the start of a buffer as large as the socket's, whose other bytes are not zero. */
    private fun received(datagram: ByteArray): ByteArray =
        ByteArray(65_507) { 0x5A }.also { datagram.copyInto(it) }

    /** A wake-up at [atMs] that plays the packet as sent (the desk owns the mix), with fresh readings after the receive. */
    private fun wake(atMs: Long, atStart: MutableOutputReadings? = null): WakeInput = WakeInput().apply {
        startedNs = atMs * 1_000_000
        receivedNs = atMs * 1_000_000
        hasOutputAtStart = atStart != null
        atStart?.let { copy(it, outputAtStart) }
        hasOutputAtReceive = true
        copy(playing(), outputAtReceive)
        serverOwnsMix = true
    }

    /** An output that takes everything, has presented nothing and plays. */
    private fun playing(underruns: Long = 0L): MutableOutputReadings = MutableOutputReadings().apply {
        underrunCount = underruns
        isPlaying = true
    }

    private fun copy(from: MutableOutputReadings, to: MutableOutputReadings) {
        to.hasWritableFrames = from.hasWritableFrames
        to.writableFrames = from.writableFrames
        to.underrunCount = from.underrunCount
        to.hasPresentedFrame = from.hasPresentedFrame
        to.presentedFrame = from.presentedFrame
        to.isPlaying = from.isPlaying
    }

    private fun packet(sequence: ULong, seed: Int): ByteArray =
        PmonFixtures.datagram(sequence, PmonFixtures.samples(seed))

    /** A PMON datagram with any header the test needs, its sample count the samples'. */
    private fun datagram(
        version: Int = 1,
        channels: Int = 2,
        sampleRate: Int = 48_000,
        sequence: ULong = 1uL,
        samples: ShortArray = PmonFixtures.samples(0),
    ): ByteArray {
        val bytes = ByteBuffer.allocate(20 + samples.size * 2).order(ByteOrder.LITTLE_ENDIAN)
            .put("PMON".toByteArray())
            .put(version.toByte())
            .put(channels.toByte())
            .putInt(sampleRate)
            .putLong(sequence.toLong())
            .putShort(samples.size.toShort())
        samples.forEach { bytes.putShort(it) }
        return bytes.array()
    }

    private companion object {
        val FRAMES = PmonFixtures.FRAMES.toLong()
    }
}
