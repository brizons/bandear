package com.bandear.android

import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioTimestamp
import android.media.AudioTrack
import com.bandear.android.core.MutableOutputReadings
import com.bandear.android.core.OutputFormat
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.SocketException
import java.net.SocketTimeoutException
import java.nio.Buffer
import java.nio.ByteBuffer
import java.nio.channels.DatagramChannel
import java.util.concurrent.atomic.AtomicBoolean
import java.util.function.IntSupplier

/** A frame of the track: 16-bit stereo. */
private const val BYTES_PER_FRAME = 2 * PlaybackLatency.CHANNELS

/**
 * Which receive loop owns the shared session flag: the one launched last.
 *
 * Every launch's loop runs while [running] is up, and a loop that ends lowers it, so a session
 * whose receiver dies on its own ends too. Only the latest launch may do that: a loop still
 * finishing after a newer launch leaves the flag, and so the newer session, alone.
 *
 * A finishing loop lowers the flag, reports its stop, and only then stops being [current]. A caller
 * that launches again only once no receiver is current therefore never finds the receiver free
 * with the old flag still to come down, and gets one stop report per launch, in launch order.
 */
internal class ReceiverLaunches<T : Any>(private val running: AtomicBoolean) {
    /** The latest launch, until its loop has finished and reported; null when none is running. */
    @Volatile
    var current: T? = null
        private set

    /** [receiver] is the latest launch from now on; an older loop can no longer end the session. */
    @Synchronized
    fun launched(receiver: T) {
        current = receiver
    }

    /**
     * [receiver]'s loop ended. While it is still the latest launch it lowers the session flag; it
     * reports through [report] either way, and then stops being [current].
     */
    fun finished(receiver: T, report: () -> Unit) {
        synchronized(this) {
            if (current === receiver) running.set(false)
        }
        try {
            report()
        } finally {
            synchronized(this) {
                if (current === receiver) current = null
            }
        }
    }
}

/**
 * The UDP receive loop and AudioTrack playback, around the shared core's receive pipeline.
 *
 * The player owns the socket, the receiver thread and the audio track, so it needs no `Context`;
 * every rule between a datagram and the samples played, the receiver metrics and the link report
 * among them, is the shared core's, reached once per wake-up through a [ReceiveSession].
 * It reports metrics, errors, completion and the `server owns the mix` decision through [Listener];
 * it never touches the UI itself. [running] is the shared session flag owned by the caller, so the
 * start/stop gating is identical to before the split. A loop that ends lowers the flag only while
 * it is the latest launch ([ReceiverLaunches]), so a loop still finishing cannot end a newer session.
 */
