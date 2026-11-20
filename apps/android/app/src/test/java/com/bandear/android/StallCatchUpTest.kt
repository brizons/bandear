package com.bandear.android

import com.bandear.android.core.MutableOutputReadings
import com.bandear.android.core.OutputFormat
import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.sin
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The catch-up of a surplus that stands ([DriftTrimmedOutput]): the smallest surplus over each
 * window of [CATCH_UP_WINDOW_MS] of audio played is the backlog that never drained, and when it is
 * over the cushion and over [CATCH_UP_FLOOR_MS], the oldest whole packets go at once, all of it but
 * the cushion (one packet at least), with a crossfade at the junction ([PcmSplice]). A surplus that comes back down within
 * the window never counts; a smaller one is left to the slow trim.
 *
 * Here the track has played everything written to it, so the surplus is what waits before it over
 * the cushion. A packet is written for each one played, so a queue stands as long as the test wants.
 */
class StallCatchUpTest {
    /** 240-frame packets, 5 ms each at 48 kHz: a window is 100 of them. */
    private val format = OutputFormat(sampleRate = 48_000L, packetFrames = FRAMES.toLong())

    /** The packets written so far, numbered from 1. */
    private var written = 0

    /** Writes the next packet ([samples] of its number). */
    private fun DriftTrimmedOutput.writeNext(samples: (Int) -> ShortArray = ::ramp) {
        written++
        write(bytes(samples(written)), FRAMES * 4)
    }

    /**
     * Plays [plays] packets, writing one for each, so the [behind] packets queued behind the one
     * playing stand; answers the discarded samples after each play.
     */
    private fun DriftTrimmedOutput.stand(behind: Int, plays: Int, samples: (Int) -> ShortArray = ::ramp): List<Long> {
        repeat(behind + 1 - (queuedFrames / FRAMES).toInt()) { writeNext(samples) }
        return (1..plays).map {
            playNext()
            writeNext(samples)
            discardedSamples
        }
    }

    @Test
    fun aSurplusThatStandsThroughAWindowIsCaughtUpAtItsEnd() {
        // A 25 ms cushion with 40 packets behind the one playing: 8 400 frames over it.
        val track = DryTrack()
        val output = DriftTrimmedOutput(track, format)
        output.setTargetFrames(1_200L)
        val discarded = output.stand(behind = 40, plays = 150)

        // Nothing goes before the window's end, with the 100th packet played...
        assertTrue(discarded.take(99).all { it == 0L })
        // ...and then all of it but the cushion at once: 30 packets, the 100th and the 29 after it.
        assertEquals(30L * FRAMES * 2, discarded[99])
        assertEquals(30L, output.packetsWithDiscardedSamples)
        assertTrue("caught up again", discarded.drop(99).all { it == discarded[99] })
        assertArrayEquals(PcmSplice.removeFrames(ramp(100) + ramp(130), 2, FRAMES), track.received[99])
        assertArrayEquals(ramp(131), track.received[100])
        // The cushion stands over it now: 10 packets behind the one playing.
        assertEquals(10L * FRAMES + FRAMES, output.queuedFrames)
    }

    @Test
    fun aStandingSurplusIsCaughtUpOnlyOverTheCushionAndTheFloor() {
        // The surplus with q packets behind the one playing is q packets less the cushion. It must
        // be over the cushion and over the floor, 30 ms (1 440 frames): the floor decides for the
        // cushions up to 25 ms, the cushion for a 50 ms one. What goes leaves the cushion standing,
        // or one packet when the cushion is smaller.
        val cases = listOf(
            // cushion, most packets behind left alone, packets caught up with one more
            Triple(128L, 6, 5),
            Triple(240L, 7, 6),
            Triple(1_200L, 11, 2),
            Triple(2_400L, 20, 1),
        )
        for ((cushion, alone, caught) in cases) {
            for (behind in listOf(alone, alone + 1)) {
                written = 0
                val track = DryTrack()
                val output = DriftTrimmedOutput(track, format)
                output.setTargetFrames(cushion)
                val discarded = output.stand(behind, plays = 250)
                val expected = if (behind == alone) 0L else caught.toLong() * FRAMES * 2
                assertEquals("cushion $cushion, $behind behind", expected, discarded.last())
                assertEquals("cushion $cushion, $behind behind, before the window's end", 0L, discarded[98])
            }
        }
    }

    @Test
    fun aCatchUpLeavesTheDepthOfBunchesThatRecurStandingOverTheCushion() {
        // A second of 20 packets at once every 20 played, as a Wi-Fi saving power delivers them:
        // each gap before a bunch drains 19 packets, which a cut must leave standing.
        val track = DryTrack()
        val output = DriftTrimmedOutput(track, format)
        output.setTargetFrames(1_200L)
        repeat(200) { played ->
            if (played % 20 == 0) repeat(20) { output.writeNext() }
            output.playNext()
        }
        assertEquals(0L, output.discardedSamples)

        // Then 40 packets stand behind the one playing, as after a stall: 8 400 frames over the
        // 25 ms cushion. At the window's end all of it goes but the cushion and the 19 packets of
        // the bunches' depth: 11 packets, where it would be 30 without the bunches.
        val discarded = output.stand(behind = 40, plays = 150)
        assertTrue(discarded.take(99).all { it == 0L })
        assertEquals(11L * FRAMES * 2, discarded[99])
        assertTrue("caught up again", discarded.drop(99).all { it == discarded[99] })
        assertEquals(29L * FRAMES + FRAMES, output.queuedFrames)
    }

