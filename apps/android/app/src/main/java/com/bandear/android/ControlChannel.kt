package com.bandear.android

import com.bandear.android.core.ChannelGroup
import com.bandear.android.core.ClientMessageException
import com.bandear.android.core.MixCommand
import com.bandear.android.core.PlaybackBufferSetting
import com.bandear.android.core.Registration
import com.bandear.android.core.ServerMessage
import com.bandear.android.core.ServerMessageException
import com.bandear.android.core.SourceChannel
import com.bandear.android.core.allClientCapabilities
import com.bandear.android.core.mixMessage
import com.bandear.android.core.parseServerMessage
import com.bandear.android.core.statsMessage
import com.bandear.android.core.ChannelKind as CoreChannelKind
import com.bandear.android.core.MixReport as CoreMixReport
import java.util.concurrent.atomic.AtomicBoolean

/**
 * How long the channel waits after the last keystroke before re-announcing a new name.
 *
 * Normal typing pauses between keystrokes are around 100-250 ms, well under this, so a name typed
 * at speed becomes one registration message per pause instead of one per key. It stays short
 * enough that the desktop, which polls the engine every 250 ms, shows the new name within about
 * half a second of the musician finishing the word.
 */
private const val NAME_RESEND_DEBOUNCE_MS = 400L

/** The WebSocket close code for a deliberate, normal close. */
private const val NORMAL_CLOSURE = 1000

/**
 * The WebSocket control channel, its connection state machine, sending and closing.
 *
 * The channel owns its [transport] (OkHttp WebSockets in production) and its [scheduler] (one
 * background thread in production). It reports opened, acknowledged, config, pushed and answered
 * mixes, error and closed events through [Listener], so the Activity only renders and wires.
 * [running] is the shared session flag owned by the caller.
 *
 * While the session runs, a connection that fails or closes is reopened on its own, following
 * [ReconnectPolicy], to the server and control port given to [connect]. Every open re-sends the
 * registration and then the current mix, so the server gets this musician back as they were. The
 * audio receiver is never touched. [close] stops the session and any pending attempt.
 *
 * Every mix carries [mixSequence], the number of the phone's newest change, and the server answers
 * with it. The channel only carries the number both ways; the caller decides which answer counts.
 *
 * The shared core reads the desk's messages ([parseServerMessage]) and writes the phone's
 * ([Registration], [mixMessage], [statsMessage]); the channel hands what it read on in the app's own
 * types and decides what the musician sees.
 */
