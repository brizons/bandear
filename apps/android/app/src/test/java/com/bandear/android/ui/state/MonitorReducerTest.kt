package com.bandear.android.ui.state

import com.bandear.android.ControlState
import com.bandear.android.LinkStatsReport
import com.bandear.android.core.MixBankOutcome
import com.bandear.android.core.RecallOutcome
import com.bandear.android.core.RecallRefusal
import com.bandear.android.ui.components.LinkHealth
import com.bandear.android.ui.components.MainTab
import com.bandear.android.ui.components.PillKind
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
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The pure reducer of everything but the mix, one behaviour per test; every state here is built only
 * from events. The mix's rules are the shared core's and are tested there (`crates/bandear-core/src/mix/`).
 */
class MonitorReducerTest {
    private val idle = MonitorUiState()
    private val server = ServerEndpoint("192.168.1.20", 50001)

    private fun MonitorUiState.after(vararg events: MonitorEvent): MonitorUiState =
        events.fold(this) { state, event -> reduce(state, event) }

    private fun running(): MonitorUiState = idle.after(SessionStarted(server, DEFAULT_AUDIO_PORT))

    private fun stats(packets: Long = 990, lost: Long = 10) = ReceiverStats(
        packets = packets,
        lost = lost,
        sampleRate = 48_000,
        packetsPerSecond = 100.0,
        interArrivalMs = 10.0,
        jitterMs = 0.4,
        discardedSamples = 2,
        packetsWithDiscardedSamples = 1,
        playbackBacklogMs = 12.5,
    )

    private fun report(received: Long = 100, underruns: Long = 0) =
        LinkStatsReport(intervalMs = 1_000, packetsReceived = received, packetsLost = 0, underruns = underruns, receiveTimeouts = 0)

    // The session and the control channel.

    @Test
    fun aStartedSessionShowsConnectingToItsServerAndLandsOnTheMix() {
        val state = idle.after(TabSelected(MainTab.SETTINGS), SessionStarted(server, DEFAULT_AUDIO_PORT))

        assertTrue(state.connection.sessionRunning)
        assertEquals(server, state.connection.server)
        assertEquals(PillKind.CONNECTING, state.connection.pill)
        assertEquals("connecting...", state.connection.statusDetail)
        assertFalse(state.connection.mixHeldByServer)
        assertEquals(MainTab.MIX, state.tab)
    }

    @Test
    fun aStartedSessionFillsTheManualFormLikeTappingAFoundServerDid() {
        val state = idle.after(SessionStarted(server, 50_010))

        assertEquals(ManualAddress("192.168.1.20", "50001", "50010"), state.connection.manual)
    }

    @Test
    fun aStartedSessionClearsTheLastSessionsCountersErrorsAndProblem() {
        val used = idle.after(
            ConnectRejected("Enter the server IP address or host name"),
            SessionStarted(server, DEFAULT_AUDIO_PORT),
            StatsUpdated(stats()),
            LinkStatsReported(report(underruns = 1)),
            AudioError("UDP socket error: closed"),
            ControlError("WebSocket rejected mix update"),
            SessionEnded,
        )

        val fresh = used.after(SessionStarted(server, DEFAULT_AUDIO_PORT))

        assertEquals(DiagnosticsUiState(), fresh.diagnostics)
        assertNull(fresh.connection.problem)
    }

    @Test
    fun theControlStateMapsToThePill() {
        val started = running()

        assertEquals(PillKind.CONNECTED, started.after(ControlChanged(ControlState.CONNECTED, false)).connection.pill)
        assertEquals(PillKind.CONNECTING, started.after(ControlChanged(ControlState.CONNECTING, false)).connection.pill)
        for (gone in listOf(ControlState.UNAVAILABLE, ControlState.CLOSED, ControlState.IDLE)) {
            assertEquals(gone.name, PillKind.NOT_CONNECTED, started.after(ControlChanged(gone, false)).connection.pill)
        }
    }

    @Test
    fun reconnectingMapsToTheReconnectingPillAndSaysWhoHoldsTheMix() {
        val held = running().after(
            ControlChanged(ControlState.CONNECTED, false),
            MixAcknowledged(ControlState.CONNECTED),
            ControlChanged(ControlState.RECONNECTING, true),
        )
        val local = running().after(ControlChanged(ControlState.RECONNECTING, false))

        assertEquals(PillKind.RECONNECTING, held.connection.pill)
        assertTrue(held.connection.reconnecting)
        assertTrue(held.connection.mixHeldByServer)
        assertEquals("reconnecting - mix held at the server", held.connection.statusDetail)
        assertEquals(PillKind.RECONNECTING, local.connection.pill)
        assertFalse(local.connection.mixHeldByServer)
        assertEquals("reconnecting - mixing on this device", local.connection.statusDetail)
    }

