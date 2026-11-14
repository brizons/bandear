package com.bandear.android

import com.bandear.android.core.MutableOutputReadings
import com.bandear.android.core.OutputFormat
import java.nio.Buffer
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.ArrayDeque

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
 * queue's bound, or lost to a failed write) are added to the presented frame [read] answers: the
 * frame reaching the listener, counted in the core's frames. The backlog is then everything not
 * heard yet, on the track and before it. Frames given up on at the bound count as discarded
 * ([discardedSamples]); trimmed frames do not.
 *
 * The samples are copied out of the core's outcome buffer, which the next call reuses, into direct
 * buffers kept for reuse, so a packet written costs no buffer of its own; a splice, at most about
 * once a second, allocates its arrays. Everything else goes to the track at once: its readings, the
 * cushion, closing it. Single-threaded, on the receive thread.
 */
internal class DriftTrimmedOutput(
    private val track: PlaybackOutput,
    format: OutputFormat,
) : PlaybackOutput {
    private val packetFrames = format.packetFrames.toInt()
    private val trimmer = DriftTrimmer(format.sampleRate.toInt())

    /** Each packet waiting, at position 0 with its bytes up to its limit. */
    private val queue = PlayoutQueue<ByteBuffer>(MAX_QUEUED_FRAMES)

    /** Buffers written out, kept to queue the next packets in. */
    private val spare = ArrayDeque<ByteBuffer>()

    /** The track read for a packet's measure. */
    private val measured = MutableOutputReadings()

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
    override fun read(readings: MutableOutputReadings) {
        track.read(readings)
        if (readings.hasPresentedFrame) readings.presentedFrame += removedFrames
    }

    /** Queues a copy of the [size] bytes at [pcm]'s position; nothing reaches the track until [playNext]. */
    override fun write(pcm: ByteBuffer, size: Int) {
        val copy = bufferFor(size)
        // The source's limit goes back to where it was, as the core's reader left it.
        val source: Buffer = pcm
        val limit = source.limit()
        source.limit(source.position() + size)
        copy.put(pcm)
        source.limit(limit)
        val view: Buffer = copy
        view.flip()
        val dropped = queue.add(copy, size / BYTES_PER_FRAME)
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
        val packet = queue.poll() ?: return false
        var written = false
        try {
            track.read(measured)
            val targetFrames = if (appliedTargetFrames > 0) appliedTargetFrames else askedTargetFrames
            if (measured.hasPresentedFrame && targetFrames > 0) {
                val onTrack = minOf(maxOf(framesWritten - measured.presentedFrame, 0L), targetFrames)
                val cut = trimmer.onPacket(packetFrames, onTrack + queue.queuedFrames, targetFrames)
                if (cut > 0) splice(packet, cut)
            }
            val frames = packet.limit() / BYTES_PER_FRAME
            track.write(packet, packet.limit())
            framesWritten += frames
            written = true
        } finally {
            // A packet that did not reach the track is never written again.
            if (!written) removedFrames += packet.limit() / BYTES_PER_FRAME
            spare.addLast(packet)
        }
        return true
    }

    override fun close() = track.close()

    /** An empty buffer of at least [size] bytes: a spare one when it is large enough. */
    private fun bufferFor(size: Int): ByteBuffer {
        val reused = spare.pollFirst()?.takeIf { it.capacity() >= size }
        val buffer = reused ?: ByteBuffer.allocateDirect(size).order(ByteOrder.LITTLE_ENDIAN)
        val view: Buffer = buffer
        view.clear()
        return buffer
    }

    /** [packet] without [cut] frames ([PcmSplice]), in place, counting what went as never reaching the track. */
    private fun splice(packet: ByteBuffer, cut: Int) {
        val samples = ShortArray(packet.limit() / 2)
        packet.asShortBuffer().get(samples)
        val spliced = PcmSplice.removeFrames(samples, PlaybackLatency.CHANNELS, cut)
        removedFrames += (samples.size - spliced.size) / PlaybackLatency.CHANNELS
        packet.asShortBuffer().put(spliced)
        val view: Buffer = packet
        view.limit(spliced.size * 2)
    }
}
