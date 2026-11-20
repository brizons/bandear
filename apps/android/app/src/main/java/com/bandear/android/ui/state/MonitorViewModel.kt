package com.bandear.android.ui.state

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import com.bandear.android.ClientMixState
import com.bandear.android.ControlChannel
import com.bandear.android.ControlState
import com.bandear.android.DiscoveredServer
import com.bandear.android.LinkStatsReport
import com.bandear.android.PlaybackLatency
import com.bandear.android.PmonPlayer
import com.bandear.android.ServerDiscovery
import com.bandear.android.core.DeskConfiguration
import com.bandear.android.core.MixBankChange
import com.bandear.android.core.MixBankOutcome
import com.bandear.android.core.MixEvent
import com.bandear.android.core.MixModel
import com.bandear.android.core.MixModelState
import com.bandear.android.core.MixUpdate
import com.bandear.android.ui.components.MainTab
import com.bandear.android.ui.screens.MonitorActions
import com.bandear.android.ui.state.MonitorEvent.AudioError
import com.bandear.android.ui.state.MonitorEvent.ConnectRejected
import com.bandear.android.ui.state.MonitorEvent.ControlChanged
import com.bandear.android.ui.state.MonitorEvent.ControlError
import com.bandear.android.ui.state.MonitorEvent.LinkStatsReported
import com.bandear.android.ui.state.MonitorEvent.ManualAddressEdited
import com.bandear.android.ui.state.MonitorEvent.MixAcknowledged
import com.bandear.android.ui.state.MonitorEvent.MixBankChanged
import com.bandear.android.ui.state.MonitorEvent.MixRecallTried
import com.bandear.android.ui.state.MonitorEvent.NameChanged
import com.bandear.android.ui.state.MonitorEvent.ServersChanged
import com.bandear.android.ui.state.MonitorEvent.SessionEnded
import com.bandear.android.ui.state.MonitorEvent.SessionStarted
import com.bandear.android.ui.state.MonitorEvent.StatsUpdated
import com.bandear.android.ui.state.MonitorEvent.TabSelected
import com.bandear.android.wellFormedForCore
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import java.util.concurrent.atomic.AtomicBoolean

/** The WebSocket close code for a deliberate, normal close. */
private const val NORMAL_CLOSE = 1000

private val PORTS = 1..65535

/**
 * Owns a monitoring session and everything the screens show, in one [state].
 *
 * It holds what `MainActivity` held: the session flag, the control channel, the audio receiver,
 * discovery, the saved mixes and the client id, all built from [MonitorCollaborators]. The screens
 * read [state] and call the intent functions. The mix is the shared core's [MixModel]: the
 * musician's edits, the desk's reports and the saved-mix actions go to it as they happen, and each
 * call says what to show, what `mix` to send and what to store. Everything else the collaborators
 * report becomes a [MonitorEvent] for the pure [reduce]. A mix the server pushes (changed on the
 * desktop) is shown and heard, and never sent back.
 *
 * Callbacks arrive on the receiver, control and discovery threads. The mix model is called from the
 * main thread and the control thread, through [MixChanges], which shows its states and queues its
 * mixes in the order its calls landed; the rest of the state is updated by an atomic
 * compare-and-set of a pure step, so any thread may report. Starting and ending a session is
 * serialised by one lock, as the Activity's UI thread serialised it. A session ends once: the
 * receiver's stop report only ends the session it belongs to, so a report arriving late cannot end
 * a session twice or close a session started after it, and after [onCleared] it touches nothing.
 * Intents are meant for the main thread. They are the screens' [MonitorActions], so the app binds
 * them directly.
 */
internal class MonitorViewModel(collaborators: MonitorCollaborators) : ViewModel(), MonitorActions {
    private val store = collaborators.store
    private val clientId = collaborators.clientId
    private val keepAlive = collaborators.keepAlive

    /** The shared session flag: the receive loop and the control channel both stop on it. */
    private val running = AtomicBoolean(false)

    /** The master values the receive thread reads for local protection; mirrored from the mix model. */
    private val playerMix = ClientMixState()

    private val sessionLock = Any()

    // Which session a receiver stop report belongs to. The receiver reports exactly one stop for
    // each launch, from the launch's `finally`, in launch order, so the n-th report is the n-th
    // session's. All three are guarded by [sessionLock].

    /** Sessions launched so far; the current (or last) session is number [sessionsLaunched]. */
    private var sessionsLaunched = 0L

    /** Stop reports received so far. */
    private var stopsReported = 0L

    /** The last session torn down, by Stop or by its receiver; it is never torn down again. */
    private var lastEndedSession = 0L

    /** Set by [onCleared]: the channel and discovery are shut down, so nothing may end a session. */
    private var cleared = false