internal class PmonPlayer(
    private val listener: Listener,
    private val running: AtomicBoolean,
    private val mixState: ClientMixState,
) {
    interface Listener {
        /** True once the server has acknowledged a mix, so local processing must not run. */
        fun serverOwnsMix(): Boolean

        fun onStats(
            packets: Long,
            lost: Long,
            sampleRate: Int?,
            packetsPerSecond: Double,
            interArrivalMs: Double,
            jitterMs: Double,
            discardedSamples: Long,
            packetsWithDiscardedSamples: Long,
            playbackBacklogMs: Double,
        )

        /**
         * One link report for the server, about once a second once the audio starts or a startup
         * grace passes without it, packets or not (the shared core's link stats). Called on the
         * receive thread, which also feeds the audio track, so it must only hand the report over
         * and never block.
         */
        fun onLinkStats(report: LinkStatsReport)

        fun onError(message: String)

        /**
         * The loop of one launch ended, on request or on its own: called exactly once per launch,
         * on the receive thread, after the session flag went down if this was the latest launch.
         * The receiver still counts as alive until this returns, so a caller that waits for it to
         * be free before launching gets the reports in launch order.
         */
        fun onStopped()
    }

    @Volatile private var socket: DatagramSocket? = null
    private val launches = ReceiverLaunches<Thread>(running)

    fun isReceiverAlive(): Boolean = launches.current?.isAlive == true

    fun launch(host: String, port: Int) {
        val thread = Thread({ receiveLoop(host, port) }, "pmon-udp-receiver")
        launches.launched(thread)
        thread.start()
    }

    fun stopReceiving() {
        socket?.close()
        launches.current?.interrupt()
    }

    private fun receiveLoop(host: String, port: Int) {
        try {
            ReceiveSession(listener, mixState, ::openOutput, running::get).use { session ->
                try {
                    InetAddress.getByName(host)
                } catch (error: Exception) {
                    listener.onError("Could not resolve server host: ${error.message ?: "unknown error"}")
                    return
                }
                // A channel's socket, so what is already waiting can be read without blocking.
                DatagramChannel.open().use { channel ->
                    val openedSocket = channel.socket()
                    socket = openedSocket
                    openedSocket.bind(InetSocketAddress(port))
                    openedSocket.soTimeout = 250
                    val buffer = ByteArray(65_507)
                    val datagram = DatagramPacket(buffer, buffer.size)
                    val waiting = ByteBuffer.wrap(buffer)
                    // Cleared through Buffer: ByteBuffer's covariant clear() is newer than some
                    // Android releases the app runs on.
                    val waitingView: Buffer = waiting
                    // The channel blocks for the timed receive and not for the drains, switched
                    // only when the next read needs the other mode.
                    var blocking = true
                    // Made once: the datagram stays where the socket put it, and the core reads
                    // exactly the bytes received from there.
                    val receive = IntSupplier {
                        if (!blocking) {
                            channel.configureBlocking(true)
                            blocking = true
                        }
                        datagram.setLength(buffer.size)
                        try {
                            openedSocket.receive(datagram)
                            datagram.length
                        } catch (_: SocketTimeoutException) {
                            // Nothing came for a while: what is held is not worth waiting for.
                            ReceiveSession.TIMED_OUT
                        }
                    }
                    val receiveWaiting = IntSupplier {
                        if (blocking) {
                            channel.configureBlocking(false)
                            blocking = false
                        }
                        waitingView.clear()
                        if (channel.receive(waiting) == null) ReceiveSession.NOTHING_WAITING else waiting.position()
                    }
                    while (running.get()) {
                        // One call into the shared core per datagram or receive timeout, each one
                        // the socket already holds included: the link report rides on these
                        // wake-ups, so it is at most one timeout late.
                        session.wake(buffer, receiveWaiting, receive)
                    }
                }
            }
        } catch (error: SocketException) {
            if (running.get()) listener.onError("UDP socket error: ${error.message ?: "unknown error"}")
        } catch (error: Exception) {
            if (running.get()) listener.onError("Receiver error: ${error.message ?: "unknown error"}")
        } catch (error: LinkageError) {
            // The shared core's library did not load: an error to show, not a crash of the app.
            if (running.get()) listener.onError("Receiver error: ${error.message ?: "the audio core did not load"}")
        } finally {
            // The session closed the track and the core's state on its way out of `use`.
            socket = null
            // The flag goes down (only if this is still the latest launch) before the receiver is
            // free again, so no new session can start in between and be ended by this loop.
            launches.finished(Thread.currentThread()) { listener.onStopped() }
        }
    }

    /**
     * The output the shared core asked for: a track at the first packet's rate and size. The rate is
     * the packet's, as the core measures the cushion and the backlog at it; a rate the platform
     * cannot play fails the open, which the session reports, and the next packet asks again.
     */
    private fun openOutput(format: OutputFormat): PlaybackOutput =
        AudioTrackOutput(openLowLatencyTrack(format.sampleRate.toInt(), format.packetFrames.toInt()))

    /**
     * Opens and starts the track, with room for the longest fixed buffer the desk may set
     * ([PlaybackLatency.trackCapacityFrames], 16-bit stereo) when the platform honours a smaller
     * buffer size on it, else at the platform minimum as before the desk could set a buffer.
     *
     * The large capacity only costs no latency if the buffer size set right after is what is queued,
     * so the smallest Auto cushion (one packet) is asked and the size the track reports is checked
     * ([PlaybackLatency.keepsLowLatency]). A track that will not shrink is released and reopened at
     * the minimum: Auto plays as tight as before, and a fixed buffer goes only as long as that track
     * holds (its capacity is what the cushion is clamped to), the cushion reported being the size
     * the track applied. A track that fails to start is released before the failure goes on
     * ([openTrackKeepingLowLatency]).
     */
    private fun openLowLatencyTrack(sampleRate: Int, packetFrames: Int): AudioTrack {
        val minimum = AudioTrack.getMinBufferSize(
            sampleRate,
            AudioFormat.CHANNEL_OUT_STEREO,
            AudioFormat.ENCODING_PCM_16BIT,
        )
        if (minimum <= 0) throw IllegalStateException("sample rate $sampleRate is not supported")
        // The smallest Auto cushion: one packet.
        val wanted = packetFrames.coerceAtLeast(1)
        return openTrackKeepingLowLatency(
            openLarge = {
                createAudioTrack(
                    sampleRate,
                    maxOf(minimum, PlaybackLatency.trackCapacityFrames(sampleRate, packetFrames) * BYTES_PER_FRAME),
                )
            },
            openMinimum = { createAudioTrack(sampleRate, minimum) },
            start = { track -> track.play() },
            applyLowestCushion = { track -> applyPlaybackBufferTarget(track, wanted) },
            keepsLowLatency = { applied ->
                PlaybackLatency.keepsLowLatency(wanted, applied, minimum / BYTES_PER_FRAME, sampleRate)
            },
            close = ::closeTrack,
        )
    }

    /** Builds a low-latency stream track at [sampleRate] with a capacity of [bufferBytes]. */
    private fun createAudioTrack(sampleRate: Int, bufferBytes: Int): AudioTrack {
        return AudioTrack.Builder()
            .setAudioAttributes(
                AudioAttributes.Builder()
                    // USAGE_MEDIA, not USAGE_GAME: measured on a Galaxy S23 the two are equal
                    // (playback backlog 33.8 ms against 34.0 ms, same framework track latency), so
                    // the device's game effect chain was not the latency term. Media is kept
                    // because it is the semantically right usage for music and it still qualifies
                    // for the low-latency fast track.
                    .setUsage(AudioAttributes.USAGE_MEDIA)
                    .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
                    .build(),
            )
            .setAudioFormat(
                AudioFormat.Builder()
                    .setEncoding(AudioFormat.ENCODING_PCM_16BIT)
                    .setSampleRate(sampleRate)
                    .setChannelMask(AudioFormat.CHANNEL_OUT_STEREO)
                    .build(),
            )
            .setBufferSizeInBytes(bufferBytes)
            .setTransferMode(AudioTrack.MODE_STREAM)
            .setPerformanceMode(AudioTrack.PERFORMANCE_MODE_LOW_LATENCY)
            .build()
            .also { track ->
                if (track.state != AudioTrack.STATE_INITIALIZED) {
                    track.release()
                    throw IllegalStateException("could not initialize AudioTrack")
                }
            }
    }
}

