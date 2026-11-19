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
 * The most audio kept waiting before the track, in frames (half a second at 48 kHz). Only a stall
 * about that long reaches it, before its surplus could stand through a catch-up window
 * ([CATCH_UP_WINDOW_MS]), and the oldest audio is then given up on.
 */
internal const val MAX_QUEUED_FRAMES = 24_000L

/**
 * The least standing surplus over the cushion that is caught up at once, in milliseconds of audio.
 *
 * A Wi-Fi stall holds the desk's datagrams back and then delivers them together, and the audio
 * piled up while the track opens does the same. All of it plays, so without a catch-up it would stay
 * as latency, given back by the slow trim ([DriftTrimmer]) at half a millisecond a second at most:
 * minutes for a 200 ms stall. So a surplus that stood through a whole catch-up window
 * ([CATCH_UP_WINDOW_MS]) above the cushion, and above this, is cut at once ([CatchUpWindow]). The
 * floor keeps a small cushion (one packet of an Auto cushion is 2.7 ms) from cutting audio for a
 * surplus the slow trim gives back within a minute; it is also about what main played of a stall's
 * burst (six 5 ms datagrams) before it lost the rest.
 */
internal const val CATCH_UP_FLOOR_MS = 30L

/**
 * How long a surplus must stand before it is caught up, in milliseconds of audio played.
 *
 * Long enough for what comes back down by itself to do so: a track whose blocking write returns
 * only every 20 to 40 ms (its mixer's period: a Galaxy S23 piled 4 to 8 packets before it each
 * time), arrival jitter, a reorder. Short enough that a stall's delay is gone about a second after
 * it.
 */
internal const val CATCH_UP_WINDOW_MS = 500L

/**
 * When to catch up at once, and how much: the surplus that stood through a whole window.
 *
 * Fed once per packet played with the surplus queued, as [DriftTrimmer] measures it (the audio on
 * the track and behind the packet, over the cushion), it keeps the smallest surplus over each window
 * of [CATCH_UP_WINDOW_MS] of audio played. That minimum is the backlog that never drained: jitter,
 * coarse write steps and reorders come back down within the window, so they never count. At the end
 * of each window, a minimum over the cushion and over [CATCH_UP_FLOOR_MS] answers the frames that
 * may go at once: the minimum less the cushion, or less one packet when the cushion is smaller, so
 * at the window's tightest moment the cushion would still have waited. That is the margin the
 * cushion is sized for: on a Galaxy S23, leaving one packet let the next arrival dip rarer than the
 * window run the track dry; and as Auto grows the cushion after underruns, a cut leaves more. Below
 * that the slow trim acts alone.
 *
 * Time is the audio played, not a clock, so it is deterministic. Single-threaded, on the receive
 * thread; allocates nothing.
 */
internal class CatchUpWindow(sampleRate: Long) {
    private val rate = if (sampleRate > 0) sampleRate else 48_000L
    private val windowFrames = CATCH_UP_WINDOW_MS * rate / 1_000
    private val floorFrames = CATCH_UP_FLOOR_MS * rate / 1_000
    private var framesInWindow = 0L
    private var windowMinimum = Long.MAX_VALUE

    /**
     * The most frames to give up on at once before the packet of [packetFrames] frames about to
     * play, given the [surplusFrames] queued over the [targetFrames] the track is sized to with it;
     * 0 but at a window's end with a surplus that stood. What it leaves is the cushion, or one
     * packet when the cushion is smaller.
     */
    fun onPacket(packetFrames: Int, surplusFrames: Long, targetFrames: Long): Long {
        if (packetFrames <= 0) return 0L
        windowMinimum = minOf(windowMinimum, surplusFrames)
        framesInWindow += packetFrames
        if (framesInWindow < windowFrames) return 0L
        val standing = windowMinimum
        framesInWindow = 0L
        windowMinimum = Long.MAX_VALUE
        return if (standing > maxOf(targetFrames, floorFrames)) standing - maxOf(packetFrames.toLong(), targetFrames) else 0L
    }
}

