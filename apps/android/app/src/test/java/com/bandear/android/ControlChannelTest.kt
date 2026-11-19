package com.bandear.android

import com.bandear.android.core.ChannelKind
import com.bandear.android.core.DeskConfiguration
import com.bandear.android.core.MixCommand
import com.bandear.android.core.MixEvent
import com.bandear.android.core.MixReport
import com.bandear.android.core.SourceChannel
import com.bandear.android.ui.state.FakeControlScheduler
import com.bandear.android.ui.state.FakeSocket
import com.bandear.android.ui.state.FakeTransport
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.concurrent.atomic.AtomicBoolean

/**
 * The control channel's wiring to the shared core: the desk's messages are read by the core
 * (`parseServerMessage`) and handed on, what they report about the mix as the core maps it
 * (`mixEventReportedBy`), and the phone's `mix` and `stats` go out as the core writes them. The
 * reading and writing rules themselves are tested in the core (`crates/bandear-core/src/`).
 */
class ControlChannelTest {
    private val events = RecordingListener()
    private val transport = FakeTransport()

    /** The mix the phone opens with, as the ViewModel would hand it to the channel. */
    private val openingMix = MixCommand(sequence = 12uL, volumePercent = 100, maxLevelPercent = 100, muted = false)
    private val channel: ControlChannel = ControlChannel(
        listener = events,
        running = AtomicBoolean(true),
        sendOpeningMix = { channel.sendMix(openingMix) },
        transport = transport,
        scheduler = FakeControlScheduler(),
    )

    /** Opens the channel and lets the server announce itself, as it does right after connect. */
    private fun openChannel(): FakeSocket {
        channel.connect("192.168.1.20", 50001, 50000, "Ana")
        return transport.sockets.single().also { socket ->
            socket.open()
            socket.receive("""{"type":"config","source_channels":2,"groups":[]}""")
        }
    }

    /** The one mix report handed on, pushed or answering. */
    private fun reported(): MixReport = when (val event = events.reported.single()) {
        is MixEvent.MixPushed -> event.v1
        is MixEvent.MixAnswered -> checkNotNull(event.report)
        else -> error("not a report: $event")
    }

    /** A mix report as the server writes it: its full 32-channel and 16-group tables, and [seq] if any. */
    private fun mixReport(type: String, firstLevel: Int = 10, seq: Long? = null): String = JSONObject()
        .put("type", type)
        .put("seq", seq)
        .put("volume_percent", 80)
        .put("max_level_percent", 90)
        .put("muted", true)
        .put("channels", JSONArray(List(32) { firstLevel + it }))
        .put("pans", JSONArray(List(32) { if (it == 1) 25L else defaultPan(it) }))
        .put("mutes", JSONArray(List(32) { it == 2 }))
        .put("solos", JSONArray(List(32) { it == 3 }))
        .put("group_levels", JSONArray(List(16) { 50 + it }))
        .toString()

    @Test
    fun aPushedMixIsReadWholeAndHandedOnWithNothingSentBack() {
        val socket = openChannel()
        val sent = socket.sent.size

        socket.receive(mixReport("mix_state"))

        assertTrue(events.reported.single() is MixEvent.MixPushed)
        val mix = reported()
        assertEquals(List(32) { 10L + it }, mix.channelGains)
        assertEquals(List(32) { if (it == 1) 25L else defaultPan(it) }, mix.pans)
        assertEquals(List(32) { it == 2 }, mix.channelMutes)
        assertEquals(List(32) { it == 3 }, mix.channelSolos)
        assertEquals(List(16) { 50L + it }, mix.groupLevels)
        assertEquals(80L, mix.volumePercent)
        assertEquals(90L, mix.maxLevelPercent)
        assertTrue(mix.muted)
        assertEquals(sent, socket.sent.size)
        assertTrue(events.errors.isEmpty())
    }

    @Test
    fun unknownTypesAreIgnored() {
        val socket = openChannel()

        socket.receive("""{"type":"mix_preview","channels":[1,2]}""")

        assertTrue(events.reported.isEmpty())
        assertTrue(events.errors.isEmpty())
    }

    @Test
    fun aMessageTheCoreCannotReadIsAnInvalidResponseAndNeverAConnectionFailure() {
        val socket = openChannel()

        socket.receive("not json")
        socket.receive("""{"type":"mix_state","volume_percent":"80"}""")

        assertEquals(List(2) { "Invalid control response: not a control message this phone can read" }, events.errors)
        assertTrue(events.reported.isEmpty())
        assertEquals(ControlState.CONNECTED, channel.controlState)
        assertEquals(0, events.closed)
        assertEquals("the socket stays open and nothing reconnects", 1, transport.sockets.size)
    }

