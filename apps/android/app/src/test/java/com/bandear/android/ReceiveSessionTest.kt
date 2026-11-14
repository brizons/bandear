package com.bandear.android

import com.bandear.android.core.OutputFormat
import com.bandear.android.core.OutputReadings
import com.bandear.android.core.PmonFixtures
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The receive loop's wiring to the shared core ([ReceiveSession]), through the generated bindings
 * and the host build of the core, with a stand-in output and a clock the test moves: what a
 * datagram, a timeout or a refused datagram makes the session do to the output and report to the
 * listener. The pipeline's own rules are tested in Rust (`crates/bandear-core`).
 */
class ReceiveSessionTest {
    private var nowMs = 0L
    private var running = true
    private val listener = RecordingListener()
    private val mixState = ClientMixState()
    private val output = StandInOutput()
    private val formats = mutableListOf<OutputFormat>()
    private var opener: (OutputFormat) -> PlaybackOutput = { output }

    private fun session() = ReceiveSession(
        listener = listener,
        mixState = mixState,
        openOutput = { format ->
            formats += format
            opener(format)
        },
        isRunning = { running },
        nanoTime = { nowMs * 1_000_000 },
    )

    @Test
    fun theFirstPacketOpensTheOutputThenPlaysThroughIt() {
        session().use { session ->
            session.wake { packet(1uL, seed = 0) }

            assertEquals(listOf(OutputFormat(sampleRate = 48_000L, packetFrames = FRAMES)), formats)
            // The Auto cushion (two packets) is asked before the first frame, then the packet plays.
            assertEquals(listOf("target 256", "write 512"), output.events)
            assertArrayEquals(PmonFixtures.littleEndian(PmonFixtures.samples(0)), output.written.single())
            val stats = listener.stats.single()
            assertEquals(1L, stats.packets)
            assertEquals(48_000, stats.sampleRate)
            // The output presented nothing yet: one packet queued, 128 frames at 48 kHz.
            assertEquals(128 * 1000.0 / 48_000, stats.playbackBacklogMs, 1e-9)
            // Not measured before a second arrival reads as 0, as the receiver always showed it.
            assertEquals(0.0, stats.interArrivalMs, 0.0)
            assertEquals(emptyList<String>(), listener.errors)
        }
    }

    @Test
    fun aDatagramIsWrittenAsTheBytesItCarried() {
        session().use { session ->
            session.wake { packet(1uL, seed = 0) }
            nowMs = 3
            session.wake { packet(2uL, seed = 1) }

            assertEquals(1, formats.size)
            assertEquals(2, output.written.size)
            assertArrayEquals(PmonFixtures.littleEndian(PmonFixtures.samples(1)), output.written[1])
            assertEquals(listOf(1L, 2L), listener.stats.map { it.packets })
            assertEquals(3.0, listener.stats[1].interArrivalMs, 1e-9)
        }
    }

    @Test
    fun aTimeoutPlaysNothingAndCarriesTheReportOnceDue() {
        // The output applies a cushion a little larger than the one asked.
        output.applies = 300L
        session().use { session ->
            session.wake { packet(1uL, seed = 0) }
            output.underruns = 2L
            nowMs = 1_000
            session.wake { null }

            assertEquals(1, output.written.size)
            assertEquals(1, listener.stats.size)
            val report = listener.reports.single()
            assertEquals(1L, report.packetsReceived)
            assertEquals(0L, report.packetsLost)
            // The output was read at the wake-up's start for the report.
            assertEquals(2L, report.underruns)
            // The cushion is the one the output applied, 300 frames at 48 kHz.
            assertEquals(6.25, report.cushionMs, 1e-9)
            // 128 frames written, none presented.
            assertEquals(128 * 1000.0 / 48_000, report.backlogMs, 1e-9)
            assertEquals(emptyList<String>(), listener.errors)
        }
    }

    @Test
    fun anUndecodableDatagramIsReportedAndPlaysNothing() {
        session().use { session ->
            val notPmon = packet(1uL, seed = 0)
            notPmon[3] = 'X'.code.toByte()
            session.wake { notPmon }

            assertEquals(listOf("invalid PMON magic"), listener.errors)
            assertTrue(formats.isEmpty())
            assertTrue(output.events.isEmpty())
            assertTrue(listener.stats.isEmpty())
        }
    }