    /**
     * The musician's mix: the shared core's model, holding the session's mix and the saved mixes,
     * with the musician channel and the saved mixes kept on the phone. Never closed by hand: the
     * control thread may still hand it a late message after [onCleared], and UniFFI frees it with
     * the ViewModel.
     */
    private val mix = MixChanges(
        model = MixModel(store.musicianChannel()?.toLong(), store.loadSavedMixes()),
        show = ::showMix,
        send = { command -> control.sendMix(command) },
    )

    private val _state = MutableStateFlow(MonitorUiState(musicianName = store.musicianName()).showing(mix.state()))

    /** Everything the screens show. */
    val state: StateFlow<MonitorUiState> = _state.asStateFlow()

    private val receiverEvents = object : PmonPlayer.Listener {
        override fun serverOwnsMix(): Boolean = control.mixAcknowledged

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
            dispatch(
                StatsUpdated(
                    ReceiverStats(
                        packets = packets,
                        lost = lost,
                        sampleRate = sampleRate,
                        packetsPerSecond = packetsPerSecond,
                        interArrivalMs = interArrivalMs,
                        jitterMs = jitterMs,
                        discardedSamples = discardedSamples,
                        packetsWithDiscardedSamples = packetsWithDiscardedSamples,
                        playbackBacklogMs = playbackBacklogMs,
                    ),
                ),
            )
        }

        override fun onLinkStats(report: LinkStatsReport) {
            control.sendLinkStats(report)
            dispatch(LinkStatsReported(report))
        }

        override fun onError(message: String) {
            dispatch(AudioError(message))
        }