    @Test
    fun aPushMissingPartOfTheMixIsReportedAndNotHandedOn() {
        val socket = openChannel()

        socket.receive("""{"type":"mix_state","volume_percent":80,"channels":[100,100]}""")

        assertTrue(events.reported.isEmpty())
        assertTrue(events.errors.single(), events.errors.single().startsWith("Invalid control response"))
    }

    // The playback buffer the desk sets.

    @Test
    fun aPlaybackMessageHandsOnTheDesksBufferWithNothingSentBack() {
        val socket = openChannel()
        val sent = socket.sent.size

        socket.receive("""{"type":"playback","buffer":{"mode":"fixed","ms":40}}""")
        socket.receive("""{"type":"playback","buffer":{"mode":"auto"}}""")
        socket.receive("""{"type":"playback","buffer":{"mode":"fixed","ms":5,"floor":2},"later":true}""")

        assertEquals(listOf(40, PlaybackLatency.DESK_BUFFER_AUTO, 5), events.buffers)
        assertEquals(sent, socket.sent.size)
        assertTrue(events.errors.isEmpty())
    }

    // The hold the desk's delayed redundant copies need.

    @Test
    fun aRedundancyMessageHandsOnTheHoldWithNothingSentBack() {
        val socket = openChannel()
        val sent = socket.sent.size

        socket.receive("""{"type":"redundancy","delay_blocks":2}""")
        socket.receive("""{"type":"redundancy","delay_blocks":0}""")
        socket.receive("""{"type":"redundancy","delay_blocks":4,"later":true}""")

        assertEquals(listOf(2, 0, 4), events.holds)
        assertEquals(sent, socket.sent.size)
        assertTrue(events.errors.isEmpty())
    }

    @Test
    fun aRedundancyMessageThePhoneCannotUseIsIgnored() {
        val socket = openChannel()

        for (unusable in listOf(
            """{"type":"redundancy"}""",
            """{"type":"redundancy","delay_blocks":-1}""",
            """{"type":"redundancy","delay_blocks":5}""",
        )) {
            socket.receive(unusable)
        }

        assertTrue(events.holds.toString(), events.holds.isEmpty())
        assertTrue(events.errors.isEmpty())
    }

    @Test
    fun aRedundancyMessageWithAValueOfTheWrongTypeIsIgnoredAsAnInvalidResponse() {
        val socket = openChannel()

        socket.receive("""{"type":"redundancy","delay_blocks":1.5}""")
        socket.receive("""{"type":"redundancy","delay_blocks":"2"}""")

        assertTrue(events.holds.toString(), events.holds.isEmpty())
        assertEquals(2, events.errors.count { it.startsWith("Invalid control response") })
    }

    @Test
    fun aPlaybackMessageThePhoneCannotUseIsIgnored() {
        val socket = openChannel()

        for (unusable in listOf(
            """{"type":"playback"}""",
            """{"type":"playback","buffer":{"mode":"elastic","ms":40}}""",
            """{"type":"playback","buffer":{"mode":"fixed"}}""",
            """{"type":"playback","buffer":{"mode":"fixed","ms":4}}""",
            """{"type":"playback","buffer":{"mode":"fixed","ms":501}}""",
        )) {
            socket.receive(unusable)
        }

        assertTrue(events.buffers.toString(), events.buffers.isEmpty())
        assertTrue(events.errors.isEmpty())
    }

    @Test
    fun aPlaybackMessageWithAValueOfTheWrongTypeIsIgnoredAsAnInvalidResponse() {
        val socket = openChannel()

        for (mistyped in listOf(
            """{"type":"playback","buffer":"auto"}""",
            """{"type":"playback","buffer":{"mode":"fixed","ms":40.5}}""",
            """{"type":"playback","buffer":{"mode":"fixed","ms":"40"}}""",
        )) {
            socket.receive(mistyped)
        }

        assertTrue(events.buffers.toString(), events.buffers.isEmpty())
        assertEquals(3, events.errors.count { it.startsWith("Invalid control response") })
    }

    // The channel details a `config` carries.

