package com.bandear.android.ui.state

import com.bandear.android.ControlState
import com.bandear.android.LinkStatsReport
import com.bandear.android.core.MixBankOutcome
import com.bandear.android.core.RecallOutcome
import com.bandear.android.ui.components.MainTab

/** The receiver's running metrics, as one value; see `PmonPlayer.Listener.onStats`. */
internal data class ReceiverStats(
    val packets: Long,
    val lost: Long,
    val sampleRate: Int?,
    val packetsPerSecond: Double,
    val interArrivalMs: Double,
    val jitterMs: Double,
    val discardedSamples: Long,
    val packetsWithDiscardedSamples: Long,
    val playbackBacklogMs: Double,
)

/**
 * Everything the screens react to apart from the mix, whether the musician did it or a collaborator
 * reported it.
 *
 * Each event is a fact that already happened. [reduce] turns a state and an event into the next
 * state and does nothing else; the side effects belong to the ViewModel. The mix itself is the
 * shared core's (`MixModel`): its changes go through the model, and the screens show the state it
 * returns ([showing]).
 */
internal sealed interface MonitorEvent {
    // The session and the control channel.

    /** A session started to [server], listening for audio on [audioPort]. */
    data class SessionStarted(val server: ServerEndpoint, val audioPort: Int) : MonitorEvent

    /** The session ended: the musician disconnected or the receiver stopped on its own. */
    data object SessionEnded : MonitorEvent

    /** Connect was refused before anything started, for the reason in [message]. */
    data class ConnectRejected(val message: String) : MonitorEvent

    /** The control channel is now in [state]; [mixAcknowledged] is whether the server took the mix. */
    data class ControlChanged(val state: ControlState, val mixAcknowledged: Boolean) : MonitorEvent

    /** The server acknowledged a mix while the control channel was in [state]. */
    data class MixAcknowledged(val state: ControlState) : MonitorEvent

    /** Discovery now hears [servers]; a server missing from the list was lost. */
    data class ServersChanged(val servers: List<ServerEndpoint>) : MonitorEvent

    /** The manual connection form now reads [address]. */
    data class ManualAddressEdited(val address: ManualAddress) : MonitorEvent

    // Saved mixes. The list and the active mix are the model's; these say what an action did.

    /** A save, rename or delete of a saved mix did [outcome]. */
    data class MixBankChanged(val outcome: MixBankOutcome) : MonitorEvent

    /** A recall of a saved mix did [outcome]. */
    data class MixRecallTried(val outcome: RecallOutcome) : MonitorEvent

    // Diagnostics.

    data class StatsUpdated(val stats: ReceiverStats) : MonitorEvent

    /** The receiver produced its once-a-second link report. */
    data class LinkStatsReported(val report: LinkStatsReport) : MonitorEvent

    /** The receiver reported a problem. */
    data class AudioError(val message: String) : MonitorEvent

    /** The control channel reported a problem. */
    data class ControlError(val message: String) : MonitorEvent

    // The rest.

    data class NameChanged(val name: String) : MonitorEvent
    data class TabSelected(val tab: MainTab) : MonitorEvent
}