    @Test
    fun aSurplusWithinTheDepthOfBunchesThatRecurIsNeitherCaughtUpNorTrimmed() {
        // The same bunches, then 24 packets behind the one playing for 20 s: 4 560 frames over the
        // 25 ms cushion, exactly the bunches' depth. Without the bunches it would be cut at once
        // and then trimmed; with them it is what the next gap needs, so it all plays, whole.
        val track = DryTrack()
        val output = DriftTrimmedOutput(track, format)
        output.setTargetFrames(1_200L)
        repeat(200) { played ->
            if (played % 20 == 0) repeat(20) { output.writeNext() }
            output.playNext()
        }
        val discarded = output.stand(behind = 24, plays = 4_000)

        assertTrue(discarded.all { it == 0L })
        assertEquals(written - 25, track.received.size)
        track.received.forEachIndexed { index, samples -> assertArrayEquals(ramp(index + 1), samples) }
    }

    @Test
    fun aSurplusThatComesBackDownWithinTheWindowIsNeverCaughtUp() {
        // 16 packets at once every 16 played, as a track whose writes return in coarse steps: the
        // queue climbs to 3 600 frames over the 1 200 of the cushion every 80 ms, and drains.
        val track = DryTrack()
        val output = DriftTrimmedOutput(track, format)
        output.setTargetFrames(1_200L)
        repeat(2_000) { played ->
            if (played % 16 == 0) repeat(16) { output.writeNext() }
            output.playNext()
        }

        assertEquals(0L, output.discardedSamples)
        assertEquals(written, track.received.size)
        track.received.forEachIndexed { index, samples -> assertArrayEquals(ramp(index + 1), samples) }
    }

    @Test
    fun aLongStallIsCaughtUpWithinAWindowOnceTheBoundTookItsShare() {
        // A 700 ms stall's 141 datagrams at once: the bound keeps the newest 24 000 frames.
        val track = DryTrack()
        val output = DriftTrimmedOutput(track, format)
        output.setTargetFrames(1_200L)
        repeat(141) { output.writeNext() }
        assertEquals(MAX_QUEUED_FRAMES, output.queuedFrames)
        assertEquals(41L * FRAMES * 2, output.discardedSamples)

        // Then the 99 behind the one playing stand: 22 560 frames over the cushion, all of it but
        // the cushion caught up at the window's end, 89 packets.
        val discarded = output.stand(behind = 99, plays = 120)
        assertTrue(discarded.take(99).all { it == 41L * FRAMES * 2 })
        assertEquals((41L + 89) * FRAMES * 2, discarded.last())
        assertArrayEquals(PcmSplice.removeFrames(ramp(141) + ramp(230), 2, FRAMES), track.received[99])
    }

    @Test
    fun theJunctionCrossfadesTheTracksAudioIntoTheFirstPacketKept() {
        // A tone whose phase runs on across packets: 30 packets of 1 150 Hz are 172.5 periods, so
        // dropping them turns the tone over at a peak and a hard cut would click.
        val tone = { sequence: Int ->
            ShortArray(FRAMES * 2) { index ->
                val frame = (sequence - 1) * FRAMES + index / 2
                (10_000 * sin(2 * PI * 1_150 * frame / 48_000)).toInt().toShort()
            }
        }
        val track = DryTrack()
        val output = DriftTrimmedOutput(track, format)
        output.setTargetFrames(1_200L)
        output.stand(behind = 40, plays = 120, samples = tone)

        // The 100th packet fades into the 130th, whose tail leads into the 131st, so both edges
        // continue the audio around them.
        assertEquals(30L * FRAMES * 2, output.discardedSamples)
        assertArrayEquals(PcmSplice.removeFrames(tone(100) + tone(130), 2, FRAMES), track.received[99])
        fun biggestStep(samples: ShortArray): Int =
            (1 until samples.size / 2).maxOf { abs(samples[it * 2] - samples[(it - 1) * 2]) }
        val played = track.received.reduce(ShortArray::plus)
        val steady = biggestStep(tone(1) + tone(2))
        assertTrue("a hard cut would click", biggestStep(tone(99) + tone(130)) > steady * 3)
        assertTrue("step ${biggestStep(played)} against $steady in the tone", biggestStep(played) <= steady * 3 / 2)
    }

    /** Packet [sequence]'s samples: a ramp from it. */
    private fun ramp(sequence: Int): ShortArray =
        ShortArray(FRAMES * 2) { ((it * 31 + sequence * 7) % 20_000 - 10_000).toShort() }

    private fun bytes(samples: ShortArray): ByteBuffer {
        val buffer = ByteBuffer.allocate(samples.size * 2).order(ByteOrder.LITTLE_ENDIAN)
        buffer.asShortBuffer().put(samples)
        return buffer
    }

    private companion object {
        const val FRAMES = 240
    }
}

/**
 * A track that has played everything written to it, applies the cushion asked, and records each
 * packet it received.
 */
private class DryTrack : PlaybackOutput {
    override val capacityFrames: Long = 24_000L
    val received = mutableListOf<ShortArray>()
    private var frames = 0L

    override fun read(readings: MutableOutputReadings) {
        readings.hasWritableFrames = false
        readings.underrunCount = 1L
        readings.hasPresentedFrame = true
        readings.presentedFrame = frames
        readings.isPlaying = true
    }

    override fun write(pcm: ByteBuffer, size: Int): Int {
        val bytes = ByteArray(size).also { pcm.get(it) }
        received += ShortArray(size / 2).also { ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN).asShortBuffer().get(it) }
        frames += size / 4
        return size
    }

    override fun setTargetFrames(frames: Long): Long = frames

    override fun close() = Unit
}