    @Test
    fun aConfigHandsOnTheChannelDetailsInChannelOrder() {
        val socket = openChannel()

        socket.receive(
            """{"type":"config","source_channels":4,"groups":[],"channels":[
                {"name":"Vox","kind":"voice","hidden":false,"link":false},
                {"name":"Keys","kind":"keys","hidden":false,"link":true},
                {"name":"","kind":"keys","hidden":false,"link":false},
                {"name":"Spare","kind":"other","hidden":true,"link":false}]}""",
        )

        assertEquals(4, events.configs.last().sourceChannels.toInt())
        assertEquals(
            listOf(
                SourceChannel("Vox", ChannelKind.VOICE, isHidden = false, isLinkedToNext = false),
                SourceChannel("Keys", ChannelKind.KEYS, isHidden = false, isLinkedToNext = true),
                SourceChannel("", ChannelKind.KEYS, isHidden = false, isLinkedToNext = false),
                SourceChannel("Spare", ChannelKind.OTHER, isHidden = true, isLinkedToNext = false),
            ),
            events.configs.last().channels,
        )
    }

    @Test
    fun aConfigFromAnOlderServerHandsOnNoDetails() {
        openChannel()

        // The opening config of [openChannel] has no `channels`, as an older server's.
        assertEquals(listOf<List<SourceChannel>?>(null), events.configs.map { it.channels })
    }

    // The engineer's limit and locks a mix report carries.

    /** [report] with the engineer's fields set to [limit] and [locked], as a server since T5a writes them. */
    private fun withEngineer(report: String, limit: Any?, locked: Any?): String =
        JSONObject(report).put("engineer_limit_percent", limit).put("locked", locked).toString()

    @Test
    fun aPushAndAnAckCarryTheEngineersLimitAndLocks() {
        val socket = openChannel()

        socket.receive(withEngineer(mixReport("mix_state"), 60, JSONArray(List(32) { it == 0 || it == 3 })))
        socket.receive(withEngineer(mixReport("mix_ack", seq = 12), 75, JSONArray(List(32) { it == 1 })))

        val (pushed, answered) = events.reported
        val push = (pushed as MixEvent.MixPushed).v1
        val ack = checkNotNull((answered as MixEvent.MixAnswered).report)
        assertEquals(60L, push.engineerLimitPercent)
        assertEquals(List(32) { it == 0 || it == 3 }, push.locked)
        assertEquals(12uL, answered.sequence)
        assertEquals(75L, ack.engineerLimitPercent)
        assertEquals(List(32) { it == 1 }, ack.locked)
        assertEquals(10L, push.channelGains[0])
    }

    @Test
    fun aReportFromAnOlderServerHasNoLimitAndNothingLocked() {
        val socket = openChannel()

        socket.receive(mixReport("mix_state"))

        assertEquals(100L, reported().engineerLimitPercent)
        assertEquals(List(32) { false }, reported().locked)
        assertEquals(null, reported().following)
    }

    @Test
    fun aReportSaysWhoseMixThePhoneFollows() {
        val socket = openChannel()

        socket.receive(JSONObject(mixReport("mix_state")).put("following", "Ana").toString())
        socket.receive(JSONObject(mixReport("mix_ack", seq = 12)).put("following", JSONObject.NULL).toString())

        assertEquals(
            listOf("Ana", null),
            events.reported.map { event ->
                when (event) {
                    is MixEvent.MixPushed -> event.v1.following
                    is MixEvent.MixAnswered -> event.report?.following
                    else -> error("not a report: $event")
                }
            },
        )
    }

    // The numbers that match the server's answers to the phone's mixes.

    @Test
    fun everyOpenSendsTheOpeningMixAndAMixGoesOutUnderTheNumberItWasHanded() {
        val socket = openChannel()

        channel.sendMix(openingMix.copy(sequence = null))

        val (opening, unnumbered) = socket.sentOfType("mix")
        assertEquals(listOf("register", "mix", "mix"), socket.sent.map { JSONObject(it).getString("type") })
        assertEquals(12L, opening.getLong("seq"))
        assertFalse(unnumbered.has("seq"))
    }

    @Test
    fun aNumberedAckAnswersItsMixWithTheMixTheServerPlays() {
        val socket = openChannel()

        socket.receive(mixReport("mix_ack", firstLevel = 40, seq = 12))

        val answer = events.reported.single() as MixEvent.MixAnswered
        assertEquals(12uL, answer.sequence)
        assertEquals(40L, answer.report?.channelGains?.get(0))
        assertEquals(1, events.acknowledged)
        assertTrue(channel.mixAcknowledged)
    }

