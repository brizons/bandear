package com.bandear.android.core

import com.sun.jna.NativeLibrary
import java.util.Locale
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test

/**
 * S8: what one `onWake` costs through the bindings in steady state, for a stereo 128-frame packet
 * at 48 kHz (2.67 ms of audio), with local protection on, a report every second and the cushion's
 * adjustment every second of audio: the core's `measure_one_wake_up`, plus the crossing. The budget
 * is 5 % of the packet, about 133 µs.
 *
 * A measurement, so it runs only when asked:
 * `BANDEAR_BENCH=1 gradle -p apps/android :app:testDebugUnitTest --tests '*ReceivePipelineBenchmark' --rerun`.
 * The unit tests load the debug build of the core; set `BANDEAR_BENCH_LIBRARY_DIR` to the directory
 * of a release `libbandear_ffi` to measure that one instead (only when this class runs alone, since
 * JNA keeps the first library it loaded). The output names the library measured.
 */
class ReceivePipelineBenchmark {
    @Test
    fun measureOneWakeUpThroughTheBindings() {
        assumeTrue("a measurement: run it with BANDEAR_BENCH=1", System.getenv("BANDEAR_BENCH") == "1")
        System.getenv("BANDEAR_BENCH_LIBRARY_DIR")?.let { NativeLibrary.addSearchPath("bandear_ffi", it) }

        val samples = ShortArray(PmonFixtures.FRAMES * 2) { (it * 97).toShort() }
        val settings = PlayerSettings(
            volumePercent = 80L,
            maxLevelPercent = 100L,
            muted = false,
            serverOwnsMix = false,
            playbackBuffer = PlaybackBufferSetting.Auto,
            redundancyHoldPackets = 0L,
        )
        val output = OutputReadings(writableFrames = 24_000L, underrunCount = 0L, presentedFrame = 0L, isPlaying = true)
        var reports = 0
        var written = 0L
        var elapsedNs = 0L

        ReceivePipeline(0uL).use { pipeline ->
            var applied: Long? = null
            var dueAt = pipeline.reportDueAtMs()

            // One wake-up as the receive loop runs it, from its received bytes on; what the outcome
            // asks of the output is only noted, as a real output would only copy the bytes.
            fun run(sequence: Long, bytes: ByteArray): WakeOutcome {
                val nowNs = sequence * PACKET_NS
                var outcome = pipeline.onWake(
                    Wake(
                        startedNs = nowNs.toULong(),
                        outputAtStart = if (nowNs / 1_000_000 >= dueAt) output else null,
                        appliedTargetFrames = applied,
                        datagram = bytes,
                        receivedNs = nowNs.toULong(),
                        outputAtReceive = output,
                        settings = settings,
                    ),
                )
                applied = null
                if (outcome.openOutput != null) {
                    outcome = pipeline.onOutputOpened(nowNs.toULong(), 24_000L, output)
                }
                for (action in outcome.actions) {
                    if (action is OutputAction.SetTargetFrames) applied = action.frames
                }
                dueAt = outcome.reportDueAtMs
                return outcome
            }

            for (sequence in 0L until WARM_UP) {
                run(sequence, PmonFixtures.datagram(sequence.toULong(), samples))
            }
            for (sequence in WARM_UP until WARM_UP + MEASURED) {
                // The bytes are built outside the timing: the platform's socket delivers them.
                val bytes = PmonFixtures.datagram(sequence.toULong(), samples)
                val started = System.nanoTime()
                val outcome = run(sequence, bytes)
                elapsedNs += System.nanoTime() - started
                if (outcome.report != null) reports++
                for (action in outcome.actions) {
                    if (action is OutputAction.Write) written += action.pcm16LeBytes.size
                }
            }
        }

        val perCallNs = elapsedNs.toDouble() / MEASURED
        val budgetNs = PACKET_NS * 0.05
        val library = NativeLibrary.getInstance("bandear_ffi").file
        val share = "%.3f".format(Locale.ROOT, perCallNs / PACKET_NS * 100)
        println(
            "S8 (Kotlin bindings, JVM ${System.getProperty("java.version")}, $library): " +
                "${perCallNs.toLong()} ns per wake-up over $MEASURED wake-ups " +
                "($reports reports, $written bytes written), " +
                "$share % of a 2.67 ms packet; budget ${budgetNs.toLong()} ns (5 %)",
        )
        assertEquals(MEASURED * PmonFixtures.FRAMES * 4, written)
        assertTrue("$perCallNs ns per wake-up is over the budget", perCallNs < budgetNs)
    }

    private companion object {
        const val PACKET_NS = 2_666_667L
        const val WARM_UP = 20_000L
        const val MEASURED = 200_000L
    }
}