/**
 * The output a [ReceiveSession] plays through, so that a desk clock that runs faster than the
 * phone's DAC, or a Wi-Fi stall, cannot leave the heard latency high.
 *
 * What the shared core hands over to write waits in a [PlayoutQueue] before the [track] instead of
 * going to it at once. The session drains every datagram the socket already holds through the core
 * first, so the audio a blocked write would otherwise leave unseen in the kernel's socket buffer
 * waits here, where it is counted. The queue is measured as [DriftTrimmer] measures it: the audio
 * still to play on the track (never more than the buffer the track is sized to: what it reports
 * beyond that is a stale position) plus the packets waiting behind the next one to play, against
 * that buffer, the cushion.
 *
 * Each packet is written ([playNext]) only once that queue was measured:
 *
 * - A surplus that stood ([CatchUpWindow]), after a stall or since the track opened: when the
 *   smallest surplus over a [CATCH_UP_WINDOW_MS] window is over the cushion and over
 *   [CATCH_UP_FLOOR_MS], the oldest whole packets, the one about to play first, go at once, all of
 *   that surplus but the cushion, or one packet if the cushion is smaller ([catchUp]). The audio on the track goes on into the first frames
 *   of the packet about to play, which fade into the first packet kept ([join], with [PcmSplice]'s
 *   crossfade), so there is no click.
 * - A drift: when the queue stays above the cushion, the surplus is spliced out of the packet about
 *   to play a few frames at a time ([DriftTrimmer], [PcmSplice]).
 *
 * The core counts every frame it hands over as written, and measures the backlog it reports (to the
 * desk, and on screen) as those frames past the one the track presents. So the queue counts in it
 * by itself, and the frames handed over that never reach the track are added to the presented frame
 * [read] answers: the frame reaching the listener, counted in the core's frames. The backlog is then
 * everything not heard yet, on the track and before it. Those frames count as discarded
 * ([discardedSamples]) too, except the ones the slow trim splices out: caught up once a surplus
 * stood, given up on at the queue's bound, or not taken by the track (a short write or an error code, as
 * [PlaybackOutput.write] answers, or a write that throws).
 *
 * The samples are copied out of the core's outcome buffer, which the next call reuses, into direct
 * buffers kept for reuse, so a packet written costs no buffer of its own; a splice or a junction,
 * rare, allocates its arrays. Everything else goes to the track at once: its readings, the cushion,
 * closing it. Single-threaded, on the receive thread.
 */