internal class ControlChannel(
    private val listener: Listener,
    private val running: AtomicBoolean,
    private val currentSnapshot: () -> MixSnapshot,
    private val moreOfMeEnabled: () -> Boolean,
    /**
     * The channels More of me lifts: the musician's channel and, when its strip is a stereo pair, the
     * other side too; empty when no channel is chosen.
     */
    private val musicianChannels: () -> List<Int>,
    /** The number of the phone's newest mix change, sent as `seq`; null sends none. */
    private val mixSequence: () -> Long? = { null },
    private val transport: ControlTransport = OkHttpControlTransport(),
    private val scheduler: ControlScheduler = ExecutorControlScheduler(),
    /** The channels the engineer locked, which More of me leaves as they are. */
    private val lockedChannels: () -> Set<Int> = { emptySet() },
) {
    interface Listener {
        fun onOpened()
        /**
         * The server announced [channels] source channels, its [groups] and the channels' [details]
         * (names, kinds, hidden channels and stereo pairs, one per source channel); [details] is null
         * when the `config` carried none, as an older server's does.
         */
        fun onConfig(channels: Int, groups: List<GroupInfo>, details: List<ChannelInfo>?)
        fun onAcknowledged()

        /**
         * The server pushed this musician's whole mix (`mix_state`) because it changed elsewhere,
         * such as on the desktop, with the engineer's controls. The channel sends nothing back: the
         * server already plays it. A listener that does not follow the server's mix keeps this
         * default and ignores it.
         */
        fun onMixPushed(report: MixReport) = Unit

        /**
         * The server answered the phone's mix number [seq]: [applied] is the whole mix it now plays
         * (its `mix_ack`) with the engineer's controls, or null when it refused the mix (an `error`)
         * or its ack had no readable mix. Answers without a number (from a server that predates
         * them) are never reported here.
         */
        fun onMixAnswered(seq: Long, applied: MixReport?) = Unit

        /**
         * The desk set this musician's playback buffer (`playback`): [fixedMs] is a fixed length in
         * milliseconds, or [PlaybackLatency.DESK_BUFFER_AUTO] for Auto. The channel sends nothing back:
         * the cushion played says it in the next `stats`. A message the phone cannot use is ignored.
         */
        fun onPlaybackBuffer(fixedMs: Int) = Unit

        /**
         * The desk sends this musician's redundant copies [holdPackets] packets late (`redundancy`):
         * the receive loop holds packets back that long so a copy can fill its lost original's gap; 0
         * for none (redundancy off, or copies right after their original). Nothing is sent back. A
         * message the phone cannot use is ignored.
         */
        fun onRedundancyHold(holdPackets: Int) = Unit

        fun onClosed()
        fun onControlError(message: String)
    }

    /** The open socket that sends go to; null while no connection is open. */
    @Volatile private var controlWebSocket: ControlSocket? = null
    @Volatile private var registrationMessage: String? = null
    @Volatile private var announcedUdpPort: Int? = null
    @Volatile private var announcedClientId: String? = null
    private var pendingNameResend: ScheduledTask? = null
    @Volatile var controlState = ControlState.IDLE
    @Volatile var mixAcknowledged = false
        private set

    // Reconnection state. Compound decisions on it are made while holding [connectionLock].
    private val connectionLock = Any()
    private val generations = ConnectionGenerations()
    private val reconnectPolicy = ReconnectPolicy()
    private var serverHost: String? = null
    private var serverControlPort = 0

    /** The socket of the current attempt, from its creation until it ends; [close] closes it. */
    private var currentSocket: ControlSocket? = null
    private var pendingReconnect: ScheduledTask? = null

    /** Set by [close]: the session was stopped, so nothing reconnects until the next [connect]. */
    private var stopped = true

    /** Whether the current connection has heard from the server yet; see [connectionHeardFrom]. */
    private var connectionProved = false

    /** Whether the current outage already showed its error; one error per outage, not per retry. */
    private var outageReported = false

    /**
     * Listens to one connection attempt. Its [generation] ties every callback to the socket it came
     * from: the callbacks of a socket that is no longer current never change the channel's state,
     * and such a socket is only ever closed, never used.
     */
    private inner class ConnectionListener(private val generation: Long) : ControlSocketEvents {
        override fun onOpen(socket: ControlSocket) {
            if (!adoptOpenedSocket(generation, socket)) {
                // Stopped or replaced while it was still opening.
                socket.close(NORMAL_CLOSURE, "superseded")
                return
            }
            listener.onOpened()
            // Registration goes first: the server adds this client as a target from it, so the mix
            // that follows finds a target to join. A reconnect does exactly the same, so the server
            // gets this musician's name and current mix back.
            sendRegistration()
            sendOpeningMix()
        }

        override fun onMessage(socket: ControlSocket, text: String) {
            if (!connectionHeardFrom(generation)) return
            try {
                handle(parseServerMessage(text))
            } catch (error: ServerMessageException) {
                handleUnreadable(error)
            } catch (error: Exception) {
                listener.onControlError("$INVALID_RESPONSE: ${error.message ?: "unknown error"}")
            }
        }

        override fun onClosing(socket: ControlSocket, code: Int, reason: String) {
            // The server is closing. Answer, so the close handshake completes, and treat the
            // connection as ended now: the onClosed that follows belongs to an ended generation.
            socket.close(NORMAL_CLOSURE, null)
            connectionEnded(generation, failure = null)
        }

        override fun onFailure(socket: ControlSocket, error: Throwable) {
            connectionEnded(generation, failure = "WebSocket error: ${error.message ?: "connection failed"}")
        }

        override fun onClosed(socket: ControlSocket, code: Int, reason: String) {
            connectionEnded(generation, failure = null)
        }
    }

    /**
     * Opens the control channel and remembers what to announce when it opens: the musician's name
     * and the UDP port this client listens on. A server that does not understand the registration
     * answers with an error or ignores it, and the rest of the session behaves as before.
     *
     * The host and control port are kept for the whole session, so a lost connection can be
     * reopened to the same server without the musician pressing anything. The [clientId], when
     * given, travels in every registration of the session, so a phone that reconnects from a new
     * IP keeps its row on the server.
     */
    fun connect(host: String, port: Int, udpPort: Int, musicianName: String, clientId: String? = null) {
        announcedUdpPort = udpPort
        announcedClientId = clientId
        registrationMessage = registerMessage(musicianName, udpPort, clientId)
        // The desk would refuse the whole registration; nothing is sent, and the mix still is.
        if (registrationMessage == null) listener.onControlError(UNREGISTRABLE_PORT)
        synchronized(connectionLock) {
            serverHost = host
            serverControlPort = port
            stopped = false
            reconnectPolicy.reset()
            outageReported = false
            cancelPendingReconnectLocked()
        }
        scheduler.execute { attemptConnection() }
    }

    /**
     * Opens one connection to the stored server, on the control scheduler. Does nothing once the
     * session stopped or the receiver ended, so an attempt that was already due when Stop came
     * never connects.
     */
    private fun attemptConnection() {
        val host: String
        val port: Int
        val generation: Long
        synchronized(connectionLock) {
            pendingReconnect = null
            if (!ReconnectPolicy.shouldReconnect(running.get(), stopped)) return
            host = serverHost ?: return
            port = serverControlPort
            generation = generations.begin()
            connectionProved = false
        }
        try {
            val socket = transport.open("ws://$host:$port", ConnectionListener(generation))
            // The attempt can fail, or the session can stop, before open returns: only a socket
            // that is still current is kept, and a stopped one is dropped before it opens.
            val kept = synchronized(connectionLock) {
                generations.isCurrent(generation).also { current -> if (current) currentSocket = socket }
            }
            if (!kept) socket.cancel()
        } catch (error: Exception) {
            if (running.get()) listener.onControlError("Could not start WebSocket: ${error.message ?: "unknown error"}")
        }
    }

    /**
     * Makes the socket of [generation] the open one, if that attempt is still current. Opening alone
     * does not reset the backoff: a server that accepts and drops at once must still back off.
     */
    private fun adoptOpenedSocket(generation: Long, socket: ControlSocket): Boolean {
        synchronized(connectionLock) {
            if (!generations.isCurrent(generation)) return false
            currentSocket = socket
            controlWebSocket = socket
            controlState = ControlState.CONNECTED
        }
        return true
    }

    /**
     * True when a message on connection [generation] should be handled, that is, while it is
     * current. The first message proves the connection healthy (the server sends `config` right
     * after connect): the backoff starts again from its shortest wait, and the next failure counts
     * as a new outage.
     */
    private fun connectionHeardFrom(generation: Long): Boolean {
        synchronized(connectionLock) {
            if (!generations.isCurrent(generation)) return false
            if (!connectionProved) {
                connectionProved = true
                reconnectPolicy.reset()
                outageReported = false
            }
        }
        return true
    }

    /**
     * Handles the end of connection [generation], whichever way it ended. Only the first end of the
     * current connection counts; a stale or repeated one is ignored. While the session runs and was
     * not stopped, the channel shows [ControlState.RECONNECTING] and schedules the next attempt.
     * Ownership of the mix is left exactly as it was.
     *
     * A failure is shown once per outage: the first one of the session, or the first after a
     * connection that had heard from the server. Retries that keep failing only update the state.
     */
    private fun connectionEnded(generation: Long, failure: String?) {
        val error: String?
        synchronized(connectionLock) {
            if (!generations.end(generation)) return
            currentSocket = null
            controlWebSocket = null
            val sessionRunning = running.get()
            val reconnect = ReconnectPolicy.shouldReconnect(sessionRunning, stopped)
            controlState = when {
                reconnect -> ControlState.RECONNECTING
                !sessionRunning -> ControlState.IDLE
                failure != null -> ControlState.UNAVAILABLE
                else -> ControlState.CLOSED
            }
            if (reconnect) scheduleReconnectLocked()
            error = failure?.takeIf { sessionRunning && !outageReported }
            if (error != null) outageReported = true
        }
        listener.onClosed()
        if (error != null) listener.onControlError(error)
    }

    /** Schedules the next attempt after the policy's wait. Call only while holding [connectionLock]. */
    private fun scheduleReconnectLocked() {
        // Null once the scheduler shut down: nothing reconnects after that.
        pendingReconnect = scheduler.schedule(reconnectPolicy.nextDelayMs()) { attemptConnection() }
    }

    /** Cancels a scheduled attempt, if any. Call only while holding [connectionLock]. */
    private fun cancelPendingReconnectLocked() {
        pendingReconnect?.cancel()
        pendingReconnect = null
    }

    /**
     * Re-announces the client under a new name on the open control channel. The name is a label,
     * not part of the audio: no reconnect happens and no receiver is restarted, and the server
     * updates the target it already has, keeping its mix state and counters.
     *
     * Sends are debounced by [NAME_RESEND_DEBOUNCE_MS] so typing produces one message per pause,
     * not one per keystroke. The stored registration is updated too, so a socket that has not
     * opened yet announces the latest name when it does.
     */
    fun updateRegistrationName(name: String) {
        if (!running.get()) return
        val udpPort = announcedUdpPort ?: return
        val message = registerMessage(name, udpPort, announcedClientId) ?: return
        registrationMessage = message
        pendingNameResend?.cancel()
        pendingNameResend = scheduler.schedule(NAME_RESEND_DEBOUNCE_MS) { sendRegistrationMessage(message) }
    }

    /**
     * Sends the registration once, on open, ahead of the first mix. Queued on the same scheduler as
     * the mix sends, so the server always sees the registration first.
     */
    private fun sendRegistration() {
        val message = registrationMessage ?: return
        scheduler.execute {
            sendRegistrationMessage(message)
        }
    }

    private fun sendRegistrationMessage(message: String) {
        val webSocket = controlWebSocket ?: return
        if (!webSocket.send(message) && running.get()) {
            listener.onControlError("WebSocket rejected the registration")
        }
    }

    /**
     * Sends the mix as it stands when the connection opens. It is read now, on the socket's thread,
     * before any message of this connection is handled, and only sent on the control scheduler: the
     * server's `config` arrives right after the open, and a mix read after it would carry the strips
     * `config` made up (neutral, for channels the phone never had) over the server's own values.
     *
     * So on a session's first connect, when the phone has no channels yet, it sends none (nor any
     * group level): the server keeps its channels and groups, and its answer, which arrives after
     * `config`, gives the phone the mix the server holds. Only the master volume, the level cap and
     * the mute, which are always the phone's own, are sent as they are. A reconnect reads the
     * phone's own mix, which the server takes back as it was.
     */
    private fun sendOpeningMix() {
        val message = outgoingMixMessage()
        scheduler.execute {
            val webSocket = controlWebSocket ?: return@execute
            if (!webSocket.send(message) && running.get()) {
                listener.onControlError("WebSocket rejected mix update")
            }
        }
    }

    /**
     * Clears ownership so a fresh session starts with the client owning its mix. The client re-sends
     * its mix on connect and regains ownership on the next acknowledgement; until then the local
     * master volume stays live even if the control channel never comes up.
     */
    fun clearMixOwnership() {
        mixAcknowledged = false
    }

    /**
     * Sends the current mix, on the control scheduler, under the newest change's number. Sends that
     * pile up behind a busy scheduler each read the mix and the number when they run, so the mix on
     * the wire is always the newest one.
     */
    fun sendMixControl() {
        scheduler.execute {
            val webSocket = controlWebSocket ?: return@execute
            if (!webSocket.send(outgoingMixMessage()) && running.get()) {
                listener.onControlError("WebSocket rejected mix update")
            }
        }
    }

    /** The `mix` message for the phone's current mix, under the newest change's number. */
    private fun outgoingMixMessage(): String {
        // The number is read before the mix: a change numbers itself after it lands, so the mix read
        // next is at least as new as the number, and an answer never vouches for a mix older than
        // the one it names.
        val sequence = mixSequence()
        val stored = currentSnapshot()
        // `More of me` is derived here, at send time, and never written back. The stored values stay
        // exactly as the musician left them, so turning the control off re-sends them.
        val outgoing = if (moreOfMeEnabled()) stored.moreOfMeFor(musicianChannels(), lockedChannels()) ?: stored else stored
        return mixMessage(outgoing.toMixCommand(sequence))
    }

    /**
     * Sends one link-stats report on the control scheduler, behind any registration or mix already
     * queued. Called from the receive thread, so it only hands the report over and never waits.
     *
     * A report describes one interval and is worthless later, so it is dropped, never queued, when
     * the channel is not open or has shut down (the scheduler drops work after shutdown). A rejected
     * send is not shown as an error either: the next report follows within a second, and the
     * server's view of a link that stops reporting is exactly what the report is for.
     */
    fun sendLinkStats(report: LinkStatsReport) {
        if (controlWebSocket == null) return
        scheduler.execute {
            controlWebSocket?.send(statsMessage(report.toCoreReport()))
        }
    }

    /**
     * Stops the session's control connection: cancels any scheduled reconnect, makes every socket's
     * callbacks stale and closes the current socket, open or still opening. Nothing reconnects until
     * the next [connect].
     */
    fun close(code: Int, reason: String) {
        val socket: ControlSocket?
        synchronized(connectionLock) {
            stopped = true
            cancelPendingReconnectLocked()
            generations.invalidate()
            socket = currentSocket
            currentSocket = null
            controlWebSocket = null
        }
        socket?.close(code, reason)
    }

    fun shutdown() {
        synchronized(connectionLock) {
            stopped = true
            cancelPendingReconnectLocked()
        }
        scheduler.shutdown()
        transport.shutdown()
    }

    /** Hands on what a readable message from the desk says. */
    private fun handle(message: ServerMessage) {
        when (message) {
            is ServerMessage.Configuration -> {
                val announcedChannels = message.v1.sourceChannels.toInt()
                // A count this phone cannot play is ignored, and the channels stay as they were.
                if (announcedChannels in 1..MAX_CHANNELS) {
                    listener.onConfig(
                        announcedChannels,
                        message.v1.groups.map { it.toGroupInfo() },
                        message.v1.channels?.map { it.toChannelInfo() },
                    )
                }
            }
            is ServerMessage.MixAck -> {
                acknowledge()
                // Without a number an ack cannot be matched to a mix, so it only acknowledges.
                message.v1.sequence?.let { seq -> listener.onMixAnswered(seq.toLong(), message.v1.toMixReport()) }
            }
            // A numbered ack whose mix cannot be read still answers its number.
            is ServerMessage.UnreadableMixAck -> {
                acknowledge()
                listener.onMixAnswered(message.sequence.toLong(), applied = null)
            }
            is ServerMessage.MixState -> listener.onMixPushed(message.v1.toMixReport())
            is ServerMessage.PlaybackBuffer -> listener.onPlaybackBuffer(message.v1.toDeskBufferMs())
            is ServerMessage.RedundancyHold -> listener.onRedundancyHold(message.blocks.toInt())
            is ServerMessage.Error -> {
                // Only an error refusing one of the phone's mixes carries its number; an error
                // answering a registration or a stats report never touches the mixes.
                message.v1.sequence?.let { seq -> listener.onMixAnswered(seq.toLong(), applied = null) }
                listener.onControlError(message.v1.message)
            }
            is ServerMessage.Unrecognized -> Unit
        }
    }

    /**
     * Handles a message from the desk the core could not read. None of them is a failure of the
     * connection: the socket stays open, nothing reconnects, and what the message would have changed
     * stays as it was.
     *
     * - Text that is not a message this phone can read, which includes a known message with a value
     *   of the wrong type ([ServerMessageException.NotJson]), is shown as an invalid response.
     * - A push (`mix_state`) missing part of its mix is never applied, and is shown the same way.
     * - An ack (`mix_ack`) with no number whose mix cannot be read still acknowledges.
     * - An `error` with no text is still the desk refusing something.
     * - Anything else (no `type`, a `config`, `playback` or `redundancy` the phone cannot use) is
     *   ignored, as an unknown type is.
     */
    private fun handleUnreadable(error: ServerMessageException) {
        when (error) {
            is ServerMessageException.NotJson -> listener.onControlError("$INVALID_RESPONSE: not a control message this phone can read")
            is ServerMessageException.MissingType -> Unit
            is ServerMessageException.MissingField ->
                handleUnreadable(error.messageType, "a `${error.messageType}` message with no `${error.field}`")
            is ServerMessageException.UnusableField ->
                handleUnreadable(error.messageType, "a `${error.messageType}` message whose `${error.field}` cannot be used")
        }
    }

    private fun handleUnreadable(messageType: String, reason: String) {
        when (messageType) {
            "mix_state" -> listener.onControlError("$INVALID_RESPONSE: $reason")
            "mix_ack" -> acknowledge()
            "error" -> listener.onControlError("unknown control error")
        }
    }

    private fun acknowledge() {
        mixAcknowledged = true
        listener.onAcknowledged()
    }
}

