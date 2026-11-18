package com.bandear.android

import com.bandear.android.core.OutputFormat
import com.bandear.android.core.OutputReadings
import java.nio.ByteBuffer
import java.nio.ByteOrder

/** A frame of the output: 16-bit stereo. */
private const val BYTES_PER_FRAME = 2 * PlaybackLatency.CHANNELS

/**
 * The most audio kept waiting before the track, in frames (half a second at 48 kHz). Only a track
 * that stalls for that long reaches it, and the oldest audio is then given up on.
 */
internal const val MAX_QUEUED_FRAMES = 24_000L

/**
 * The output a [ReceiveSession] plays through, so that a desk clock that runs faster than the
 * phone's DAC cannot make the heard latency grow without bound.
 *
 * What the shared core hands over to write waits in a [PlayoutQueue] before the [track] instead of
 * going to it at once. The session drains every datagram the socket already holds through the core
 * first, so the audio a blocked write would otherwise leave unseen in the kernel's socket buffer
 * waits here, where it is counted. Each packet is then written ([playNext]) only after the
 * [DriftTrimmer] judged the whole queue it waits in: the audio still to play on the track (never
 * more than the buffer the track is sized to: what it reports beyond that is a stale position) plus
 * the packets waiting before it, against that buffer. When that queue stays above the buffer, the
 * surplus is spliced out of the packet's PCM ([PcmSplice]) a few frames at a time.
 *
 * The core counts every frame it hands over as written, and measures the backlog it reports (to the
 * desk, and on screen) as those frames past the one the track presents. So the queue counts in it
 * by itself, and the frames handed over that never reach the track (trimmed, given up on at the
 * queue's bound, or lost to a failed write) are added to the presented frame [readings] answers:
 * the frame reaching the listener, counted in the core's frames. The backlog is then everything not
 * heard yet, on the track and before it. Frames given up on at the bound count as discarded
 * ([discardedSamples]); trimmed frames do not.
 *
 * Everything else goes to the track at once: its readings, the cushion, closing it. Single-threaded,
 * on the receive thread.
 */
internal class DriftTrimmedOutput(
    private val track: PlaybackOutput,
    format: OutputFormat,
) : PlaybackOutput {
    private val packetFrames = format.packetFrames.toInt()
    private val trimmer = DriftTrimmer(format.sampleRate.toInt())
    private val queue = PlayoutQueue<ByteArray>(MAX_QUEUED_FRAMES)

    /** The frames written to the track. */
    private var framesWritten = 0L

    /** The frames handed over that never reached the track. */
    private var removedFrames = 0L

    /** The cushion the core asked for last, and what the track answered (0 when it cannot say). */
    private var askedTargetFrames = 0L
    private var appliedTargetFrames = 0L

    /** The samples given up on at the queue's bound, and the packets whose queueing did that. */
    var discardedSamples = 0L
        private set
    var packetsWithDiscardedSamples = 0L
        private set

    override val capacityFrames: Long get() = track.capacityFrames

    /** The track's readings, its presented frame counted in the frames the core handed over. */
    override fun readings(): OutputReadings {
        val readings = track.readings()
        val presented = readings.presentedFrame ?: return readings
        return readings.copy(presentedFrame = presented + removedFrames)
    }

    /** Queues the bytes; nothing reaches the track until [playNext]. */
    override fun write(pcm16LeBytes: ByteArray) {
        val dropped = queue.add(pcm16LeBytes, pcm16LeBytes.size / BYTES_PER_FRAME)
        if (dropped > 0) {
            removedFrames += dropped
            discardedSamples += dropped * PlaybackLatency.CHANNELS
            packetsWithDiscardedSamples++
        }
    }

    override fun setTargetFrames(frames: Long): Long =
        track.setTargetFrames(frames).also { applied ->
            askedTargetFrames = frames
            appliedTargetFrames = applied
        }

    /**
     * Writes the oldest packet waiting to the track, trimmed when a surplus has persisted; false
     * when none waits. A failed write is thrown, and the packet is not written again.
     */
    fun playNext(): Boolean {
        var bytes = queue.poll() ?: return false
        val presented = track.readings().presentedFrame
        val targetFrames = if (appliedTargetFrames > 0) appliedTargetFrames else askedTargetFrames
        if (presented != null && targetFrames > 0) {
            val onTrack = minOf(maxOf(framesWritten - presented, 0L), targetFrames)
            val cut = trimmer.onPacket(packetFrames, onTrack + queue.queuedFrames, targetFrames)
            if (cut > 0) bytes = splice(bytes, cut)
        }
        val frames = bytes.size / BYTES_PER_FRAME
        try {
            track.write(bytes)
        } catch (error: Exception) {
            removedFrames += frames
            throw error
        }
        framesWritten += frames
        return true
    }

    override fun close() = track.close()

    /** [bytes] without [cut] frames ([PcmSplice]), counting what went as never reaching the track. */
    private fun splice(bytes: ByteArray, cut: Int): ByteArray {
        val samples = ShortArray(bytes.size / 2)
        ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN).asShortBuffer().get(samples)
        val spliced = PcmSplice.removeFrames(samples, PlaybackLatency.CHANNELS, cut)
        removedFrames += (samples.size - spliced.size) / PlaybackLatency.CHANNELS
        val out = ByteArray(spliced.size * 2)
        ByteBuffer.wrap(out).order(ByteOrder.LITTLE_ENDIAN).asShortBuffer().put(spliced)
        return out
    }
}
