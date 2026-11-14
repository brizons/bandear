package com.bandear.android

import com.bandear.android.core.MutableOutputReadings
import com.bandear.android.core.OutputFormat
import com.bandear.android.core.PmonFixtures
import java.nio.ByteBuffer
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The receive loop's wiring to the shared core ([ReceiveSession]), through its JNI functions and
 * the host build of the core, with a stand-in output and a clock the test moves: what a
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

    @Test
    fun whatTheSocketAlreadyHoldsGoesThroughTheCoreAndPlaysInOrder() {
        session().use { session ->
            session.wake { packet(1uL, seed = 0) }
            nowMs = 3
            val waiting = ArrayDeque(listOf(packet(3uL, seed = 2), packet(4uL, seed = 3)))
            session.wake(receiveWaiting = { waiting.removeFirstOrNull() }) { packet(2uL, seed = 1) }

            assertEquals(
                (0..3).map { PmonFixtures.littleEndian(PmonFixtures.samples(it)).toList() },
                output.written.map { it.toList() },
            )
            assertEquals(listOf(1L, 2L, 3L, 4L), listener.stats.map { it.packets })
            assertTrue(waiting.isEmpty())
        }
    }

    /**
     * Main's `PlaybackLatencyTest.theBacklogReportedCountsTheAudioQueuedBeforeTheTrackToo`, where
     * the backlog is reported now: the shared core's link report, through the session.
     */
    @Test
    fun theBacklogReportedCountsTheAudioQueuedBeforeTheTrackToo() {
        // 240-frame packets at 48 kHz are 5 ms each; a fixed 15 ms buffer is a 3-packet cushion.
        mixState.deskBufferMs = 15
        session().use { session ->
            // 480 frames on the track plus 960 waiting before it are 1 440 frames: 30 ms at 48 kHz.
            val report = reportWithFourPacketsQueued(session, output, presented = 240L)
            assertEquals(30.0, report.backlogMs, 1e-9)
            // The cushion is the aim, not what is queued: it does not move.
            assertEquals(15.0, report.cushionMs, 1e-9)
        }
        // Not measured while the track's own position is unknown, queue or not.
        listener.reports.clear()
        val unknown = StandInOutput()
        opener = { unknown }
        session().use { session ->
            assertTrue(reportWithFourPacketsQueued(session, unknown, presented = null).backlogMs.isNaN())
        }
    }

    @Test
    fun aSurplusThatPersistsIsSplicedOutOfThePcmTheCoreHandsOver() {
        // 240-frame packets at 48 kHz (5 ms each) and a fixed 15 ms buffer: the track holds 720
        // frames, and it is always full, having played all but those of what it was written.
        mixState.deskBufferMs = 15
        output.lagFrames = 720L
        val packets = 800
        var sent = 1
        var giveOne = true
        val burst = ArrayDeque((2..5).map { long(it.toULong()) })
        // Four packets wait behind the first one, and then one more arrives during each write: a
        // surplus that persists, as a faster desk clock leaves it.
        val receiveWaiting = {
            burst.removeFirstOrNull()?.also { sent++ } ?: if (giveOne && sent < packets) {
                giveOne = false
                sent++
                nowMs += 5
                long(sent.toULong())
            } else {
                giveOne = true
                null
            }
        }
        session().use { session ->
            session.wake(receiveWaiting) { long(1uL) }
        }

        assertEquals(packets, output.written.size)
        val trimmed = output.written.indices.filter { output.written[it].size != FRAMES_240 * 4 }
        assertTrue("the surplus was never trimmed", trimmed.isNotEmpty())
        // It acts only once the surplus has persisted for two seconds of audio.
        assertTrue(trimmed.first() >= DriftTrimmer.WINDOW_SECONDS * 48_000 / FRAMES_240)
        output.written.forEachIndexed { index, bytes ->
            val samples = samples240((index + 1).toULong())
            val expected = if (index in trimmed) PcmSplice.removeFrames(samples, 2, 8) else samples
            assertArrayEquals("packet ${index + 1}", PmonFixtures.littleEndian(expected), bytes)
        }
        // Never more than 0.05% of the audio played, give or take the one splice banked.
        val trimmedFrames = trimmed.size * 8L
        assertTrue(trimmedFrames * 1_000_000 <= packets * FRAMES_240 * DriftTrimmer.MAX_TRIM_PPM + 8 * 1_000_000)
        // The frames trimmed are not counted as still to play: the backlog reported stays the
        // track's 720 frames plus the five packets waiting before it, 40 ms, before and after the
        // trims.
        assertTrue(listener.reports.size >= 3)
        listener.reports.forEach { assertEquals(40.0, it.backlogMs, 1e-9) }
        assertTrue(listener.reports.last().intervalMs > 0)
    }

    @Test
    fun aQueueOverItsBoundGivesUpTheOldestAudioAndCountsItAsDiscarded() {
        // 102 packets of 240 frames wait at once, 24 480 frames: past the 24 000 kept, the two
        // oldest are given up on.
        val waiting = ArrayDeque((2..102).map { long(it.toULong()) })
        session().use { session ->
            session.wake(receiveWaiting = { waiting.removeFirstOrNull() }) { long(1uL) }
        }

        assertEquals(100, output.written.size)
        assertArrayEquals(PmonFixtures.littleEndian(samples240(3uL)), output.written.first())
        val shown = listener.stats.last()
        assertEquals(2L * FRAMES_240 * 2, shown.discardedSamples)
        assertEquals(2L, shown.packetsWithDiscardedSamples)
    }

    /**
     * Plays three 240-frame packets through [track], which then reads as having [presented] the
     * first 240 frames (or as not able to say, on null), and wakes once more just before the report
     * is due: that packet and three more already waiting are queued before the track when a fifth,
     * waiting too, reaches the core as the report falls due. Answers that report.
     */
    private fun reportWithFourPacketsQueued(
        session: ReceiveSession,
        track: StandInOutput,
        presented: Long?,
    ): LinkStatsReport {
        for (sequence in 1uL..3uL) {
            nowMs = (sequence.toLong() - 1) * 5
            session.wake { long(sequence) }
        }
        track.presented = presented
        nowMs = 999
        val waiting = ArrayDeque((5..7).map { long(it.toULong()) })
        session.wake(
            receiveWaiting = {
                waiting.removeFirstOrNull() ?: if (nowMs < 1_000) {
                    nowMs = 1_000
                    long(8uL)
                } else {
                    null
                }
            },
        ) { long(4uL) }
        return listener.reports.single()
    }

    private fun packet(sequence: ULong, seed: Int): ByteArray =
        PmonFixtures.datagram(sequence, PmonFixtures.samples(seed))

    /**
     * One wake-up whose receive answers [receive]'s datagram, or times out on null; the datagram
     * lands at the start of a socket-sized buffer, as the receive loop's does.
     */
    private fun ReceiveSession.wake(receiveWaiting: () -> ByteArray? = { null }, receive: () -> ByteArray?) {
        val buffer = ByteArray(65_507)
        wake(
            buffer,
            receiveWaiting = {
                receiveWaiting()?.also { it.copyInto(buffer) }?.size ?: ReceiveSession.NOTHING_WAITING
            },
        ) { receive()?.also { it.copyInto(buffer) }?.size ?: ReceiveSession.TIMED_OUT }
    }

    /** Packet [sequence] of 240 stereo frames: 5 ms at 48 kHz, the samples a ramp from it. */
    private fun long(sequence: ULong): ByteArray = PmonFixtures.datagram(sequence, samples240(sequence))

    private fun samples240(sequence: ULong): ShortArray =
        ShortArray(FRAMES_240 * 2) { ((it * 31 + sequence.toInt() * 7) % 20_000 - 10_000).toShort() }

    private companion object {
        val FRAMES = PmonFixtures.FRAMES.toLong()
        const val FRAMES_240 = 240
    }
}

/** An output that takes every write, records what it was asked, and reads as the test sets it. */
private class StandInOutput : PlaybackOutput {
    override val capacityFrames: Long = 24_000L
    var underruns = 0L
    var presented: Long? = 0L

    /** When set, it presents all but this many of the frames written so far, and [presented] is ignored. */
    var lagFrames: Long? = null

    /** The frames it answers to a target; null applies the one asked. */
    var applies: Long? = null
    var failsToWrite = false
    var closed = false
    val events = mutableListOf<String>()
    val written = mutableListOf<ByteArray>()
    private var writtenFrames = 0L

    override fun read(readings: MutableOutputReadings) {
        val lag = lagFrames
        val frame = if (lag != null) maxOf(writtenFrames - lag, 0L) else presented
        readings.hasWritableFrames = false
        readings.underrunCount = underruns
        readings.hasPresentedFrame = frame != null
        readings.presentedFrame = frame ?: 0L
        readings.isPlaying = true
    }

    override fun write(pcm: ByteBuffer, size: Int) {
        if (failsToWrite) throw IllegalStateException("the track is gone")
        events += "write $size"
        written += ByteArray(size).also { pcm.get(it) }
        writtenFrames += size / 4
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