/** How the musician is told a message from the desk could not be read; never a connection failure. */
private const val INVALID_RESPONSE = "Invalid control response"

/** What the musician is told when the desk would refuse the registration for its UDP port. */
private const val UNREGISTRABLE_PORT = "Could not register: the UDP port must be between 1 and 65535"

/**
 * The registration the shared core writes for [name] listening on [udpPort], with [clientId] when the
 * desk keeps it and every capability this build implements ([allClientCapabilities]). Null for a port
 * the desk refuses, so nothing is sent; the core refuses port 0, and a port outside 0-65535 is no port.
 */
private fun registerMessage(name: String, udpPort: Int, clientId: String?): String? {
    if (udpPort !in 0..UShort.MAX_VALUE.toInt()) return null
    return try {
        Registration(name, udpPort.toUShort(), clientId, allClientCapabilities()).use { it.encodedJson() }
    } catch (_: ClientMessageException) {
        null
    }
}

/** The `mix` message for [this] mix under [sequence]: every list, so the desk plays the whole mix. */
private fun MixSnapshot.toMixCommand(sequence: Long?): MixCommand = toCoreSnapshot().let { mix ->
    MixCommand(
        // A null number puts no key at all, so the message is exactly what older servers read.
        sequence = sequence?.takeIf { it >= 0 }?.toULong(),
        volumePercent = mix.volumePercent,
        maxLevelPercent = mix.maxLevelPercent,
        muted = mix.muted,
        channelGains = mix.channelGains,
        pans = mix.channelPans,
        channelMutes = mix.channelMutes,
        channelSolos = mix.channelSolos,
        groupLevels = mix.groupLevels,
    )
}