        override fun onStopped() = receiverStopped()
    }

    private val controlEvents = object : ControlChannel.Listener {
        override fun onOpened() = publishControlState()

        override fun onConfig(configuration: DeskConfiguration) = changeMix(MixEvent.ConfigReceived(configuration))

        override fun onAcknowledged() {
            dispatch(MixAcknowledged(control.controlState))
        }

        // A push or an answer: the model shows it only when it is the server's latest word, and
        // never sends it back, except the first report after a link, which it numbers itself.
        override fun onMixReported(event: MixEvent) = changeMix(event)

        // Handed to the receive thread as is: it follows the desk's buffer at its next packet.
        override fun onPlaybackBuffer(fixedMs: Int) {
            playerMix.deskBufferMs = fixedMs
        }

        // Likewise the hold for delayed copies: the receive thread holds packets that long from its
        // next packet.
        override fun onRedundancyHold(holdPackets: Int) {
            playerMix.redundancyHoldPackets = holdPackets
        }

        override fun onClosed() = publishControlState()

        override fun onControlError(message: String) {
            dispatch(ControlError(message))
        }
    }

    private val scannerEvents = object : ServerDiscovery.Listener {
        override fun onServersChanged(servers: List<DiscoveredServer>) {
            dispatch(ServersChanged(servers.map { ServerEndpoint(it.address, it.controlPort) }))
        }
    }

    private val control: ControlChannel = ControlChannel(
        listener = controlEvents,
        running = running,
        // Read and queued in order with every other mix, so it never overtakes a newer change.
        sendOpeningMix = { mix.sendOpeningMix() },
        transport = collaborators.controlTransport,
        scheduler = collaborators.controlScheduler,
    )
    private val receiver: AudioReceiver = collaborators.receiverFactory(receiverEvents, running, playerMix)
    private val scanner: ServerScanner = collaborators.scannerFactory(scannerEvents)

    init {
        scanner.start()
    }

    // The session.

    /** Connects to a found [server], listening for audio on the port set in Settings. */
    override fun connect(server: ServerEndpoint) {
        startSession(server.address, server.controlPort.toString(), _state.value.connection.manual.audioPort)
    }

    /** Connects to a typed address. The form keeps what was typed, so a mistake can be corrected. */
    override fun connectManual(host: String, controlPort: String, audioPort: String) {
        dispatch(ManualAddressEdited(ManualAddress(host, controlPort, audioPort)))
        startSession(host, controlPort, audioPort)
    }

    override fun editManualAddress(address: ManualAddress) {
        dispatch(ManualAddressEdited(address))
    }

    private fun startSession(hostText: String, controlPortText: String, audioPortText: String) {
        synchronized(sessionLock) {
            if (running.get()) return
            if (receiver.isAlive()) return reject("The previous receiver is still stopping")
            val host = hostText.trim()
            val audioPort = audioPortText.trim().toIntOrNull()?.takeIf { it in PORTS }
            val controlPort = controlPortText.trim().toIntOrNull()?.takeIf { it in PORTS }
            if (host.isEmpty()) return reject("Enter the server IP address or host name")
            if (audioPort == null) return reject("Enter a valid UDP port between 1 and 65535")
            if (controlPort == null) return reject("Enter a valid WebSocket control port between 1 and 65535")

            dispatch(SessionStarted(ServerEndpoint(host, controlPort), audioPort))
            // What the engineer set belongs to the last session; the mix's numbers carry on.
            changeMix(MixEvent.SessionStarted)
            scanner.stop()
            running.set(true)
            control.controlState = ControlState.CONNECTING
            control.clearMixOwnership()
            // Every session starts on Auto: the phone keeps nothing of a buffer an earlier server set,
            // and a server that takes it from the desk sends it again on registering.
            playerMix.deskBufferMs = PlaybackLatency.DESK_BUFFER_AUTO
            // Nor does it keep a hold for delayed copies: a server that delays them says so again.
            playerMix.redundancyHoldPackets = 0
            // The name and the UDP port travel in the registration, so the server adds this musician
            // as a target with no address typed; the client id keeps their row when the IP changes.
            control.connect(host, controlPort, audioPort, _state.value.musicianName, clientId())
            sessionsLaunched++
            receiver.launch(host, audioPort)
            keepPlaying(ServerEndpoint(host, controlPort).label, sessionsLaunched)
        }
    }

    /**
     * Keeps the app playing in the background for [session], with [server] on its notification. A
     * phone that refuses leaves the session playing while the app is on screen, as before, and says
     * why in the diagnostics.
     */
    private fun keepPlaying(server: String, session: Long) {
        try {
            keepAlive.start(server) { disconnectFromNotification(session) }
        } catch (error: Exception) {
            dispatch(AudioError("Background playback unavailable: ${error.message ?: error.javaClass.simpleName}"))
        }
    }

    /** The notification's Disconnect for [session]: it ends that session only, never a later one. */
    private fun disconnectFromNotification(session: Long) {
        synchronized(sessionLock) {
            if (session == sessionsLaunched) disconnect()
        }
    }

    private fun reject(message: String) {
        dispatch(ConnectRejected(message))
    }

    /** Ends the session: the receiver stops, the control channel closes and discovery listens again. */
    override fun disconnect() {
        synchronized(sessionLock) {
            if (!running.getAndSet(false)) return
            // Ended before the receiver is asked to stop, so its stop report finds nothing left to end.
            lastEndedSession = sessionsLaunched
            receiver.stop()
            closeSession("stopped")
        }
    }

    /**
     * The receiver ended, on request or on its own, and reported it. Only the latest session's
     * report ends a session, and only once: a report for a session already ended (by Stop, or one
     * replaced by a newer session) does nothing, and after [onCleared] no report does anything.
     */
    private fun receiverStopped() {
        synchronized(sessionLock) {
            // The real receiver never reports more stops than launches; a stray report counts as
            // the latest session's, so the counts cannot drift apart.
            stopsReported = minOf(stopsReported + 1, sessionsLaunched)
            val session = stopsReported
            if (cleared || session != sessionsLaunched || session == lastEndedSession) return
            lastEndedSession = session
            closeSession("receiver stopped")
        }
    }

    /** The end of a session: the control channel closes and stops reconnecting, and discovery listens again. */
    private fun closeSession(reason: String) {
        keepAlive.stop()
        control.close(NORMAL_CLOSE, reason)
        control.controlState = ControlState.IDLE
        dispatch(SessionEnded)
        // A report arriving after this changes nothing.
        changeMix(MixEvent.SessionEnded)
        scanner.start()
    }

    private fun publishControlState() {
        dispatch(ControlChanged(control.controlState, control.mixAcknowledged))
    }

    // The mix. A change that changes nothing (a value already set, a channel that does not exist)
    // sends nothing, as a slider that did not move never did: the model says what to send.

    override fun setChannelLevel(channel: Int, level: Int) =
        changeMix(MixEvent.ChannelLevelChanged(channel.toLong(), level.toLong()))

    override fun setChannelPan(channel: Int, pan: Int) =
        changeMix(MixEvent.ChannelPanChanged(channel.toLong(), pan.toLong()))

    override fun setChannelMuted(channel: Int, muted: Boolean) =
        changeMix(MixEvent.ChannelMuteChanged(channel.toLong(), muted))

    override fun setChannelSoloed(channel: Int, soloed: Boolean) =
        changeMix(MixEvent.ChannelSoloChanged(channel.toLong(), soloed))

    override fun setGroupLevel(group: Int, level: Int) =
        changeMix(MixEvent.GroupLevelChanged(group.toLong(), level.toLong()))

    override fun setMasterVolume(level: Int) = changeMix(MixEvent.MasterVolumeChanged(level.toLong()))

    override fun setMasterMuted(muted: Boolean) = changeMix(MixEvent.MasterMuteChanged(muted))

    override fun setLevelCap(level: Int) = changeMix(MixEvent.LevelCapChanged(level.toLong()))

    /** Turns More of me on or off; it stays off, with a notice, until a channel is chosen. */
    override fun setMoreOfMe(on: Boolean) = changeMix(MixEvent.MoreOfMeToggled(on))

    /** Takes [channel] as the musician's, or none; the phone remembers it once the model takes it. */
    override fun setMusicianChannel(channel: Int?) = changeMix(MixEvent.MusicianChannelChosen(channel?.toLong()))

    /**
     * Hands [event] to the mix model, from the main thread (the musician) or the control thread (the
     * desk). [MixChanges] shows the state it left and queues the mix it numbered, in call order;
     * a musician channel the change moved (chosen, moved to its pair's first side, or forgotten by a
     * `config`) is remembered on the phone.
     */
    private fun changeMix(event: MixEvent) {
        mix.apply(event, alongside = ::rememberMusicianChannel)
    }

    private fun rememberMusicianChannel(update: MixUpdate) {
        if (update.musicianChannelChanged) store.setMusicianChannel(update.state.moreOfMe.musicianChannel?.toInt())
    }

    /**
     * Shows the mix model's [state] and copies its master values to the mix state the receive thread
     * reads: the lower of the musician's cap and the engineer's limit, as the server plays it. Called
     * by [MixChanges] for every change, in the order the changes landed, so the last copy always
     * matches the model.
     */
    private fun showMix(state: MixModelState) {
        _state.update { it.showing(state) }
        playerMix.volumePercent = state.masterVolume.toInt()
        playerMix.maxLevelPercent = state.effectiveCap.toInt()
        playerMix.muted = state.masterMuted
    }

    // Saved mixes. They hold the mix as the musician set it, never the More of me lift; the rules
    // are the model's, and the phone keeps the list it hands back.

    /**
     * Saves the current mix as [name], trimmed; a name already used is updated. Returns whether it
     * was saved: a blank name is refused with a notice, and the screen keeps what was typed.
     */
    override fun saveMix(name: String): Boolean =
        changeSavedMixes { it.saveMix(name.wellFormedForCore()) }.outcome is MixBankOutcome.Saved

    /**
     * Applies a saved mix and sends it, exactly as if the musician had moved every control. The
     * phone's store is read first, so a mix that is gone is reported and the list refreshed. While
     * the mix follows another musician's the recall is refused with a notice and nothing is sent.
     */
    override fun recallMix(name: String) {
        mix.recall(name, store.loadSavedMixes()) { recall -> dispatch(MixRecallTried(recall.outcome)) }
    }

    /** Renames a saved mix. A name another mix already has is refused, so no mix is overwritten. */
    override fun renameMix(from: String, to: String) {
        changeSavedMixes { it.renameMix(from, to.wellFormedForCore()) }
    }

    override fun deleteMix(name: String) {
        changeSavedMixes { it.deleteMix(name) }
    }

    /** A save, rename or delete through the model; the phone stores the list when it changed. */
    private fun changeSavedMixes(change: (MixModel) -> MixBankChange): MixBankChange =
        mix.changeSavedMixes(change, store = store::storeSavedMixes) { changed ->
            dispatch(MixBankChanged(changed.outcome))
        }

    // The rest.

    /**
     * Keeps the name as typed, remembers it and re-announces it on the open control channel. The
     * channel waits for a pause in typing before it sends, so a word is one message, not one per key.
     */
    override fun setName(name: String) {
        dispatch(NameChanged(name))
        store.setMusicianName(name)
        control.updateRegistrationName(name)
    }

    override fun selectTab(tab: MainTab) {
        dispatch(TabSelected(tab))
    }

    override fun onCleared() {
        synchronized(sessionLock) {
            disconnect()
            // A receiver report arriving from now on must not reach the channel or discovery,
            // which are shut down below.
            cleared = true
        }
        control.close(NORMAL_CLOSE, "destroyed")
        control.shutdown()
        scanner.shutdown()
    }

    /** Applies [event] atomically: [reduce] is pure, so a step that lost a race is simply taken again. */
    private fun dispatch(event: MonitorEvent) {
        _state.update { reduce(it, event) }
    }

    companion object {
        /** Builds the ViewModel on the real network, audio and storage of the application. */
        val Factory: ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val application = checkNotNull(this[ViewModelProvider.AndroidViewModelFactory.APPLICATION_KEY])
                MonitorViewModel(MonitorCollaborators.production(application))
            }
        }
    }
}