    @Test
    fun anAckWithoutANumberOnlyAcknowledges() {
        val socket = openChannel()

        socket.receive(mixReport("mix_ack", firstLevel = 40))

        assertTrue(events.reported.isEmpty())
        assertEquals(1, events.acknowledged)
    }

    @Test
    fun theMixGoesOutAsTheCoreWritesIt() {
        val socket = openChannel()

        channel.sendMix(
            MixCommand(
                sequence = 12uL,
                volumePercent = 90,
                maxLevelPercent = 95,
                muted = false,
                channelGains = listOf(80, 20),
                pans = listOf(0, 100),
                channelMutes = listOf(false, true),
                channelSolos = listOf(true, false),
                groupLevels = listOf(70),
            ),
        )

        assertEquals(
            """{"type":"mix","seq":12,"volume_percent":90,"max_level_percent":95,"muted":false,""" +
                """"channels":[80,20],"pans":[0,100],"mutes":[false,true],"solos":[true,false],"group_levels":[70]}""",
            socket.sent.last(),
        )
    }

    @Test
    fun anAckWithoutANumberWhoseMixCannotBeReadStillAcknowledges() {
        val socket = openChannel()

        socket.receive("""{"type":"mix_ack","channels":[100]}""")

        assertTrue(events.reported.isEmpty())
        assertEquals(1, events.acknowledged)
        assertTrue(channel.mixAcknowledged)
        assertTrue(events.errors.isEmpty())
    }

    @Test
    fun aNumberedErrorAnswersItsMixWithNothingToShow() {
        val socket = openChannel()

        socket.receive("""{"type":"error","message":"too many channels","seq":12}""")

        assertEquals(listOf(MixEvent.MixAnswered(12uL, null)), events.reported)
        assertEquals(listOf("too many channels"), events.errors)
    }

    @Test
    fun anErrorWithoutANumberNeverTouchesTheMixes() {
        val socket = openChannel()

        socket.receive("""{"type":"error","message":"no UDP target configured for client IP"}""")

        assertTrue(events.reported.isEmpty())
        assertEquals(listOf("no UDP target configured for client IP"), events.errors)
    }

    @Test
    fun aNumberedAckWhoseMixCannotBeReadStillAnswersItsNumber() {
        val socket = openChannel()

        socket.receive("""{"type":"mix_ack","seq":12,"channels":[100]}""")

        assertEquals(listOf(MixEvent.MixAnswered(12uL, null)), events.reported)
        assertEquals(1, events.acknowledged)
        assertTrue(events.errors.isEmpty())
    }

    @Test
    fun aLinkReportGoesOutAsTheCoresStatsMessage() {
        val socket = openChannel()

        channel.sendLinkStats(
            LinkStatsReport(
                intervalMs = 1_000,
                packetsReceived = 200,
                packetsLost = 3,
                underruns = 1,
                receiveTimeouts = 2,
                jitterMs = 1.2345,
                cushionMs = Double.NaN,
                backlogMs = 37.5,
                recovered = 4,
            ),
        )

        // A value not measured (NaN) is left out, as the core leaves out one it is not given.
        assertEquals(
            """{"type":"stats","interval_ms":1000,"packets_received":200,"packets_lost":3,"underruns":1,""" +
                """"receive_timeouts":2,"jitter_ms":1.23,"backlog_ms":37.5,"recovered":4}""",
            socket.sent.last(),
        )
    }

    private class RecordingListener : ControlChannel.Listener {
        /** What the desk's messages reported about the mix, in order. */
        val reported = mutableListOf<MixEvent>()
        val errors = mutableListOf<String>()
        val configs = mutableListOf<DeskConfiguration>()
        val buffers = mutableListOf<Int>()
        val holds = mutableListOf<Int>()
        var acknowledged = 0
        var closed = 0

        override fun onOpened() = Unit

        override fun onConfig(configuration: DeskConfiguration) {
            configs += configuration
        }

        override fun onAcknowledged() {
            acknowledged++
        }

        override fun onMixReported(event: MixEvent) {
            reported += event
        }

        override fun onPlaybackBuffer(fixedMs: Int) {
            buffers += fixedMs
        }

        override fun onRedundancyHold(holdPackets: Int) {
            holds += holdPackets
        }

        override fun onClosed() {
            closed++
        }

        override fun onControlError(message: String) {
            errors += message
        }
    }

    private companion object {
        /** The pan a channel opens at: even channels left, odd ones right. */
        fun defaultPan(channel: Int): Long = if (channel % 2 == 0) 0L else 100L
    }
}