/** The desk's buffer as the receive loop follows it: [PlaybackLatency.DESK_BUFFER_AUTO] or milliseconds. */
private fun PlaybackBufferSetting.toDeskBufferMs(): Int = when (this) {
    PlaybackBufferSetting.Auto -> PlaybackLatency.DESK_BUFFER_AUTO
    is PlaybackBufferSetting.Fixed -> milliseconds.toInt()
}

/** A group as the app keeps it; groups are matched by position, so the desk's id is not kept. */
private fun ChannelGroup.toGroupInfo() = GroupInfo(name, channels.filter { it <= Int.MAX_VALUE }.map(Long::toInt))

private fun SourceChannel.toChannelInfo() = ChannelInfo(name, kind.toChannelKind(), hidden = isHidden, link = isLinkedToNext)

private fun CoreChannelKind.toChannelKind(): ChannelKind = when (this) {
    CoreChannelKind.VOICE -> ChannelKind.VOICE
    CoreChannelKind.GUITAR -> ChannelKind.GUITAR
    CoreChannelKind.BASS -> ChannelKind.BASS
    CoreChannelKind.KEYS -> ChannelKind.KEYS
    CoreChannelKind.DRUMS -> ChannelKind.DRUMS
    CoreChannelKind.PERCUSSION -> ChannelKind.PERCUSSION
    CoreChannelKind.HORNS -> ChannelKind.HORNS
    CoreChannelKind.STRINGS -> ChannelKind.STRINGS
    CoreChannelKind.PLAYBACK -> ChannelKind.PLAYBACK
    CoreChannelKind.CLICK -> ChannelKind.CLICK
    CoreChannelKind.OTHER -> ChannelKind.OTHER
}

/**
 * A whole mix as the desk reports it, in the app's own types: the lists are the server's full tables
 * (32 channels, 16 groups), and the client keeps the channels and groups it knows.
 */
private fun CoreMixReport.toMixReport() = MixReport(
    mix = MixSnapshot(
        channelGains = channelGains.toInts(),
        channelPans = pans.toInts(),
        channelMutes = channelMutes.toBooleanArray(),
        channelSolos = channelSolos.toBooleanArray(),
        groupLevels = groupLevels.toInts(),
        volumePercent = volumePercent.toInt(),
        maxLevelPercent = maxLevelPercent.toInt(),
        muted = muted,
    ),
    engineer = EngineerControls(
        limitPercent = engineerLimitPercent.toInt(),
        lockedChannels = locked.indices.filter { locked[it] }.toSet(),
        following = following,
    ),
)