internal class DriftTrimmedOutput(
    private val track: PlaybackOutput,
    format: OutputFormat,
) : PlaybackOutput {
    private val packetFrames = format.packetFrames.toInt()
    private val trimmer = DriftTrimmer(format.sampleRate.toInt())
    private val catchUpWindow = CatchUpWindow(format.sampleRate)

    /** Each packet waiting, at position 0 with its bytes up to its limit. */
    private val queue = PlayoutQueue<ByteBuffer>(MAX_QUEUED_FRAMES)

    /** Buffers written out or given up on, kept to queue the next packets in. */
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

    /**
     * The samples handed over that never reached the track, but for the slow trim's, and the
     * packets that lost some.
     */
    var discardedSamples = 0L
        private set
    var packetsWithDiscardedSamples = 0L
        private set

    /** The frames waiting before the track. */
    val queuedFrames: Long get() = queue.queuedFrames

    override val capacityFrames: Long get() = track.capacityFrames

    /** The track's readings, its presented frame counted in the frames the core handed over. */
    override fun read(readings: MutableOutputReadings) {
        track.read(readings)
        if (readings.hasPresentedFrame) readings.presentedFrame += removedFrames
    }

    /** Queues a copy of the [size] bytes at [pcm]'s position; nothing reaches the track until [playNext]. It takes every byte. */
    override fun write(pcm: ByteBuffer, size: Int): Int {
        val copy = bufferFor(size)
        // The source's limit goes back to where it was, as the core's reader left it.
        val source: Buffer = pcm
        val limit = source.limit()
        source.limit(source.position() + size)
        copy.put(pcm)
        source.limit(limit)
        val view: Buffer = copy
        view.flip()
        discard(queue.add(copy, size / BYTES_PER_FRAME))
        return size
    }

    override fun setTargetFrames(frames: Long): Long =
        track.setTargetFrames(frames).also { applied ->
            askedTargetFrames = frames
            appliedTargetFrames = applied
        }

    /**
     * Writes the oldest packet waiting to the track, after the surplus that stood is caught up
     * ([catchUp]) or trimmed when a surplus has persisted; false when none waits. What the track
     * does not take is given up on, and a write that throws loses the packet: it is not written
     * again.
     */
    fun playNext(): Boolean {
        var packet = queue.poll() ?: return false
        var written = false
        try {
            val targetFrames = targetFrames()
            val onTrack = if (targetFrames > 0) onTrack(targetFrames) else null
            if (onTrack != null) {
                val surplus = onTrack + queue.queuedFrames - targetFrames
                val budget = catchUpWindow.onPacket(packetFrames, surplus, targetFrames)
                if (budget > 0) packet = catchUp(packet, budget)
                val cut = trimmer.onPacket(packetFrames, onTrack + queue.queuedFrames, targetFrames)
                if (cut > 0) splice(packet, cut)
            }
            val bytes = packet.limit()
            val taken = track.write(packet, bytes)
            // Only what the track took is written: a short count or an error code loses the rest.
            val takenFrames = taken.coerceIn(0, bytes) / BYTES_PER_FRAME
            framesWritten += takenFrames
            written = true
            discard((bytes / BYTES_PER_FRAME - takenFrames).toLong())
        } finally {
            // A packet that did not reach the track is never written again.
            if (!written) discard((packet.limit() / BYTES_PER_FRAME).toLong())
            spare.addLast(packet)
        }
        return true
    }

    override fun close() = track.close()

    /**
     * Gives up on [head], the packet about to play, and on the oldest whole packets after it, as many
     * as [budget] frames hold, keeping at least one; answers the first packet kept, begun by [head]'s
     * first frames ([join]). Answers [head] itself when not even it fits in [budget] or nothing
     * follows it.
     */
    private fun catchUp(head: ByteBuffer, budget: Long): ByteBuffer {
        var dropped = framesOf(head)
        if (dropped > budget) return head
        var kept = queue.poll() ?: return head
        while (dropped + framesOf(kept) <= budget) {
            val next = queue.poll() ?: break
            dropped += framesOf(kept)
            discard(framesOf(kept))
            spare.addLast(kept)
            kept = next
        }
        // Joined to the packet kept, the head plays as long as that packet: its own frames go.
        discard(framesOf(head))
        join(head, kept)
        spare.addLast(head)
        return kept
    }

    /**
     * [packet] begun by the first frames of [head], the packet given up on before it: the two are
     * spliced ([PcmSplice]) so that the audio on the track goes on into the head and fades into the
     * packet, whose tail leads into what follows. Written into [packet], as long as it was.
     */
    private fun join(head: ByteBuffer, packet: ByteBuffer) {
        val keptSamples = packet.limit() / 2
        val headSamples = minOf(head.limit() / 2, keptSamples)
        val samples = ShortArray(headSamples + keptSamples)
        head.asShortBuffer().get(samples, 0, headSamples)
        packet.asShortBuffer().get(samples, headSamples, keptSamples)
        val joined = PcmSplice.removeFrames(samples, PlaybackLatency.CHANNELS, headSamples / PlaybackLatency.CHANNELS)
        packet.asShortBuffer().put(joined)
    }

    /** The cushion: the buffer the track applied, or the one asked when it could not say. */
    private fun targetFrames(): Long = if (appliedTargetFrames > 0) appliedTargetFrames else askedTargetFrames

    /**
     * The audio still to play on the track, never more than [targetFrames] (what it reports beyond
     * the buffer it is sized to is a stale position); null when it cannot say yet.
     */
    private fun onTrack(targetFrames: Long): Long? {
        track.read(measured)
        if (!measured.hasPresentedFrame) return null
        return minOf(maxOf(framesWritten - measured.presentedFrame, 0L), targetFrames)
    }

    /** [frames] handed over that will never reach the track: given up on, counted as discarded. */
    private fun discard(frames: Long) {
        if (frames <= 0) return
        removedFrames += frames
        discardedSamples += frames * PlaybackLatency.CHANNELS
        packetsWithDiscardedSamples++
    }

    private fun framesOf(packet: ByteBuffer): Long = (packet.limit() / BYTES_PER_FRAME).toLong()

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
