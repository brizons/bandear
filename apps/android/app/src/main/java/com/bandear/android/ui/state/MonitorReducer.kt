package com.bandear.android.ui.state

import com.bandear.android.ControlState
import com.bandear.android.controlStatusLabel
import com.bandear.android.core.MixBankOutcome
import com.bandear.android.core.RecallOutcome
import com.bandear.android.core.RecallRefusal
import com.bandear.android.ui.components.LinkHealth
import com.bandear.android.ui.components.MainTab
import com.bandear.android.ui.components.pillKindFor
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

/**
 * Returns the state that follows [state] once [event] happened. Pure: no IO, no clock, no threads,
 * so the screens' logic is an ordinary JVM test.
 *
 * The mix is not reduced here: the shared core's `MixModel` owns it, and the ViewModel shows the
 * state each change returns ([showing]). What stays here is the session, the pill, the servers, the
 * manual form, the diagnostics, the name, the tab, and the words for what a saved-mix action did.
 */
internal fun reduce(state: MonitorUiState, event: MonitorEvent): MonitorUiState = when (event) {
    is SessionStarted -> state.copy(
        connection = state.connection.copy(
            sessionRunning = true,
            server = event.server,
            problem = null,
            manual = ManualAddress(
                host = event.server.address,
                controlPort = event.server.controlPort.toString(),
                audioPort = event.audioPort.toString(),
            ),
        ).showing(ControlState.CONNECTING, mixAcknowledged = false),
        // Counters and errors describe one session, so a new one starts them again.
        diagnostics = DiagnosticsUiState(),
        tab = MainTab.MIX,
    )
    SessionEnded -> state.copy(
        connection = state.connection.copy(sessionRunning = false, server = null)
            .showing(ControlState.IDLE, state.connection.mixHeldByServer),
        diagnostics = state.diagnostics.copy(linkHealth = LinkHealth.UNKNOWN),
    )
    is ConnectRejected -> state.copy(
        connection = state.connection.copy(problem = event.message),
        diagnostics = state.diagnostics.copy(lastError = event.message),
    )
    is ControlChanged ->
        // Once the session ended only "idle" is true: a late callback cannot raise the pill again.
        if (!state.connection.sessionRunning && event.state != ControlState.IDLE) {
            state
        } else {
            state.copy(connection = state.connection.showing(event.state, event.mixAcknowledged))
        }
    is MixAcknowledged ->
        // An acknowledgement belongs to a session too: a late one cannot mark the mix held while idle.
        if (!state.connection.sessionRunning) {
            state
        } else {
            state.copy(connection = state.connection.showing(event.state, mixAcknowledged = true))
        }
    is ServersChanged -> state.copy(servers = event.servers)
    is ManualAddressEdited -> state.copy(connection = state.connection.copy(manual = event.address))

    // A rename to the name the mix already has says nothing, and leaves the last notice.
    is MixBankChanged -> state.copy(
        savedMixes = state.savedMixes.copy(notice = bankNotice(event.outcome) ?: state.savedMixes.notice),
    )
    is MixRecallTried -> state.copy(savedMixes = state.savedMixes.copy(notice = recallNotice(event.outcome)))

    is StatsUpdated -> state.copy(
        diagnostics = state.diagnostics.copy(
            packetsReceived = event.stats.packets,
            packetsLost = event.stats.lost,
            sampleRateHz = event.stats.sampleRate,
            packetsPerSecond = event.stats.packetsPerSecond,
            interArrivalMs = event.stats.interArrivalMs,
            jitterMs = event.stats.jitterMs,
            discardedSamples = event.stats.discardedSamples,
            packetsWithDiscardedSamples = event.stats.packetsWithDiscardedSamples,
            playbackBufferMs = event.stats.playbackBacklogMs,
        ),
    )
    is LinkStatsReported -> state.copy(
        diagnostics = state.diagnostics.copy(
            linkHealth = phoneLinkHealth(event.report),
            underruns = state.diagnostics.underruns + event.report.underruns,
        ),
    )
    is AudioError -> state.copy(diagnostics = state.diagnostics.copy(lastError = event.message))
    is ControlError -> state.copy(diagnostics = state.diagnostics.copy(controlError = event.message))

    is NameChanged -> state.copy(musicianName = event.name)
    is TabSelected -> state.copy(tab = event.tab)
}

/** The connection showing a control channel in [controlState], with the mix held where [mixAcknowledged] says. */
private fun ConnectionUiState.showing(controlState: ControlState, mixAcknowledged: Boolean) = copy(
    pill = pillKindFor(controlState),
    mixHeldByServer = mixAcknowledged,
    statusDetail = controlStatusLabel(controlState, mixAcknowledged),
)

/** What the Saved mixes screen says a save, rename or delete did; null when there is nothing to say. */
private fun bankNotice(outcome: MixBankOutcome): String? = when (outcome) {
    is MixBankOutcome.Saved -> "Saved \"${outcome.name}\""
    is MixBankOutcome.Renamed -> "Renamed \"${outcome.from}\" to \"${outcome.to}\""
    is MixBankOutcome.Deleted -> "Deleted \"${outcome.name}\""
    MixBankOutcome.Unchanged -> null
    MixBankOutcome.NameNeeded -> "Enter a name for the mix"
    is MixBankOutcome.Missing -> missingNotice(outcome.name)
    is MixBankOutcome.NameTaken -> "A mix named \"${outcome.name}\" already exists"
}

/**
 * What the Saved mixes screen says a recall did. While the mix follows another's the server keeps
 * every channel and group from it, so a saved mix is not recalled at all, and the notice says why.
 */
private fun recallNotice(outcome: RecallOutcome): String = when (outcome) {
    is RecallOutcome.Recalled -> "Recalled \"${outcome.name}\""
    is RecallOutcome.Refused -> when (val refusal = outcome.refusal) {
        is RecallRefusal.Following ->
            "\"${outcome.name}\" was not recalled: your mix follows ${leaderName(refusal.leader)}'s"
    }
    is RecallOutcome.Missing -> missingNotice(outcome.name)
}

private fun missingNotice(name: String): String = "\"$name\" is no longer saved"
