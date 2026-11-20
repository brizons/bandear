package com.bandear.android

import com.bandear.android.core.MutableOutputReadings
import com.bandear.android.core.OutputFormat
import com.bandear.android.core.PmonFixtures
import com.bandear.android.core.ReceiveError
import java.nio.ByteBuffer
import java.nio.ByteOrder
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
    private val wifi = StandInWifiLock()
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
        holdWifi = wifi::hold,
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

    /** The words as they were when the limit was Android's own constant, now the shared core's. */
    @Test
    fun aPacketWithTooManyChannelsIsReportedWithTheLimit() {
        assertEquals(
            "PMON packet announces 33 channels, the limit is 32",
            ReceiveError.TooManyChannels(33L).describe(),
        )
    }

    @Test
    fun closingTheSessionClosesTheOutput() {
        val session = session()
        session.wake { packet(1uL, seed = 0) }
        session.close()

        assertTrue(output.closed)
    }

    @Test
    fun theSessionHoldsTheWifiLockFromItsStartUntilItCloses() {
        val session = session()
        assertEquals(listOf("acquire"), wifi.events)

        session.wake { packet(1uL, seed = 0) }
        nowMs = 300
        session.wake { null }
        // Packets and timeouts neither take it again nor give it back.
        assertEquals(listOf("acquire"), wifi.events)

        session.close()
        assertEquals(listOf("acquire", "release"), wifi.events)
    }

    @Test
    fun theWifiLockIsGivenBackWhenTheOutputFailsToClose() {
        output.failsToClose = true
        val session = session()
        session.wake { packet(1uL, seed = 0) }

        val failure = runCatching { session.close() }.exceptionOrNull()

        assertEquals("the track would not close", failure?.message)
        assertEquals(listOf("acquire", "release"), wifi.events)
    }

    @Test
    fun aWifiLockThePhoneRefusesLeavesTheSessionPlaying() {
        wifi.refuses = true
        session().use { session ->
            session.wake { packet(1uL, seed = 0) }

            assertArrayEquals(PmonFixtures.littleEndian(PmonFixtures.samples(0)), output.written.single())
            assertEquals(emptyList<String>(), listener.errors)
        }
        // Nothing was held, so nothing is given back.
        assertEquals(emptyList<String>(), wifi.events)
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

    @Test
    fun aWriteTheTrackRefusesCountsAsDiscardedAndNotAsQueued() {
        // The track answers an error code (AudioTrack.ERROR_DEAD_OBJECT) for the first packet.
        output.answers = { -6 }
        session().use { session ->
            session.wake { packet(1uL, seed = 0) }
            output.answers = null
            nowMs = 3
            session.wake { packet(2uL, seed = 1) }

            // The first packet's 128 frames never reached the track.
            assertEquals(FRAMES * 2, listener.stats.last().discardedSamples)
            assertEquals(1L, listener.stats.last().packetsWithDiscardedSamples)
            nowMs = 1_000
            session.wake { null }
            // Only the second packet waits: 128 frames, not 256.
            assertEquals(128 * 1000.0 / 48_000, listener.reports.single().backlogMs, 1e-9)
        }
    }

    @Test
    fun aShortWriteCountsWhatTheTrackDidNotTakeAsDiscarded() {
        // The track takes 200 of the packet's 512 bytes: 50 frames, and 78 are lost.
        output.answers = { 200 }
        session().use { session ->
            session.wake { packet(1uL, seed = 0) }
            output.answers = null
            nowMs = 3
            session.wake { packet(2uL, seed = 1) }

            assertEquals(78L * 2, listener.stats.last().discardedSamples)
            assertEquals(1L, listener.stats.last().packetsWithDiscardedSamples)
            nowMs = 1_000
            session.wake { null }
            assertEquals((50 + 128) * 1000.0 / 48_000, listener.reports.single().backlogMs, 1e-9)
        }
    }

    // A Wi-Fi stall: the datagrams sent while it lasts arrive together when it ends. A surplus is
    // caught up at once only once it has stood through a catch-up window (CATCH_UP_WINDOW_MS).

    @Test
    fun aStallsBurstIsCaughtUpWithinASecondAndTheBacklogReportedDropsWithIt() {
        // A fixed 25 ms buffer: five packets of 240 frames, 1 200 frames. A 200 ms stall at 2 s
        // brings 41 datagrams at once, 175 ms more than the buffer behind the first.
        mixState.deskBufferMs = 25
        val track = RealTimeTrack()
        playThroughAStall(track, seconds = 6, stallAtMs = 2_000, stallMs = 200)

        // From a second after the stall on, the backlog reported is the buffer and the cushion the
        // cut leaves standing, 50 ms, not the 175 ms more the burst brought...
        assertBacklogBackToTheBufferAndTheCushionLeft(stallEndMs = 2_200)
        // ...because once the surplus had stood through the window that ends with packet 500, 400
        // of them after the stall, 30 packets went at once: all of it but the cushion, from 500 on.
        assertReceived(track, packets = 1_200, head = 500, kept = 530)
        assertEquals(30L * FRAMES_240 * 2, listener.stats.last().discardedSamples)
        assertTrue(track.underrunsAtMs.all { it in 2_000..2_200 })
    }

    @Test
    fun aStallBelowTheCatchUpPlaysEveryPacketAndOnlyTheSlowTrimActs() {
        // The same 25 ms buffer and a 50 ms stall: 11 datagrams at once, 20 ms more than the
        // buffer, less than the 30 ms the catch-up needs.
        mixState.deskBufferMs = 25
        val track = RealTimeTrack()
        playThroughAStall(track, seconds = 6, stallAtMs = 2_000, stallMs = 50)

        // Every packet plays, whole or spliced by the slow trim, and nothing is discarded.
        assertEquals(1_200, track.received.size)
        val trimmed = track.received.indices.filter { track.received[it].size != FRAMES_240 * 2 }
        track.received.forEachIndexed { index, samples ->
            val sent = samples240((index + 1).toULong())
            val expected = if (index in trimmed) PcmSplice.removeFrames(sent, 2, 8) else sent
            assertArrayEquals("packet ${index + 1}", expected, samples)
        }
        assertTrue("the slow trim never acted", trimmed.isNotEmpty())
        assertTrue("it trimmed before the stall", trimmed.first() >= 400)
        assertEquals(0L, listener.stats.last().discardedSamples)
    }

    @Test
    fun aLongStallIsCaughtUpWithinASecondOnceTheBoundTookItsShare() {
        // A 700 ms stall: 141 datagrams at once. The queue's bound keeps the newest 500 ms of
        // them, and the rest of the surplus is caught up once it has stood through a window.
        mixState.deskBufferMs = 25
        val track = RealTimeTrack()
        playThroughAStall(track, seconds = 6, stallAtMs = 2_000, stallMs = 700)

        assertBacklogBackToTheBufferAndTheCushionLeft(stallEndMs = 2_700)
        assertReceived(track, packets = 1_200, head = 541, kept = 630, boundDropped = 401..441)
        assertEquals((41L + 89) * FRAMES_240 * 2, listener.stats.last().discardedSamples)
        assertTrue(track.underrunsAtMs.all { it in 2_000..2_700 })
    }

    @Test
    fun aBacklogStandingFromTheStartIsCaughtUpWithinASecond() {
        // The first 250 ms arrive at once, as when the audio piles up while the track opens.
        mixState.deskBufferMs = 25
        val track = RealTimeTrack()
        playThroughAStall(track, seconds = 6, stallAtMs = 0, stallMs = 250)

        // Caught up once the first window ends, with packet 100, at 720 ms.
        assertBacklogBackToTheBufferAndTheCushionLeft(stallEndMs = 250)
        assertReceived(track, packets = 1_200, head = 100, kept = 140)
        assertEquals(40L * FRAMES_240 * 2, listener.stats.last().discardedSamples)
        assertTrue(track.underrunsAtMs.isEmpty())
    }

    /**
     * The device's case (a Galaxy S23, Auto cushion, 256-frame packets): the track's blocking write
     * returns only at its mixer's ticks, so 4 to 8 packets pile up before it each time, and the
     * datagrams arrive 1 to 3 ms late. A minute of that is never caught up and never runs dry.
     */
    @Test
    fun aTrackWhoseWritesReturnInCoarseStepsIsNeverCaughtUpAndNeverRunsDry() {
        for (periodMs in listOf(20L, 40L)) {
            listener.stats.clear()
            listener.reports.clear()
            listener.reportTimesMs.clear()
            val track = CoarseTrack(periodFrames = periodMs * FRAMES_PER_MS)
            playThroughAStall(track, seconds = 60, stallAtMs = 0, stallMs = 0, frames = FRAMES_256, jitter = ::jitter)

            assertEquals("underruns with a $periodMs ms period", emptyList<Long>(), track.underrunsAtMs)
            assertEquals("discarded with a $periodMs ms period", 0L, listener.stats.last().discardedSamples)
            assertEquals(60 * 48_000 / FRAMES_256, track.received.size)
            track.received.forEachIndexed { index, samples -> assertArrayEquals(ramp(index + 1, FRAMES_256), samples) }
        }
    }

    @Test
    fun aStallUnderCoarseStepsIsCaughtUpAndNothingRunsDryAfterTheCut() {
        // A fixed 25 ms buffer, so the cushion does not move with the stall's own underrun.
        mixState.deskBufferMs = 25
        for (stallMs in listOf(200L, 700L)) {
            listener.stats.clear()
            listener.reports.clear()
            listener.reportTimesMs.clear()
            val track = CoarseTrack(periodFrames = 40 * FRAMES_PER_MS)
            playThroughAStall(track, seconds = 8, stallAtMs = 3_000, stallMs = stallMs, frames = FRAMES_256, jitter = ::jitter)

            val stallEndMs = 3_000 + stallMs
            val discarded = listener.stats.last().discardedSamples / 2
            assertTrue("$stallMs ms: nothing caught up", discarded > 0)
            assertEquals("$stallMs ms: whole packets", 0L, discarded % FRAMES_256)
            // The stall itself runs the track dry; nothing does after it.
            assertTrue("$stallMs ms: ran dry at ${track.underrunsAtMs}", track.underrunsAtMs.all { it in 3_000..stallEndMs })
            // A second after it, the backlog is back where it was before it but for the cushion the
            // catch-up leaves standing before the track (25 ms, and less than a packet more, as whole
            // packets go) and the room the track had left at the tightest moment before the stall,
            // which it no longer has: the audio on the track is measured no higher than the cushion
            // (as the device's must be), so that room is not seen. The slow trim gives them back. A
            // report also reads up to the arrivals' jitter (2 ms, from 1 to 3) more or less than
            // another. And the 8 packets a 40 ms step piles up before each write are a bunch that
            // recurs ([BunchReserve]): the cut leaves 7 of them standing too, latency a track that
            // rides its own steps does not need.
            val before = listener.reports.indices.filter { listener.reportTimesMs[it] in 1_000 until 3_000 }
            val after = listener.reports.indices.filter { listener.reportTimesMs[it] >= stallEndMs + 1_000 }
            assertTrue(before.isNotEmpty() && after.isNotEmpty())
            val backlog = listener.reports.map { it.backlogMs }
            val highest = before.maxOf { backlog[it] } + 25.0 + (2 + 7) * FRAMES_256 / 48.0 + 2.0
            after.forEach { assertTrue("$stallMs ms: backlog $backlog at ${listener.reportTimesMs}", backlog[it] <= highest + 1e-9) }
        }
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

    /**
     * The margin a catch-up leaves: at the window's tightest point the queue keeps the cushion, not
     * one packet. On a FAST track whose buffer is 256 frames (the device's at the smallest Auto
     * cushion), the track itself rides out about 5 ms of an arrival dip, so what the cut leaves
     * standing before it decides whether a dip rarer than the window runs it dry.
     *
     * A fixed 21 ms buffer is a 4-packet cushion of 256-frame packets, 1 024 frames (21.33 ms,
     * Auto's largest). After a 250 ms stall's surplus is cut, every 1 to 3 s the datagrams of a
     * 21 ms span arrive late, together, on top of 1 to 3 ms of jitter. Each dip is within the
     * cushion: the cushion left standing rides it out. One packet left standing (5.33 ms) does not;
     * in the model that chose these numbers it ran the track dry from 17 ms dips on, and the cushion
     * from 33 ms on, whatever the seed.
     */
    @Test
    fun aCutLeavesTheCushionStandingSoADipWithinItNeverRunsTheTrackDry() {
        mixState.deskBufferMs = 21
        for (seed in 1L..4L) {
            listener.stats.clear()
            listener.reports.clear()
            listener.reportTimesMs.clear()
            val random = java.util.Random(seed)
            val dips = mutableListOf<LongRange>()
            var atMs = 2_750L + random.nextInt(2_000)
            while (atMs < 11_500) {
                dips += atMs..atMs + 21
                atMs += 1_000 + random.nextInt(2_000)
            }
            val track = FastTrack()
            playThroughAStall(
                track, seconds = 12, stallAtMs = 1_500, stallMs = 250, frames = FRAMES_256,
                jitter = ::jitter, lateBunchesMs = dips,
            )

            assertTrue("seed $seed: nothing caught up", listener.stats.last().discardedSamples > 0)
            assertTrue("seed $seed: only ${dips.size} dips", dips.size >= 3)
            // The stall itself runs the track dry (and its start may, with one packet ahead of so
            // small a buffer); no dip does after the cut.
            assertEquals("seed $seed: ran dry, dips at $dips", emptyList<Long>(), track.underrunsAtMs.filter { it > 1_750 })
        }
    }

    /**
     * The Galaxy S23's Wi-Fi as measured (spec L11): for 10 to 15 s at a time it hands the datagrams
     * over in bunches about every 100 ms, one beacon interval, between stretches of one at a time,
     * with the low-latency Wi-Fi lock held. A catch-up that leaves only the cushion standing in a
     * regular stretch runs the track dry at the next bunched stretch's gaps; one that leaves the
     * bunches' depth standing does not, and still catches up a stall's surplus.
     *
     * Auto cushion, 256-frame packets, the FAST track. The first 250 ms arrive at once, as when the
     * audio piles up while the track opens; bunched from 4 to 14 s and from 30 to 40 s; a 300 ms
     * stall at 46 s.
     */
    @Test
    fun aCutLeavesTheWiFisBunchDepthStandingSoTheNextBunchedStretchNeverRunsDry() {
        fun bunched(fromMs: Long, untilMs: Long) = (fromMs until untilMs step 100).map { it..it + 100 }
        val late = bunched(4_000, 14_000) + bunched(30_000, 40_000) + listOf(46_000L..46_300L)
        val track = FastTrack()
        playThroughAStall(
            track, seconds = 50, stallAtMs = 0, stallMs = 250, frames = FRAMES_256,
            jitter = ::jitter, lateBunchesMs = late,
        )

        // The first bunched stretch, before its depth was seen, and the stall may run it dry;
        // nothing else does, the second bunched stretch included.
        val dry = track.underrunsAtMs.filter { it !in 0..14_100 && it !in 46_000..46_400 }
        assertEquals("ran dry at ${track.underrunsAtMs}", emptyList<Long>(), dry)
        // The stall's surplus is caught up: from 1.5 s after it, the backlog is no higher than the
        // highest of the five seconds before it, but for Auto's cushion grown by the stall's own
        // underruns (at most 4 packets), whole packets going (one more) and the jitter (2 ms).
        // Played whole, the 200 ms the stall ran the track dry for would stand on top.
        val backlog = listener.reports.map { it.backlogMs }
        val before = listener.reports.indices.filter { listener.reportTimesMs[it] in 41_000 until 46_000 }
        val after = listener.reports.indices.filter { listener.reportTimesMs[it] >= 47_800 }
        assertTrue(before.isNotEmpty() && after.isNotEmpty())
        val highest = before.maxOf { backlog[it] } + 5 * FRAMES_256 / 48.0 + 2.0
        after.forEach { assertTrue("backlog $backlog at ${listener.reportTimesMs}", backlog[it] <= highest) }
    }

    /**
     * Plays [seconds] of packets of [frames] frames, sent back to back at 48 kHz, through a session
     * whose output is [track] and whose clock is the track's. Each arrives [jitter] frames after it
     * was sent, except the datagrams sent during the [stallMs] that starts at [stallAtMs]: they
     * arrive together when it ends, as a Wi-Fi stall delivers them, and so do those sent during each of
     * the [lateBunchesMs] (from its first to its last millisecond). A receive that waits 250 ms for
     * nothing times out, as the receive loop's does.
     */
    private fun playThroughAStall(
        track: ClockedTrack,
        seconds: Int,
        stallAtMs: Long,
        stallMs: Long,
        frames: Int = FRAMES_240,
        jitter: (Int) -> Long = { 0L },
        lateBunchesMs: List<LongRange> = emptyList(),
    ) {
        val packets = seconds * 48_000 / frames
        fun arrival(sequence: Int): Long {
            val sent = (sequence - 1).toLong() * frames
            val stallEnd = (stallAtMs + stallMs) * FRAMES_PER_MS
            if (sent >= stallAtMs * FRAMES_PER_MS && sent < stallEnd) return stallEnd
            val bunch = lateBunchesMs.firstOrNull { sent >= it.first * FRAMES_PER_MS && sent < it.last * FRAMES_PER_MS }
            return if (bunch != null) bunch.last * FRAMES_PER_MS else sent + jitter(sequence)
        }
        fun datagram(sequence: Int) = PmonFixtures.datagram(sequence.toULong(), ramp(sequence, frames))
        var next = 1
        listener.clockMs = { track.clock / FRAMES_PER_MS }
        ReceiveSession(
            listener = listener,
            mixState = mixState,
            openOutput = { track },
            isRunning = { running },
            holdWifi = wifi::hold,
            nanoTime = { track.clock * 1_000_000 / FRAMES_PER_MS },
        ).use { session ->
            while (next <= packets) {
                session.wake(
                    receiveWaiting = {
                        if (next <= packets && arrival(next) <= track.clock) datagram(next++) else null
                    },
                ) {
                    if (arrival(next) > track.clock + 250 * FRAMES_PER_MS) {
                        track.clock += 250 * FRAMES_PER_MS
                        null
                    } else {
                        track.clock = maxOf(track.clock, arrival(next))
                        datagram(next++)
                    }
                }
            }
        }
    }

    /** 1 to 3 ms of arrival delay, the same for a packet on every run. */
    private fun jitter(sequence: Int): Long = FRAMES_PER_MS + (sequence * 2_654_435_761L).mod(97L)

    /**
     * [track] received the [packets] sent, in order, but for those given up on: [boundDropped] at
     * the queue's bound, and a catch-up's from [head] to the packet before [kept], whose slot holds
     * [head]'s first frames fading into [kept] ([PcmSplice]). The slow trim may have spliced 8 frames
     * out of any of them.
     */
    private fun assertReceived(track: ClockedTrack, packets: Int, head: Int, kept: Int, boundDropped: IntRange = IntRange.EMPTY) {
        val expected = (1..packets).mapNotNull { sequence ->
            when {
                sequence in boundDropped -> null
                sequence == head -> PcmSplice.removeFrames(ramp(head, FRAMES_240) + ramp(kept, FRAMES_240), 2, FRAMES_240)
                sequence in head + 1..kept -> null
                else -> ramp(sequence, FRAMES_240)
            }
        }
        assertEquals(expected.size, track.received.size)
        expected.forEachIndexed { index, samples ->
            val received = track.received[index]
            assertTrue(
                "packet $index received",
                received.contentEquals(samples) || received.contentEquals(PcmSplice.removeFrames(samples, 2, 8)),
            )
        }
    }

    /**
     * Every report before the stall reads the buffer and at most one packet more, 20 to 30 ms; from
     * a second after its end on, the buffer on the track and the cushion the cut left standing
     * before it, 50 ms, less what the slow trim gave back since (half a millisecond a second at
     * most): 45 to 50 ms.
     */
    private fun assertBacklogBackToTheBufferAndTheCushionLeft(stallEndMs: Long) {
        val after = listener.reports.indices.filter { listener.reportTimesMs[it] >= stallEndMs + 1_000 }
        assertTrue("no report a second after the stall", after.size >= 2)
        val backlog = listener.reports.map { it.backlogMs }
        val stallStartMs = if (stallEndMs <= 250) 0 else 2_000
        for (index in listener.reports.indices) {
            val atMs = listener.reportTimesMs[index]
            val expected = when {
                atMs < stallStartMs -> 20.0..30.0
                atMs >= stallEndMs + 1_000 -> 45.0..50.0
                else -> continue
            }
            assertTrue("backlog $backlog ms at ${listener.reportTimesMs}", backlog[index] in expected)
        }
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

    private fun samples240(sequence: ULong): ShortArray = ramp(sequence.toInt(), FRAMES_240)

    /** Packet [sequence]'s samples, [frames] stereo frames: a ramp from it. */
    private fun ramp(sequence: Int, frames: Int): ShortArray =
        ShortArray(frames * 2) { ((it * 31 + sequence * 7) % 20_000 - 10_000).toShort() }

    private companion object {
        val FRAMES = PmonFixtures.FRAMES.toLong()
        const val FRAMES_240 = 240

        /** The device's packets: 256 frames, 5.33 ms at 48 kHz. */
        const val FRAMES_256 = 256

        /** Frames per millisecond at 48 kHz: the stall tests' clock counts frames. */
        const val FRAMES_PER_MS = 48L
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
    var failsToClose = false

    /** What a write answers for the bytes it was handed; null takes them all. */
    var answers: ((Int) -> Int)? = null
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

    override fun write(pcm: ByteBuffer, size: Int): Int {
        if (failsToWrite) throw IllegalStateException("the track is gone")
        events += "write $size"
        val taken = answers?.invoke(size) ?: size
        written += ByteArray(size).also { pcm.get(it) }
        writtenFrames += taken.coerceAtLeast(0) / 4
        return taken
    }

    override fun setTargetFrames(frames: Long): Long {
        events += "target $frames"
        return applies ?: frames
    }

    override fun close() {
        closed = true
        if (failsToClose) throw IllegalStateException("the track would not close")
    }
}

/** A Wi-Fi lock that records when it is taken and given back, or that the phone refuses. */
private class StandInWifiLock {
    var refuses = false
    val events = mutableListOf<String>()

    fun hold(): AutoCloseable {
        if (refuses) throw SecurityException("Neither user 10123 nor current process has android.permission.WAKE_LOCK.")
        events += "acquire"
        return AutoCloseable { events += "release" }
    }
}

/** A simulated track that plays against its own [clock], in frames at 48 kHz. */
private interface ClockedTrack : PlaybackOutput {
    var clock: Long

    /** The samples of every packet it received. */
    val received: List<ShortArray>

    /** When it ran dry, in ms: once each time it did. */
    val underrunsAtMs: List<Long>
}

/** Records [size] bytes of little-endian PCM from [pcm] as samples. */
private fun samplesOf(pcm: ByteBuffer, size: Int): ShortArray {
    val bytes = ByteArray(size).also { pcm.get(it) }
    return ShortArray(size / 2).also { ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN).asShortBuffer().get(it) }
}

/**
 * An output that plays in real time at 48 kHz, frame by frame, as an `AudioTrack` played with
 * blocking writes does: it starts once its buffer (the cushion applied) is full, plays while it
 * holds audio, runs dry when it does not and plays again as soon as it is written, and a write that
 * does not fit blocks (moves the clock) until it does.
 */
private class RealTimeTrack : ClockedTrack {
    override val capacityFrames: Long = 24_000L
    override var clock = 0L
    override val received = mutableListOf<ShortArray>()
    override val underrunsAtMs = mutableListOf<Long>()
    private var bufferFrames = 0L
    private var held = 0L
    private var played = 0L
    private var started = false
    private var advancedTo = 0L

    /** Plays what the time since it last looked allows. */
    private fun advance() {
        val elapsed = clock - advancedTo
        advancedTo = clock
        if (!started || elapsed <= 0) return
        if (elapsed <= held) {
            held -= elapsed
            played += elapsed
        } else {
            if (held > 0) underrunsAtMs += (clock - elapsed + held) / 48
            played += held
            held = 0
        }
    }

    override fun read(readings: MutableOutputReadings) {
        advance()
        readings.hasWritableFrames = false
        readings.underrunCount = underrunsAtMs.size.toLong()
        readings.hasPresentedFrame = true
        readings.presentedFrame = played
        readings.isPlaying = true
    }

    override fun write(pcm: ByteBuffer, size: Int): Int {
        advance()
        received += samplesOf(pcm, size)
        held += size / 4
        if (held >= bufferFrames) started = true
        if (started && held > bufferFrames) {
            // Blocks until the frames fit.
            clock += held - bufferFrames
            advance()
        }
        return size
    }

    override fun setTargetFrames(frames: Long): Long {
        bufferFrames = frames
        return frames
    }

    override fun close() = Unit
}

/**
 * An output whose blocking write returns only in coarse steps, as the Galaxy S23's did. It plays
 * frame by frame against [clock], but a write that does not fit sleeps until the next wake-up, every
 * [periodFrames] (the mixer's period), so once the buffer is full the datagrams arriving meanwhile
 * pile up before it, 4 to 8 packets each time. The buffer holds the cushion applied plus a period,
 * and the frame it presents is a period behind what it played (the HAL and the DAC), so the audio it
 * says is still to play is always more than the cushion, as the device's was. It starts once its
 * buffer is full, and runs dry when it has nothing left to play.
 */
private class CoarseTrack(private val periodFrames: Long) : ClockedTrack {
    override val capacityFrames: Long = 24_000L
    override var clock = 0L
    override val received = mutableListOf<ShortArray>()
    override val underrunsAtMs = mutableListOf<Long>()
    private var cushion = 0L
    private var held = 0L
    private var played = 0L
    private var started = false
    private var advancedTo = 0L

    private val bufferFrames: Long get() = cushion + periodFrames

    /** Plays what the time since it last looked allows. */
    private fun advance() {
        val elapsed = clock - advancedTo
        advancedTo = clock
        if (!started || elapsed <= 0) return
        if (elapsed <= held) {
            held -= elapsed
            played += elapsed
        } else {
            if (held > 0) underrunsAtMs += (clock - elapsed + held) / 48
            played += held
            held = 0
        }
    }

    override fun read(readings: MutableOutputReadings) {
        advance()
        readings.hasWritableFrames = false
        readings.underrunCount = underrunsAtMs.size.toLong()
        readings.hasPresentedFrame = true
        readings.presentedFrame = maxOf(played - periodFrames, 0L)
        readings.isPlaying = true
    }

    override fun write(pcm: ByteBuffer, size: Int): Int {
        advance()
        received += samplesOf(pcm, size)
        val frames = size / 4L
        if (!started && held + frames > bufferFrames) started = true
        // Sleeps until a wake-up finds room.
        while (started && held + frames > bufferFrames) {
            clock = (clock / periodFrames + 1) * periodFrames
            advance()
        }
        held += frames
        if (held >= bufferFrames) started = true
        return size
    }

    override fun setTargetFrames(frames: Long): Long {
        cushion = frames
        return frames
    }

    override fun close() = Unit
}

/**
 * An output like the Galaxy S23's FAST track: its FastMixer takes [stepFrames] (4 ms) from the
 * buffer every [stepFrames] of [clock], and the buffer holds [bufferFrames] whatever cushion is
 * asked, so the track itself rides out only about 5 ms of an arrival dip. A blocking write copies
 * what fits and waits for the mixer to make room for the rest. The frame it presents is
 * [lagFrames] (24 ms: the HAL and the DAC) behind what the mixer took, so the audio it says is still
 * to play is always more than the cushion, as the device's was. It starts once its buffer is full;
 * a step that finds less than it takes runs it dry.
 */
private class FastTrack(
    private val bufferFrames: Long = 256,
    private val stepFrames: Long = 192,
    private val lagFrames: Long = 1_152,
) : ClockedTrack {
    override val capacityFrames: Long = 24_000L
    override var clock = 0L
    override val received = mutableListOf<ShortArray>()
    override val underrunsAtMs = mutableListOf<Long>()
    private var held = 0L
    private var taken = 0L
    private var started = false
    private var dry = false
    private var nextStep = stepFrames

    /** The mixer's steps up to now. */
    private fun advance() {
        while (nextStep <= clock) {
            if (started) {
                if (held < stepFrames) {
                    if (!dry) underrunsAtMs += nextStep / 48
                    dry = true
                    taken += held
                    held = 0
                } else {
                    dry = false
                    taken += stepFrames
                    held -= stepFrames
                }
            }
            nextStep += stepFrames
        }
    }

    override fun read(readings: MutableOutputReadings) {
        advance()
        readings.hasWritableFrames = false
        readings.underrunCount = underrunsAtMs.size.toLong()
        readings.hasPresentedFrame = true
        readings.presentedFrame = maxOf(taken - lagFrames, 0L)
        readings.isPlaying = true
    }

    override fun write(pcm: ByteBuffer, size: Int): Int {
        advance()
        received += samplesOf(pcm, size)
        var remaining = size / 4L
        while (true) {
            val copied = minOf(maxOf(bufferFrames - held, 0L), remaining)
            held += copied
            remaining -= copied
            if (held >= bufferFrames) started = true
            if (remaining == 0L) return size
            // Waits for the mixer to make room.
            clock = maxOf(clock, nextStep)
            advance()
        }
    }

    override fun setTargetFrames(frames: Long): Long = frames

    override fun close() = Unit
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

    /** When each report came, by [clockMs]. */
    val reportTimesMs = mutableListOf<Long>()
    var clockMs: () -> Long = { 0L }
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
        reportTimesMs += clockMs()
    }

    override fun onError(message: String) {
        errors += message
    }

    override fun onStopped() = Unit
}