/**
 * The receive session's output on the platform: an open, started [track], played with blocking
 * writes, so every frame handed over is queued ([MutableOutputReadings.hasWritableFrames] is false).
 */
private class AudioTrackOutput(private val track: AudioTrack) : PlaybackOutput {
    private val timestamp = AudioTimestamp()

    override val capacityFrames: Long = track.bufferCapacityInFrames.toLong()

    override fun read(readings: MutableOutputReadings) {
        readings.hasWritableFrames = false
        readings.underrunCount = track.underrunCount.toLong()
        // The frame reaching the listener now, once the track can say.
        readings.hasPresentedFrame = track.getTimestamp(timestamp)
        readings.presentedFrame = if (readings.hasPresentedFrame) timestamp.framePosition else 0L
        readings.isPlaying = track.playState == AudioTrack.PLAYSTATE_PLAYING
    }

    /**
     * The bytes go to the track as they lie in the core's outcome buffer, a direct one: a
     * `ByteBuffer` write takes any PCM encoding in the platform's byte order, which is little
     * endian on every ABI the app ships, so no sample is copied or converted in Kotlin. A short
     * count only comes from a write error.
     */
    override fun write(pcm: ByteBuffer, size: Int) {
        track.write(pcm, size, AudioTrack.WRITE_BLOCKING)
    }

    override fun setTargetFrames(frames: Long): Long =
        applyPlaybackBufferTarget(track, frames.coerceIn(1L, Int.MAX_VALUE.toLong()).toInt())
            .coerceAtLeast(0)
            .toLong()

    override fun close() = closeTrack(track)
}

/** Stops [track] and gives back its resources; a track that never played is released too. */
private fun closeTrack(track: AudioTrack) {
    try {
        track.pause()
        track.flush()
    } catch (_: IllegalStateException) {
        // Not playing: there is nothing to stop, only to release.
    }
    track.release()
}

/**
 * Asks the track for a playback buffer of [frames]: the cushion the shared core sets.
 *
 * The track's buffer occupancy is the playback latency, so a buffer that is merely the device's
 * default can sit tens of milliseconds behind the render position and pay that on every sample.
 * The platform clamps the request up to its own hardware minimum, so a target below the floor
 * degrades to the floor rather than failing; a refused call leaves the buffer as it was.
 * Answers the buffer size the track reports afterwards, in frames, or -1 when it cannot say.
 */
private fun applyPlaybackBufferTarget(track: AudioTrack, frames: Int): Int =
    try {
        track.setBufferSizeInFrames(frames)
        track.bufferSizeInFrames
    } catch (_: IllegalStateException) {
        // Keep the buffer the track was created with.
        -1
    }
