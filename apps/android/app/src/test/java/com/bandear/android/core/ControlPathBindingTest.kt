package com.bandear.android.core

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Calls the shared control path through the generated bindings: a desk message read, the phone's
 * messages written, the reconnect policy stepped. The rules are tested in Rust
 * (`crates/bandear-core/src/control/`); these tests prove each call and type crosses the binding and
 * reads in Kotlin as the control channel expects.
 */
class ControlPathBindingTest {
    @Test
    fun aDeskMessageIsReadIntoKotlinValues() {
        val message = parseServerMessage(
            """{"type":"config","source_channels":2,"groups":[{"name":"Drums","channels":[0,1]}],""" +
                """"channels":[{"name":"Lead","kind":"voice","hidden":false,"link":true},"junk"]}""",
        )

        assertEquals(
            ServerMessage.Configuration(
                DeskConfiguration(
                    sourceChannels = 2u,
                    sampleRate = null,
                    groups = listOf(ChannelGroup(id = null, name = "Drums", channels = listOf(0L, 1L))),
                    channels = listOf(
                        SourceChannel(name = "Lead", kind = ChannelKind.VOICE, isHidden = false, isLinkedToNext = true),
                        SourceChannel(name = "", kind = ChannelKind.OTHER, isHidden = false, isLinkedToNext = false),
                    ),
                ),
            ),
            message,
        )
        assertEquals(
            ServerMessage.MixAck(
                MixReport(
                    sequence = 7uL,
                    volumePercent = 80L,
                    maxLevelPercent = 95L,
                    muted = false,
                    channelGains = listOf(10L),
                    pans = listOf(50L),
                    channelMutes = listOf(false),
                    channelSolos = listOf(true),
                    groupLevels = emptyList(),
                    engineerLimitPercent = 100L,
                    locked = listOf(false),
                    following = "Ana",
                ),
            ),
            parseServerMessage(
                """{"type":"mix_ack","seq":7,"volume_percent":80,"max_level_percent":95,"muted":false,""" +
                    """"channels":[10],"pans":[50],"mutes":[false],"solos":[true],"group_levels":[],"following":"Ana"}""",
            ),
        )
        assertEquals(
            ServerMessage.PlaybackBuffer(PlaybackBufferSetting.Fixed(milliseconds = 40u)),
            parseServerMessage("""{"type":"playback","buffer":{"mode":"fixed","ms":40}}"""),
        )
    }

    @Test
    fun anUnreadableDeskMessageThrowsWhy() {
        assertThrows(ServerMessageException.NotJson::class.java) { parseServerMessage("not json") }
        val missing = assertThrows(ServerMessageException.MissingField::class.java) {
            parseServerMessage("""{"type":"playback"}""")
        }
        assertEquals("buffer", missing.field)
        assertEquals("playback", missing.messageType)
    }

    @Test
    fun thePhonesMessagesAreWrittenAsTheDeskReadsThem() {
        Registration("  Ana  ", 50_000u, "phone-1", allClientCapabilities()).use { registration ->
            assertEquals("Ana", registration.name())
            assertEquals(
                """{"type":"register","name":"Ana","udp_port":50000,"client_id":"phone-1",""" +
                    """"capabilities":["stats.v2","playback.buffer","redundancy.recovered","redundancy.hold"]}""",
                registration.encodedJson(),
            )
        }
        assertThrows(ClientMessageException.UdpPortMustNotBeZero::class.java) {
            Registration("Ana", 0u, null, emptyList()).close()
        }

        assertEquals(
            """{"type":"mix","seq":7,"volume_percent":80,"max_level_percent":95,"muted":false,"channels":[10,20]}""",
            mixMessage(MixCommand(sequence = 7uL, volumePercent = 80L, maxLevelPercent = 95L, muted = false, channelGains = listOf(10L, 20L))),
        )
        assertEquals(
            """{"type":"stats","interval_ms":1000,"packets_received":4,"packets_lost":0,"underruns":0,""" +
                """"receive_timeouts":1,"jitter_ms":1.23,"recovered":1}""",
            statsMessage(
                LinkStatsReport(
                    intervalMs = 1_000L,
                    packetsReceived = 4L,
                    packetsLost = 0L,
                    underruns = 0L,
                    receiveTimeouts = 1L,
                    jitterMs = 1.2345,
                    cushionMs = null,
                    backlogMs = null,
                    recovered = 1L,
                ),
            ),
        )
    }

    @Test
    fun theReconnectPolicyStepsAlongItsScheduleAndResets() {
        ReconnectPolicy().use { policy ->
            assertEquals(listOf(500L, 1_000L, 2_000L, 4_000L, 5_000L, 5_000L), List(6) { policy.nextDelayMs() })
            policy.reset()
            assertEquals(500L, policy.nextDelayMs())
        }
        assertEquals(500L, reconnectDelayForAttemptMs(-1L))
        assertEquals(5_000L, reconnectMaximumDelayMs())
        assertTrue(shouldReconnect(sessionRunning = true, stopped = false))
        assertFalse(shouldReconnect(sessionRunning = true, stopped = true))
    }

    @Test
    fun theClientIdRuleAndTheSavedMixesCross() {
        assertEquals("phone-1", validClientId("phone-1"))
        assertNull(validClientId("not valid!"))

        val mixes = listOf(
            SavedMix(
                name = "Verse",
                snapshot = MixSnapshot(
                    channelGains = listOf(30L, 60L),
                    channelPans = listOf(0L, 100L),
                    channelMutes = listOf(true, false),
                    channelSolos = listOf(false, true),
                    groupLevels = listOf(40L),
                    volumePercent = 70L,
                    maxLevelPercent = 90L,
                    muted = true,
                ),
            ),
        )
        assertEquals(mixes, decodeSavedMixes(encodeSavedMixes(mixes)))
        assertEquals(emptyList<SavedMix>(), decodeSavedMixes(null))
    }
}