    @Test
    fun anAcknowledgedMixIsHeldByTheServer() {
        val state = running().after(ControlChanged(ControlState.CONNECTED, false), MixAcknowledged(ControlState.CONNECTED))

        assertEquals(PillKind.CONNECTED, state.connection.pill)
        assertTrue(state.connection.mixHeldByServer)
        assertFalse(state.connection.reconnecting)
        assertEquals("connected - mix applied on the server", state.connection.statusDetail)
    }

    @Test
    fun anEndedSessionIsNotConnectedAndForgetsItsServerAndLinkHealth() {
        val ended = running().after(
            ControlChanged(ControlState.CONNECTED, true),
            LinkStatsReported(report()),
            SessionEnded,
        )

        assertFalse(ended.connection.sessionRunning)
        assertNull(ended.connection.server)
        assertEquals(PillKind.NOT_CONNECTED, ended.connection.pill)
        assertEquals("not connected", ended.connection.statusDetail)
        assertEquals(LinkHealth.UNKNOWN, ended.diagnostics.linkHealth)
    }

    @Test
    fun aLateControlChangeCannotRaiseThePillOnceTheSessionEnded() {
        val ended = running().after(
            ControlChanged(ControlState.CONNECTED, false),
            SessionEnded,
            ControlChanged(ControlState.RECONNECTING, false),
        )

        assertEquals(PillKind.NOT_CONNECTED, ended.connection.pill)
        assertEquals("not connected", ended.connection.statusDetail)
    }

    @Test
    fun aLateAcknowledgementChangesNothingOnceTheSessionEnded() {
        val ended = running().after(ControlChanged(ControlState.CONNECTED, false), SessionEnded)

        assertEquals(ended, ended.after(MixAcknowledged(ControlState.CONNECTED)))
        assertEquals(ended, ended.after(MixAcknowledged(ControlState.IDLE)))
        assertEquals(idle, idle.after(MixAcknowledged(ControlState.CONNECTED)))
    }

    @Test
    fun anAcknowledgementInTheNextSessionIsHeldByTheServer() {
        val next = running().after(
            SessionEnded,
            SessionStarted(server, DEFAULT_AUDIO_PORT),
            ControlChanged(ControlState.CONNECTED, false),
            MixAcknowledged(ControlState.CONNECTED),
        )

        assertEquals(PillKind.CONNECTED, next.connection.pill)
        assertTrue(next.connection.mixHeldByServer)
        assertEquals("connected - mix applied on the server", next.connection.statusDetail)
    }

    @Test
    fun aRefusedConnectSaysWhyAndStartsNothing() {
        val message = "Enter a valid UDP port between 1 and 65535"

        val state = idle.after(ConnectRejected(message))

        assertEquals(message, state.connection.problem)
        assertEquals(message, state.diagnostics.lastError)
        assertFalse(state.connection.sessionRunning)
        assertEquals(PillKind.NOT_CONNECTED, state.connection.pill)
    }

    @Test
    fun theManualFormKeepsWhatWasTyped() {
        val typed = ManualAddress(host = "studio.local", controlPort = "6000", audioPort = "abc")

        assertEquals(typed, idle.after(ManualAddressEdited(typed)).connection.manual)
    }

    @Test
    fun foundServersAreListedAndALostOneDisappears() {
        val first = ServerEndpoint("10.0.0.2", 50001)
        val second = ServerEndpoint("10.0.0.3", 50001)

        val both = idle.after(ServersChanged(listOf(first, second)))
        val oneLost = both.after(ServersChanged(listOf(second)))

        assertEquals(listOf(first, second), both.servers)
        assertEquals(listOf(second), oneLost.servers)
        assertEquals("10.0.0.2:50001", first.label)
    }

    // Saved mixes. The list and the active mix are the shared core's; the words are the screen's.

    /** A state listing [names], as the mix model shows its saved mixes. */
    private fun listing(vararg names: String) = idle.copy(savedMixes = SavedMixesUiState(names = names.toList()))

    @Test
    fun theEmptyStateShowsWhenNothingIsSaved() {
        assertEquals("No saved mixes yet", idle.savedMixes.emptyMessage)
        assertEquals("No saved mixes yet", idle.after(MixBankChanged(MixBankOutcome.Deleted("Verse"))).savedMixes.emptyMessage)
    }

    @Test
    fun theEmptyStateNeverShowsWhileOneMixIsSaved() {
        val outcomes = listOf(
            MixBankChanged(MixBankOutcome.Saved("Verse")),
            MixBankChanged(MixBankOutcome.Deleted("Chorus")),
            MixBankChanged(MixBankOutcome.NameNeeded),
            MixBankChanged(MixBankOutcome.Renamed("Old", "Verse")),
            MixBankChanged(MixBankOutcome.Missing("Gone")),
            MixRecallTried(RecallOutcome.Recalled("Verse")),
        )

        for (outcome in outcomes) {
            val state = listing("Verse").after(outcome)
            assertNull("the empty state shows next to a saved mix", state.savedMixes.emptyMessage)
            assertEquals(listOf("Verse"), state.savedMixes.mixes.map { it.name })
        }
    }

