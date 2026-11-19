package com.bandear.android

import com.bandear.android.core.MutableOutputReadings
import com.bandear.android.core.OutputFormat
import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.ceil
import kotlin.math.sin
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The catch-up of a standing surplus at [DriftTrimmedOutput]'s edges, from the independent check of
 * the change: when a standing queue is caught up and by how much at each packet size, cushion and
 * track fill, the junction against [PcmSplice] and the heard steps, and the frame accounting
 * through short, refused and throwing writes mixed with slow trims and catch-ups.
 */
class CatchUpAccountingTest {

    /**
     * A track with [onTrack] frames still to play (once that many were taken), whose write answers
     * [answer] (null takes all) or throws when [throws]. Records what each write was handed.
     */
    private class CtlTrack : PlaybackOutput {
        override val capacityFrames = 24_000L
        var onTrack = 0L
        var answer: ((Int) -> Int)? = null
        var throws = false
        var taken = 0L
        val handed = mutableListOf<ShortArray>()
        val played = mutableListOf<ShortArray>()

        val presented: Long get() = maxOf(taken - onTrack, 0L)

        override fun read(readings: MutableOutputReadings) {
            readings.hasWritableFrames = false
            readings.underrunCount = 0L
            readings.hasPresentedFrame = true
            readings.presentedFrame = presented
            readings.isPlaying = true
        }

        override fun write(pcm: ByteBuffer, size: Int): Int {
            val bytes = ByteArray(size).also { pcm.get(it) }
            val samples = ShortArray(size / 2).also { ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN).asShortBuffer().get(it) }
            handed += samples
            if (throws) throw IllegalStateException("the track is gone")
            val result = answer?.invoke(size) ?: size
            val takenBytes = result.coerceIn(0, size) / 4 * 4
            taken += takenBytes / 4
            played += samples.copyOf(takenBytes / 2)
            return result
        }

