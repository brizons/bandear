package com.bandear.android.ui.state

import android.content.Context
import com.bandear.android.ClientIdentity
import com.bandear.android.ClientMixState
import com.bandear.android.ControlScheduler
import com.bandear.android.ControlTransport
import com.bandear.android.ExecutorControlScheduler
import com.bandear.android.ForegroundKeepAlive
import com.bandear.android.LowLatencyWifiLock
import com.bandear.android.MixBankStore
import com.bandear.android.OkHttpControlTransport
import com.bandear.android.PmonPlayer
import com.bandear.android.ServerDiscovery
import com.bandear.android.core.SavedMix
import java.util.concurrent.atomic.AtomicBoolean

/*
 * The seams between [MonitorViewModel] and the classes that touch the network, the audio hardware
 * and the phone's storage. Production wraps the existing classes, unchanged; the JVM tests hand in
 * fakes. The control channel itself is not wrapped: it already takes its transport and scheduler as
 * seams, so the ViewModel builds the real channel in both.
 */

/** The audio receiver of a session; [PmonPlayer] in production. */
internal interface AudioReceiver {
    /** True while the previous session's receiver thread is still ending. */
    fun isAlive(): Boolean

    /** Starts receiving from [host] on the local UDP [port]. */
    fun launch(host: String, port: Int)

    /** Asks the receiver to end; it reports `onStopped` once it has. */
    fun stop()
}

/** Listens for server beacons while no session runs; [ServerDiscovery] in production. */
internal interface ServerScanner {
    fun start()

    /** Pauses listening; [start] resumes it. */
    fun stop()

    /** Stops listening for good. */
    fun shutdown()
}

/**
 * Keeps the app playing while a session runs, with the screen locked or another app in front;
 * [com.bandear.android.ForegroundKeepAlive] (a foreground service with an ongoing notification) in
 * production. Without it, Android freezes the app and mutes its audio seconds after it leaves the
 * screen.
 */
internal interface SessionKeepAlive {
    /**
     * A session with [server] (as the screens name it) started, from the musician's Connect;
     * [onDisconnect] is the notification's Disconnect. Throws when the phone does not allow it.
     */
    fun start(server: String, onDisconnect: () -> Unit)

    /** The session ended. */
    fun stop()
}

/** Saved mixes, the musician channel and the musician's name on this phone; [MixBankStore] in production. */
internal interface MonitorStore {
    /** The saved mixes, in the order they were saved. */
    fun loadSavedMixes(): List<SavedMix>

    /** Keeps [mixes] as the saved mixes; the shared core's model decides what they are. */
    fun storeSavedMixes(mixes: List<SavedMix>)

    fun musicianChannel(): Int?
    fun setMusicianChannel(channel: Int?)
    fun musicianName(): String
    fun setMusicianName(name: String)
}

/**
 * What [MonitorViewModel] is built from. The receiver and the scanner are made by factories because
 * they report to listeners the ViewModel owns; the receiver also shares the session flag and the
 * master values the audio thread reads.
 */
internal class MonitorCollaborators(
    val store: MonitorStore,
    /** This installation's client id; read when a session starts. */
    val clientId: () -> String,
    val receiverFactory: (PmonPlayer.Listener, AtomicBoolean, ClientMixState) -> AudioReceiver,
    val scannerFactory: (ServerDiscovery.Listener) -> ServerScanner,
    val controlTransport: ControlTransport,
    val controlScheduler: ControlScheduler,
    /** Started with each session the musician connects, stopped when it ends. */
    val keepAlive: SessionKeepAlive,
) {
    companion object {
        /** The real network, audio and storage, exactly as `MainActivity` builds them. */
        fun production(context: Context): MonitorCollaborators {
            val appContext = context.applicationContext
            val identity = ClientIdentity(appContext)
            val wifiLock = LowLatencyWifiLock(appContext)
            return MonitorCollaborators(
                store = BankStoreAdapter(MixBankStore(appContext)),
                clientId = { identity.clientId },
                receiverFactory = { listener, running, mixState ->
                    PlayerReceiver(PmonPlayer(listener, running, mixState, wifiLock::hold))
                },
                scannerFactory = { listener -> DiscoveryScanner(ServerDiscovery(listener)) },
                controlTransport = OkHttpControlTransport(),
                controlScheduler = ExecutorControlScheduler(),
                keepAlive = ForegroundKeepAlive(appContext),
            )
        }
    }
}

private class PlayerReceiver(private val player: PmonPlayer) : AudioReceiver {
    override fun isAlive(): Boolean = player.isReceiverAlive()

    override fun launch(host: String, port: Int) = player.launch(host, port)

    override fun stop() = player.stopReceiving()
}

private class DiscoveryScanner(private val discovery: ServerDiscovery) : ServerScanner {
    override fun start() = discovery.start()

    override fun stop() = discovery.stop()

    override fun shutdown() = discovery.shutdown()
}

private class BankStoreAdapter(private val store: MixBankStore) : MonitorStore {
    override fun loadSavedMixes(): List<SavedMix> = store.loadSavedMixes()

    override fun storeSavedMixes(mixes: List<SavedMix>) = store.storeSavedMixes(mixes)

    override fun musicianChannel(): Int? = store.musicianChannel()

    override fun setMusicianChannel(channel: Int?) = store.setMusicianChannel(channel)

    override fun musicianName(): String = store.musicianName()

    override fun setMusicianName(name: String) = store.setMusicianName(name)
}