    @Test
    fun anOutputThatFailsToOpenIsReportedAndTheNextPacketAsksAgain() {
        opener = { throw IllegalStateException("sample rate 48000 is not supported") }
        session().use { session ->
            session.wake { packet(1uL, seed = 0) }

            assertEquals(listOf("sample rate 48000 is not supported"), listener.errors)
            assertTrue(listener.stats.isEmpty())

            opener = { output }
            nowMs = 3
            session.wake { packet(2uL, seed = 1) }

            assertEquals(2, formats.size)
            assertArrayEquals(PmonFixtures.littleEndian(PmonFixtures.samples(1)), output.written.single())
            assertEquals(1, listener.errors.size)
        }
    }

    @Test
    fun thePhonesControlsAndTheDesksBufferApplyFromTheMixState() {
        listener.ownsMix = false
        mixState.muted = true
        mixState.deskBufferMs = 40
        session().use { session ->
            session.wake { packet(1uL, seed = 0) }

            // 40 ms at 48 kHz in whole packets of 128 frames: 15 packets.
            assertEquals("target 1920", output.events.first())
            // Muted on the phone, while the desk does not own the mix yet: silence.
            assertArrayEquals(ByteArray(FRAMES.toInt() * 4), output.written.single())
        }
    }

    @Test
    fun whatWokeAStoppedReceiveIsNotAudio() {
        session().use { session ->
            session.wake {
                running = false
                packet(1uL, seed = 0)
            }

            assertTrue(formats.isEmpty())
            assertTrue(listener.stats.isEmpty())
            assertTrue(listener.errors.isEmpty())
        }
    }

    @Test
    fun aFailingOutputIsReportedAndTheNextPacketStillPlays() {
        output.failsToWrite = true
        session().use { session ->
            session.wake { packet(1uL, seed = 0) }
            assertEquals(listOf("the track is gone"), listener.errors)

            output.failsToWrite = false
            nowMs = 3
            session.wake { packet(2uL, seed = 1) }
            assertArrayEquals(PmonFixtures.littleEndian(PmonFixtures.samples(1)), output.written.single())
        }
    }

    @Test
    fun closingTheSessionClosesTheOutput() {
        val session = session()
        session.wake { packet(1uL, seed = 0) }
        session.close()

        assertTrue(output.closed)
    }

    private fun packet(sequence: ULong, seed: Int): ByteArray =
        PmonFixtures.datagram(sequence, PmonFixtures.samples(seed))

    private companion object {
        val FRAMES = PmonFixtures.FRAMES.toLong()
    }
}

/** An output that takes every write, records what it was asked, and reads as the test sets it. */
private class StandInOutput : PlaybackOutput {
    override val capacityFrames: Long = 24_000L
    var underruns = 0L
    var presented: Long? = 0L

    /** The frames it answers to a target; null applies the one asked. */
    var applies: Long? = null
    var failsToWrite = false
    var closed = false
    val events = mutableListOf<String>()
    val written = mutableListOf<ByteArray>()

    override fun readings(): OutputReadings =
        OutputReadings(writableFrames = null, underrunCount = underruns, presentedFrame = presented, isPlaying = true)

    override fun write(pcm16LeBytes: ByteArray) {
        if (failsToWrite) throw IllegalStateException("the track is gone")
        events += "write ${pcm16LeBytes.size}"
        written += pcm16LeBytes
    }

    override fun setTargetFrames(frames: Long): Long {
        events += "target $frames"
        return applies ?: frames
    }

    override fun close() {
        closed = true
    }
}

/** What the receiver showed: [PmonPlayer.Listener.onStats]'s values, one call each. */
private data class Shown(
    val packets: Long,
    val lost: Long,
    val sampleRate: Int?,
    val packetsPerSecond: Double,
    val interArrivalMs: Double,
    val jitterMs: Double,
    val discardedSamples: Long,
    val packetsWithDiscardedSamples: Long,
    val playbackBacklogMs: Double,
)

private class RecordingListener : PmonPlayer.Listener {
    var ownsMix = true
    val stats = mutableListOf<Shown>()
    val reports = mutableListOf<LinkStatsReport>()
    val errors = mutableListOf<String>()

    override fun serverOwnsMix(): Boolean = ownsMix

    override fun onStats(
        packets: Long,
        lost: Long,
        sampleRate: Int?,
        packetsPerSecond: Double,
        interArrivalMs: Double,
        jitterMs: Double,
        discardedSamples: Long,
        packetsWithDiscardedSamples: Long,
        playbackBacklogMs: Double,
    ) {
        stats += Shown(
            packets,
            lost,
            sampleRate,
            packetsPerSecond,
            interArrivalMs,
            jitterMs,
            discardedSamples,
            packetsWithDiscardedSamples,
            playbackBacklogMs,
        )
    }

    override fun onLinkStats(report: LinkStatsReport) {
        reports += report
    }

    override fun onError(message: String) {
        errors += message
    }

    override fun onStopped() = Unit
}
