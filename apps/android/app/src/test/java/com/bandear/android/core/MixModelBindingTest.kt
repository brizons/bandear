package com.bandear.android.core

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Drives the shared mix model through the generated bindings: a level change that sends, the desk's
 * ack that settles it, a saved mix recalled. The rules are tested in Rust
 * (`crates/bandear-core/src/mix/`); these tests prove each call and type crosses the binding and
 * reads in Kotlin as the view model and the screens expect.
 */
class MixModelBindingTest {
    /** A model whose session started and whose desk announced a voice and a keys pair. */
    private fun connected(): MixModel {
        val model = MixModel(null, emptyList())
        model.apply(MixEvent.SessionStarted)
        model.apply(
            MixEvent.ConfigReceived(
                DeskConfiguration(
                    sourceChannels = 3u,
                    sampleRate = null,
                    groups = listOf(ChannelGroup(id = "band", name = "Band", channels = listOf(0L, 1L, 2L))),
                    channels = listOf(
                        SourceChannel(name = " Vox ", kind = ChannelKind.VOICE, isHidden = false, isLinkedToNext = false),
                        SourceChannel(name = "Keys", kind = ChannelKind.KEYS, isHidden = false, isLinkedToNext = true),
                        SourceChannel(name = "", kind = ChannelKind.KEYS, isHidden = false, isLinkedToNext = false),
                    ),
                ),
            ),
        )
        return model
    }

    @Test
    fun aLevelChangeSendsTheMixUnderItsNumberAndTheDesksAckSettlesIt() {
        connected().use { model ->
            val change = model.apply(MixEvent.ChannelLevelChanged(channel = 1L, level = 40L))

            val command = requireNotNull(change.command)
            assertEquals(change.state.sync.latest, command.sequence)
            assertEquals(1uL, command.sequence)
            // The pair's strip writes both sides.
            assertEquals(listOf(100L, 40L, 40L), command.channelGains)
            assertFalse(change.state.isSettled)
            assertEquals(listOf(0L, 1L), change.state.strips.map { it.index })
            assertEquals(listOf("Vox", "Keys"), change.state.strips.map { it.name })
            assertEquals(listOf(100L, 40L), change.state.strips.map { it.level })
            assertEquals(listOf(null, 2L), change.state.strips.map { it.pairedWith })
            assertEquals(
                listOf(listOf(NamedChannel(index = 0L, name = "Vox"), NamedChannel(index = 1L, name = "Keys"))),
                change.state.groupFaders.map { it.members },
            )
            assertEquals(change.state, model.state())
            assertEquals(command, model.openingMix())

            val ack = parseServerMessage(
                """{"type":"mix_ack","seq":1,"volume_percent":100,"max_level_percent":100,"muted":false,""" +
                    """"channels":[100,40,40],"pans":[0,0,100],"mutes":[false,false,false],""" +
                    """"solos":[false,false,false],"group_levels":[100]}""",
            )
            val settled = model.apply(mixEventReportedBy(ack)!!)

            assertNull(settled.command)
            assertTrue(settled.state.isSettled)
            assertEquals(MixSync(latest = 1uL, answered = 1uL), settled.state.sync)
            assertNull(mixEventReportedBy(parseServerMessage("""{"type":"redundancy","delay_blocks":1}""")))
        }
    }

    @Test
    fun moreOfMeReportsTheMusiciansChannelAndLiftsOnlyWhatIsSent() {
        MixModel(2L, emptyList()).use { model ->
            model.apply(MixEvent.SessionStarted)

            val announced = model.apply(
                MixEvent.ConfigReceived(
                    DeskConfiguration(sourceChannels = 2u, sampleRate = null, groups = emptyList(), channels = null),
                ),
            )
            val chosen = model.apply(MixEvent.MusicianChannelChosen(channel = 1L))
            val lifted = model.apply(MixEvent.MoreOfMeToggled(on = true))

            // Channel 2 is past the two announced, so the remembered channel is forgotten.
            assertTrue(announced.musicianChannelChanged)
            assertNull(announced.state.moreOfMe.musicianChannel)
            assertTrue(chosen.musicianChannelChanged)
            assertNull(chosen.command)
            assertEquals(MoreOfMeState(isEnabled = true, musicianChannel = 1L, notice = null), lifted.state.moreOfMe)
            assertEquals(listOf(50L, 100L), lifted.command?.channelGains)
            assertEquals(listOf(100L, 100L), lifted.state.strips.map { it.level })
        }
    }

    @Test
    fun aSavedMixIsSavedRenamedAndRecalled() {
        connected().use { model ->
            model.apply(MixEvent.ChannelLevelChanged(channel = 0L, level = 30L))

            val saved = model.saveMix(" Verse ")
            model.apply(MixEvent.ChannelLevelChanged(channel = 0L, level = 90L))
            val renamed = model.renameMix("Verse", "Chorus")
            val recall = model.recallMix("Chorus")

            assertEquals(MixBankOutcome.Saved(name = "Verse"), saved.outcome)
            assertTrue(saved.savedMixesChanged)
            assertEquals(listOf("Verse"), saved.state.savedMixNames)
            assertEquals("Verse", saved.state.activeMixName)
            assertEquals(MixBankOutcome.Renamed(from = "Verse", to = "Chorus"), renamed.outcome)
            assertEquals(RecallOutcome.Recalled(name = "Chorus"), recall.outcome)
            assertEquals(listOf(30L, 100L, 100L), recall.update.command?.channelGains)
            assertEquals(listOf(30L, 100L), recall.update.state.strips.map { it.level })
            assertEquals(listOf("Chorus"), model.savedMixes().map { it.name })
            assertEquals(RecallOutcome.Missing(name = "Gone"), model.recallMix("Gone").outcome)
            assertEquals(MixBankOutcome.NameNeeded, model.saveMix("  ").outcome)
        }
    }

    @Test
    fun aRecallWhileFollowingIsRefusedAndSendsNothing() {
        connected().use { model ->
            val verse = MixSnapshot(
                channelGains = listOf(10L, 20L, 30L),
                channelPans = emptyList(),
                channelMutes = emptyList(),
                channelSolos = emptyList(),
                groupLevels = emptyList(),
                volumePercent = 100L,
                maxLevelPercent = 100L,
                muted = false,
            )
            model.reloadSavedMixes(listOf(SavedMix(name = "Verse", snapshot = verse)))
            val push = mixEventReportedBy(
                parseServerMessage(
                    """{"type":"mix_state","volume_percent":100,"max_level_percent":100,"muted":false,""" +
                        """"channels":[70,70,70],"pans":[0,0,100],"mutes":[false,false,false],""" +
                        """"solos":[false,false,false],"group_levels":[100],""" +
                        """"engineer_limit_percent":60,"following":"Ana"}""",
                ),
            )!!
            val followed = model.apply(push)

            val recall = model.recallMix("Verse")

            assertEquals("Ana", followed.state.following)
            assertEquals(60L, followed.state.effectiveCap)
            assertTrue(followed.state.isLimited)
            assertEquals(RecallRefusal.Following(leader = "Ana"), followed.state.recallRefusal)
            assertEquals(
                RecallOutcome.Refused(name = "Verse", refusal = RecallRefusal.Following(leader = "Ana")),
                recall.outcome,
            )
            assertNull(recall.update.command)
            assertEquals(followed.state, recall.update.state)
        }
    }
}
