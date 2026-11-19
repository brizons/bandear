package com.bandear.android.core

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Drives the shared receive pipeline through the generated bindings the way the receive loop will:
 * a packet asks for the output, the output opens, later packets play through it, and a timeout
 * carries the link report. The pipeline's rules are tested in Rust
 * (`crates/bandear-core/src/receive_pipeline.rs`); these tests prove each call and type crosses
 * the binding and reads in Kotlin as the loop expects.
 */
class ReceivePipelineBindingTest {
    @Test
    fun aPacketOpensTheOutputThenPlaysThroughIt() {
        ReceivePipeline(0uL).use { pipeline ->
            assertEquals(3_000L, pipeline.reportDueAtMs())

            val first = pipeline.onWake(packet(1uL, seed = 0, atMs = 0))
            assertEquals(
                OutputFormat(sampleRate = 48_000L, packetFrames = PmonFixtures.FRAMES.toLong()),
                first.openOutput,
            )
            assertTrue(first.actions.isEmpty())

            val opened = pipeline.onOutputOpened(0uL, 24_000L, PmonFixtures.READINGS)
            assertEquals(2, opened.actions.size)
            assertEquals(OutputAction.SetTargetFrames(2L * PmonFixtures.FRAMES), opened.actions[0])
            assertArrayEquals(PmonFixtures.littleEndian(PmonFixtures.samples(0)), writtenBytes(opened.actions[1]))
            assertNull(opened.openOutput)
            assertEquals(1_000L, opened.reportDueAtMs)

            val second = pipeline.onWake(packet(2uL, seed = 1, atMs = 10))
            assertEquals(1, second.actions.size)
            val bytes = writtenBytes(second.actions[0])
            // The byte order, read by hand: 0x0102, -2, Short.MIN_VALUE, Short.MAX_VALUE.
            assertArrayEquals(
                byteArrayOf(0x02, 0x01, 0xFE.toByte(), 0xFF.toByte(), 0x00, 0x80.toByte(), 0xFF.toByte(), 0x7F),
                bytes.copyOf(8),
            )
            assertArrayEquals(PmonFixtures.littleEndian(PmonFixtures.samples(1)), bytes)
            assertEquals(listOf(2L), second.stats.map { it.packets })
            assertNull(second.error)

            // Nothing came for a while, and the report fell due.
            val timeout = pipeline.onWake(PmonFixtures.wake(null, atMs = 1_000, atStart = PmonFixtures.READINGS))
            assertTrue(timeout.actions.isEmpty())
            assertTrue(timeout.stats.isEmpty())
            assertNull(timeout.error)
            assertNull(timeout.openOutput)
            val report = timeout.report!!
            assertEquals(2L, report.packetsReceived)
            assertEquals(0L, report.packetsLost)
            assertEquals(2_000L, timeout.reportDueAtMs)
        }
    }

    @Test
    fun anUndecodableDatagramIsAReceiveError() {
        ReceivePipeline(0uL).use { pipeline ->
            val notPmon = PmonFixtures.datagram(1uL, PmonFixtures.samples(0))
            notPmon[3] = 'X'.code.toByte()
            val error = pipeline.onWake(PmonFixtures.wake(notPmon, atMs = 0)).error
            val reason = (error as ReceiveError.Undecodable).reason
            assertArrayEquals("PMOX".toByteArray(), (reason as DecodeError.UnknownMagic).magic)

            val short = ByteArray(5) { 'P'.code.toByte() }
            assertEquals(
                ReceiveError.Undecodable(DecodeError.HeaderTooShort(5uL)),
                pipeline.onWake(PmonFixtures.wake(short, atMs = 10)).error,
            )
        }
    }

    /** The track's capacity and the channel limit, as the app computed them before the core did. */
    @Test
    fun thePlayersRoomAndTheChannelLimitAreTheCores() {
        // 500 ms at 48 kHz is 24 000 frames: 93.75 packets of 256, so 94 of them.
        assertEquals(94L * 256, playbackCapacityFrames(48_000L, 256L))
        assertEquals(24_000L, playbackCapacityFrames(48_000L, 240L))
        assertEquals(22_050L, playbackCapacityFrames(44_100L, 0L))
        assertEquals(32L, maximumMixChannels())
    }

    /** A wake-up at [atMs] that received packet [sequence] with [PmonFixtures.samples] of [seed]. */
    private fun packet(sequence: ULong, seed: Int, atMs: Long): Wake =
        PmonFixtures.wake(PmonFixtures.datagram(sequence, PmonFixtures.samples(seed)), atMs)

    private fun writtenBytes(action: OutputAction): ByteArray = (action as OutputAction.Write).pcm16LeBytes
}

/**
 * PMON datagrams built by hand from the layout in `crates/bandear-core/src/packet.rs`, and the
 * wake-ups that carry them.
 */
internal object PmonFixtures {
    /** Stereo frames per packet: the S8 packet, 128 frames at 48 kHz. */
    const val FRAMES = 128

    /** An output that takes everything, has presented nothing, never ran dry, and plays. */
    val READINGS = OutputReadings(writableFrames = null, underrunCount = 0L, presentedFrame = null, isPlaying = true)

    /** The packet plays as sent: the desk owns the mix. */
    private val SETTINGS = PlayerSettings(
        volumePercent = 100L,
        maxLevelPercent = 100L,
        muted = false,
        serverOwnsMix = true,
        playbackBuffer = PlaybackBufferSetting.Auto,
        redundancyHoldPackets = 0L,
    )

    /**
     * One datagram as the desk sends it: `PMON`, version 1, 2 channels, 48 000 Hz, the sequence,
     * the sample count, then the samples, every number little endian.
     */
    fun datagram(sequence: ULong, samples: ShortArray): ByteArray =
        java.nio.ByteBuffer.allocate(20 + samples.size * 2)
            .order(java.nio.ByteOrder.LITTLE_ENDIAN)
            .put("PMON".toByteArray())
            .put(1)
            .put(2)
            .putInt(48_000)
            .putLong(sequence.toLong())
            .putShort(samples.size.toShort())
            .put(littleEndian(samples))
            .array()

    /**
     * A packet's interleaved samples: both extremes, a sign and a byte order to read, then a ramp
     * that differs per `seed`.
     */
    fun samples(seed: Int): ShortArray =
        shortArrayOf(0x0102, -2, Short.MIN_VALUE, Short.MAX_VALUE) +
            ShortArray(FRAMES * 2 - 4) { ((it + 4) * 97 + seed).toShort() }

    fun littleEndian(samples: ShortArray): ByteArray {
        val bytes = java.nio.ByteBuffer.allocate(samples.size * 2).order(java.nio.ByteOrder.LITTLE_ENDIAN)
        samples.forEach { bytes.putShort(it) }
        return bytes.array()
    }

    /** A wake-up at [atMs], with fresh readings after the receive. */
    fun wake(datagram: ByteArray?, atMs: Long, atStart: OutputReadings? = null): Wake =
        Wake(
            startedNs = (atMs * 1_000_000).toULong(),
            outputAtStart = atStart,
            appliedTargetFrames = null,
            datagram = datagram,
            receivedNs = (atMs * 1_000_000).toULong(),
            outputAtReceive = READINGS,
            settings = SETTINGS,
        )
}