        override fun setTargetFrames(frames: Long) = frames
        override fun close() = Unit
    }

    private fun tone(sequence: Int, frames: Int, noise: Boolean = false): ShortArray = ShortArray(frames * 2) { index ->
        val frame = (sequence - 1).toLong() * frames + index / 2
        val left = 9_000 * sin(2 * PI * 440.0 * frame / 48_000) + 5_000 * sin(2 * PI * 1_234.5 * frame / 48_000 + 0.3)
        val right = 7_000 * sin(2 * PI * 660.0 * frame / 48_000 + 1.1)
        val hiss = if (noise) ((frame * 2 + index % 2) * 2_654_435_761L % 4_001L - 2_000L).toDouble() else 0.0
        ((if (index % 2 == 0) left else right) + hiss).toInt().toShort()
    }

    private fun bytes(samples: ShortArray): ByteBuffer {
        val buffer = ByteBuffer.allocate(samples.size * 2).order(ByteOrder.LITTLE_ENDIAN)
        buffer.asShortBuffer().put(samples)
        return buffer
    }

    private fun maxStep(samples: ShortArray): Int =
        (2 until samples.size).maxOf { abs(samples[it] - samples[it - 2]) }

    /** The packets played in a catch-up window ([CATCH_UP_WINDOW_MS] at 48 kHz) of [frames]-frame packets. */
    private fun windowPackets(frames: Int): Int = ((CATCH_UP_WINDOW_MS * 48 + frames - 1) / frames).toInt()

    @Test
    fun threshold() {
        for (frames in listOf(128, 240)) for (target in listOf(128L, 240L, 1_200L, 1_439L, 1_440L, 1_441L, 2_400L)) {
            for (fill in listOf(0L, target / 2, target)) {
                val allowed = maxOf(target, CATCH_UP_FLOOR_MS * 48)
                // The most packets behind the one playing whose standing surplus is not over the allowance.
                val alone = (0..1_000).last { fill + it.toLong() * frames - target <= allowed }
                val prefill = maxOf(1, ((fill + frames - 1) / frames).toInt())
                // The prefill's plays open the first window, so the first window of standing plays ends at twice it.
                val caughtAtPlay = 2 * windowPackets(frames)

                for (behind in listOf(alone, alone + 1, alone + 2)) {
                    val track = CtlTrack()
                    val output = DriftTrimmedOutput(track, OutputFormat(sampleRate = 48_000L, packetFrames = frames.toLong()))
                    output.setTargetFrames(target)
                    var sequence = 0
                    repeat(prefill) { output.write(bytes(tone(++sequence, frames)), frames * 4); output.playNext() }
                    track.onTrack = fill
                    repeat(behind + 1) { output.write(bytes(tone(++sequence, frames)), frames * 4) }
                    var caughtAt = -1
                    for (play in prefill + 1..caughtAtPlay + windowPackets(frames) / 2) {
                        val before = output.discardedSamples
                        output.playNext()
                        output.write(bytes(tone(++sequence, frames)), frames * 4)
                        if (output.discardedSamples > before) {
                            assertEquals("caught up twice", -1, caughtAt)
                            caughtAt = play
                        }
                    }
                    val label = "$frames-frame packets, cushion $target, fill $fill, $behind behind"
                    // All of a standing surplus over the allowance goes but the cushion (one packet
                    // at least), in whole packets: none when not even one fits.
                    val surplus = fill + behind.toLong() * frames - target
                    val keep = maxOf(frames.toLong(), target)
                    val dropped = if (surplus > allowed) maxOf(0L, (surplus - keep) / frames) else 0L
                    if (dropped == 0L) {
                        assertEquals(label, -1, caughtAt)
                        assertEquals(label, 0L, output.discardedSamples)
                        track.handed.forEachIndexed { index, samples -> assertArrayEquals(label, tone(index + 1, frames), samples) }
                        continue
                    }
                    // Caught up at the window's end, leaving the cushion and less than a packet more.
                    assertEquals(label, caughtAtPlay, caughtAt)
                    assertEquals(label, dropped * frames * 2, output.discardedSamples)
                    assertTrue(label, surplus - dropped * frames in keep until keep + frames)
                    val head = caughtAtPlay
                    assertArrayEquals(label, PcmSplice.removeFrames(tone(head, frames) + tone(head + dropped.toInt(), frames), 2, frames), track.handed[head - 1])
                    for (index in head until track.handed.size) {
                        assertArrayEquals(label, tone(index + 1 + dropped.toInt(), frames), track.handed[index])
                    }
                    val reading = MutableOutputReadings().also { output.read(it) }
                    assertEquals(label, sequence.toLong() * frames, reading.presentedFrame + fill + output.queuedFrames)
                }
            }
        }
    }

    @Test
    fun junction() {
        val frames = 240
        for (noise in listOf(false, true)) for (fill in listOf(0L, 600L, 1_200L)) {
            val track = CtlTrack()
            val output = DriftTrimmedOutput(track, OutputFormat(sampleRate = 48_000L, packetFrames = frames.toLong()))
            output.setTargetFrames(1_200L)
            val prefill = maxOf(1, ((fill + frames - 1) / frames).toInt())
            var sequence = 0
            repeat(prefill) { output.write(bytes(tone(++sequence, frames, noise)), frames * 4); output.playNext() }
            track.onTrack = fill
            // 40 packets stand behind the one playing until the second window ends, at play 200.
            repeat(41) { output.write(bytes(tone(++sequence, frames, noise)), frames * 4) }
            for (play in prefill + 1..2 * windowPackets(frames) + 10) {
                output.playNext()
                output.write(bytes(tone(++sequence, frames, noise)), frames * 4)
            }
            // All of the standing surplus but the 1 200-frame cushion goes.
            val dropped = ((fill + 40L * frames - 1_200L - 1_200L) / frames).toInt()
            val headSequence = 2 * windowPackets(frames)
            val head = tone(headSequence, frames, noise)
            val keptPcm = tone(headSequence + dropped, frames, noise)
            val joined = track.handed[headSequence - 1]
            assertArrayEquals(PcmSplice.removeFrames(head + keptPcm, 2, frames), joined)
            // Where the junction differs from both the head and the packet kept: inside one 64-frame window.
            val differing = (0 until frames).filter { f -> (0..1).any { c -> joined[f * 2 + c] != head[f * 2 + c] } && (0..1).any { c -> joined[f * 2 + c] != keptPcm[f * 2 + c] } }
            val start = (frames - PcmSplice.FADE_FRAMES) / 2
            assertTrue("fade at $differing", differing.all { it in start until start + PcmSplice.FADE_FRAMES })
            assertTrue((0 until start * 2).all { joined[it] == head[it] })
            assertTrue(((start + PcmSplice.FADE_FRAMES) * 2 until frames * 2).all { joined[it] == keptPcm[it] })
            // Steps: the stream as heard against the signal's own steps plus what a 64-frame linear crossfade adds.
            val heard = track.played.reduce(ShortArray::plus)
            val signal = (1..sequence).map { tone(it, frames, noise) }.reduce(ShortArray::plus)
            val natural = maxStep(signal)
            val gap = (start until start + PcmSplice.FADE_FRAMES).maxOf { f -> maxOf(abs(head[f * 2] - keptPcm[f * 2]), abs(head[f * 2 + 1] - keptPcm[f * 2 + 1])) }
            val bound = natural + ceil(gap / (PcmSplice.FADE_FRAMES + 1.0)).toInt() + 2
            assertTrue("step ${maxStep(heard)} over $bound", maxStep(heard) <= bound)
            // Accounting: handed over = taken + removed + queued; removed = discarded (no slow trim here).
            val reading = MutableOutputReadings().also { output.read(it) }
            val removed = reading.presentedFrame - track.presented
            assertEquals(sequence.toLong() * frames, track.taken + removed + output.queuedFrames)
            assertEquals(removed * 2, output.discardedSamples)
            assertEquals(dropped.toLong(), output.packetsWithDiscardedSamples)
        }
    }

    @Test
    fun writeAccounting() {
        val frames = 240
        val cases: List<Pair<String, ((Int) -> Int)?>> = listOf(
            "full" to { size: Int -> size },
            "short200" to { _: Int -> 200 },
            "zero" to { _: Int -> 0 },
            "deadObject-6" to { _: Int -> -6 },
            "invalidOperation-3" to { _: Int -> -3 },
            "error-1" to { _: Int -> -1 },
            "minValue" to { _: Int -> Int.MIN_VALUE },
            "partialFrame201" to { _: Int -> 201 },
            "overSize" to { size: Int -> size + 4 },
            "throws" to null,
        )
        for ((name, answer) in cases) {
            val track = CtlTrack()
            val output = DriftTrimmedOutput(track, OutputFormat(sampleRate = 48_000L, packetFrames = frames.toLong()))
            output.setTargetFrames(1_200L)
            output.write(bytes(tone(1, frames)), frames * 4)
            output.write(bytes(tone(2, frames)), frames * 4)
            if (answer == null) track.throws = true else track.answer = answer
            val threw = try {
                output.playNext()
                false
            } catch (error: IllegalStateException) {
                true
            }
            assertEquals(answer == null, threw)
            track.throws = false
            track.answer = null
            assertTrue(output.playNext())
            assertTrue(!output.playNext())
            val expectedLost = if (answer == null) frames else frames - (answer(frames * 4).coerceIn(0, frames * 4) / 4)
            assertEquals(expectedLost * 2L, output.discardedSamples)
            assertEquals(if (expectedLost > 0) 1L else 0L, output.packetsWithDiscardedSamples)
            val reading = MutableOutputReadings().also { output.read(it) }
            val removed = reading.presentedFrame - track.presented
            assertEquals(expectedLost.toLong(), removed)
            assertEquals(2L * frames, track.taken + removed + output.queuedFrames)
            assertArrayEquals(tone(2, frames), track.handed.last())
        }
    }

    @Test
    fun mixedAccounting() {
        // A full track, two packets waiting (a persistent surplus the slow trim splices), a burst of 30
        // every 500 packets (a surplus that stands until caught up), and short, refused and throwing
        // writes now and then.
        val frames = 240
        val target = 1_200L
        val track = CtlTrack()
        val output = DriftTrimmedOutput(track, OutputFormat(sampleRate = 48_000L, packetFrames = frames.toLong()))
        output.setTargetFrames(target)
        var sequence = 0
        repeat(5) { output.write(bytes(tone(++sequence, frames)), frames * 4); output.playNext() }
        track.onTrack = target
        var throwsSeen = 0
        val catchUpsAtPlay = mutableListOf<Int>()
        val burstsAtPlay = mutableListOf<Int>()
        fun check() {
            val reading = MutableOutputReadings().also { output.read(it) }
            val removed = reading.presentedFrame - track.presented
            assertEquals(sequence.toLong() * frames, track.taken + removed + output.queuedFrames)
            val trimmed = track.handed.sumOf { (frames - it.size / 2).toLong() }
            assertEquals(removed - trimmed, output.discardedSamples / 2)
        }
        // 200 plays after the last burst, so it too has two windows to be caught up in.
        for (i in 1..6_200) {
            var writes = 0
            while (writes < 3 && (writes == 0 || output.queuedFrames < 2L * frames)) {
                val before = output.discardedSamples
                output.write(bytes(tone(++sequence, frames)), frames * 4)
                assertEquals("queueing never gives anything up", before, output.discardedSamples)
                writes++
                check()
            }
            if (i % 500 == 0 && i <= 6_000) {
                burstsAtPlay += i
                repeat(30) {
                    output.write(bytes(tone(++sequence, frames)), frames * 4)
                    check()
                }
            }
            track.answer = when {
                i % 211 == 0 -> { _: Int -> -6 }
                i % 97 == 0 -> { size: Int -> size / 2 / 4 * 4 }
                else -> null
            }
            track.throws = i % 331 == 0
            // What this play gives up beyond what the track did not take is a catch-up.
            val discardedBefore = output.discardedSamples / 2
            val handedBefore = track.handed.sumOf { it.size / 2L }
            val takenBefore = track.taken
            try {
                output.playNext()
            } catch (error: IllegalStateException) {
                throwsSeen++
            }
            val notTaken = (track.handed.sumOf { it.size / 2L } - handedBefore) - (track.taken - takenBefore)
            if (output.discardedSamples / 2 - discardedBefore > notTaken) catchUpsAtPlay += i
            track.answer = null
            track.throws = false
            check()
        }
        while (output.playNext()) check()
        check()
        val splices = track.handed.count { it.size / 2 == frames - 8 }
        assertTrue(splices > 0)
        // Each burst is caught up once, within two windows of it, and the steady traffic never is.
        assertEquals("caught up at $catchUpsAtPlay", burstsAtPlay.size, catchUpsAtPlay.size)
        burstsAtPlay.zip(catchUpsAtPlay).forEach { (burst, caught) ->
            assertTrue("burst at $burst caught up at $caught", caught - burst in 1..2 * windowPackets(frames))
        }
        assertTrue(throwsSeen > 0)
    }
}