    @Test
    fun eachSavedMixActionSaysWhatItDid() {
        fun notice(outcome: MixBankOutcome) = listing("Verse").after(MixBankChanged(outcome)).savedMixes.notice

        assertEquals("Saved \"Verse\"", notice(MixBankOutcome.Saved("Verse")))
        assertEquals("Renamed \"Verse\" to \"Bridge\"", notice(MixBankOutcome.Renamed("Verse", "Bridge")))
        assertEquals("Deleted \"Verse\"", notice(MixBankOutcome.Deleted("Verse")))
        assertEquals("Enter a name for the mix", notice(MixBankOutcome.NameNeeded))
        assertEquals("\"Verse\" is no longer saved", notice(MixBankOutcome.Missing("Verse")))
        assertEquals("A mix named \"Chorus\" already exists", notice(MixBankOutcome.NameTaken("Chorus")))
    }

    @Test
    fun aRenameToTheSameNameSaysNothingAndKeepsTheLastNotice() {
        val saved = listing("Verse").after(MixBankChanged(MixBankOutcome.Saved("Verse")))

        assertEquals(saved, saved.after(MixBankChanged(MixBankOutcome.Unchanged)))
    }

    @Test
    fun aRecallSaysWhatItDidAndWhyItWasRefused() {
        fun notice(outcome: RecallOutcome) = listing("Verse").after(MixRecallTried(outcome)).savedMixes.notice

        assertEquals("Recalled \"Verse\"", notice(RecallOutcome.Recalled("Verse")))
        assertEquals("\"Verse\" is no longer saved", notice(RecallOutcome.Missing("Verse")))
        assertEquals(
            "\"Verse\" was not recalled: your mix follows Ana's",
            notice(RecallOutcome.Refused("Verse", RecallRefusal.Following(" Ana "))),
        )
        assertEquals(
            "\"Verse\" was not recalled: your mix follows another musician's",
            notice(RecallOutcome.Refused("Verse", RecallRefusal.Following(""))),
        )
    }

    // Diagnostics.

    @Test
    fun statsFillTheDiagnostics() {
        val diagnostics = running().after(StatsUpdated(stats(packets = 990, lost = 10))).diagnostics

        assertEquals(990L, diagnostics.packetsReceived)
        assertEquals(10L, diagnostics.packetsLost)
        assertEquals(1.0, diagnostics.lossPercent, 1e-9)
        assertEquals(48_000, diagnostics.sampleRateHz)
        assertEquals(100.0, diagnostics.packetsPerSecond, 0.0)
        assertEquals(10.0, diagnostics.interArrivalMs, 0.0)
        assertEquals(0.4, diagnostics.jitterMs, 0.0)
        assertEquals(12.5, diagnostics.playbackBufferMs, 0.0)
        assertEquals(2L, diagnostics.discardedSamples)
        assertEquals(1L, diagnostics.packetsWithDiscardedSamples)
    }

    @Test
    fun lossIsZeroBeforeAnyPacket() {
        assertEquals(0.0, idle.diagnostics.lossPercent, 0.0)
    }

    @Test
    fun linkReportsSetTheLinkHealthAndAddUpUnderruns() {
        val warned = running().after(LinkStatsReported(report(underruns = 1)))
        val recovered = warned.after(LinkStatsReported(report(underruns = 0)))
        val silent = recovered.after(LinkStatsReported(report(received = 0)))

        assertEquals(LinkHealth.WARNING, warned.diagnostics.linkHealth)
        assertEquals(LinkHealth.GOOD, recovered.diagnostics.linkHealth)
        assertEquals(1L, recovered.diagnostics.underruns)
        assertEquals(LinkHealth.SILENT, silent.diagnostics.linkHealth)
    }

    @Test
    fun errorsAreKeptPerSource() {
        val state = running().after(AudioError("UDP socket error: closed"), ControlError("WebSocket rejected mix update"))

        assertEquals("UDP socket error: closed", state.diagnostics.lastError)
        assertEquals("WebSocket rejected mix update", state.diagnostics.controlError)
    }

    // The rest.

    @Test
    fun theNameIsKeptAsTyped() {
        assertEquals(" Ana ", idle.after(NameChanged(" Ana ")).musicianName)
    }

    @Test
    fun selectingATabShowsIt() {
        assertEquals(MainTab.MIXES, idle.after(TabSelected(MainTab.MIXES)).tab)
    }
}
