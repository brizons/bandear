package com.bandear.android.ui.state

import com.bandear.android.ChannelKind
import com.bandear.android.core.ChannelGroup
import com.bandear.android.core.DeskConfiguration
import com.bandear.android.core.MixEvent
import com.bandear.android.core.MixModel
import com.bandear.android.core.MixModelState
import com.bandear.android.core.MixReport
import com.bandear.android.core.MoreOfMeNotice
import com.bandear.android.core.SourceChannel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import com.bandear.android.core.ChannelKind as CoreChannelKind

/**
 * The shared core's mix as the screens show it ([showing]): the core's strips, groups and master
 * section carried over, with the phone's own words. The rules behind them are tested in the core.
 */
class MixViewTest {
    private fun config(channels: Int, details: List<SourceChannel>? = null, groups: List<ChannelGroup> = emptyList()) =
        MixEvent.ConfigReceived(
            DeskConfiguration(sourceChannels = channels.toUByte(), sampleRate = null, groups = groups, channels = details),
        )

    private fun channel(
        name: String = "",
        kind: CoreChannelKind = CoreChannelKind.OTHER,
        hidden: Boolean = false,
        link: Boolean = false,
    ) = SourceChannel(name, kind, isHidden = hidden, isLinkedToNext = link)

    private fun group(name: String, vararg channels: Long) = ChannelGroup(id = null, name = name, channels = channels.toList())

    /** The model after [events], in a session. */
    private fun shown(vararg events: MixEvent, musicianChannel: Long? = null): MonitorUiState =
        MixModel(musicianChannel, emptyList()).use { model ->
            model.apply(MixEvent.SessionStarted)
            events.fold(model.state()) { _, event -> model.apply(event).state }.let { MonitorUiState().showing(it) }
        }

    @Test
    fun stripsAreNamedByTheEngineerOrByTheirNumberAndAPairIsOneStrip() {
        val details = listOf(
            channel(" Vox ", CoreChannelKind.VOICE),
            channel("Keys", CoreChannelKind.KEYS, link = true),
            channel(),
            channel("Spare", hidden = true),
            channel("  "),
        )

        val mix = shown(config(5, details)).mix

        assertEquals(listOf("Vox", "Keys", "CH 5"), mix.channels.map { it.label })
        assertEquals(listOf(0, 1, 4), mix.channels.map { it.index })
        assertEquals(listOf(null, 2, null), mix.channels.map { it.pairedWith })
        assertEquals(listOf(ChannelKind.VOICE, ChannelKind.KEYS, ChannelKind.OTHER), mix.channels.map { it.kind })
        assertEquals(listOf(ChannelChoice(0, "Vox"), ChannelChoice(1, "Keys"), ChannelChoice(4, "CH 5")), mix.musicianChoices)
        assertEquals("CH 4", mix.labelOf(3))
    }

    @Test
    fun withoutDetailsEveryChannelShowsByNumberWithNoKind() {
        val mix = shown(config(2)).mix

        assertEquals(listOf("CH 1", "CH 2"), mix.channels.map { it.label })
        assertEquals(listOf<ChannelKind?>(null, null), mix.channels.map { it.kind })
    }

    @Test
    fun everyKindTheDeskNamesShowsAsTheAppsOwnKind() {
        val kinds = CoreChannelKind.entries

        val mix = shown(config(kinds.size, kinds.map { channel(kind = it) })).mix

        assertEquals(ChannelKind.entries, mix.channels.map { it.kind })
        assertEquals(kinds.map { it.name }, ChannelKind.entries.map { it.name })
    }

    @Test
    fun aGroupNamesItsStripsAndWithoutDetailsItsChannelsByNumber() {
        val groups = listOf(group("Band", 0, 1, 2))
        val paired = listOf(channel("Keys", link = true), channel(), channel("Vox"))

        val named = shown(config(3, paired, groups)).mix
        val numbered = shown(config(3, groups = groups)).mix

        assertEquals(listOf("Keys", "Vox"), named.groups.single().members)
        assertEquals(GroupTag(0, "Band"), named.channels.first().group)
        assertEquals(listOf("CH 1", "CH 2", "CH 3"), numbered.groups.single().members)
        assertEquals(listOf(0, 1, 2), numbered.groups.single().channels)
    }

    @Test
    fun theMasterSectionShowsTheCapAndTheEngineersLimit() {
        val limited = MixReport(
            sequence = null,
            volumePercent = 70,
            maxLevelPercent = 90,
            muted = true,
            channelGains = List(32) { 100L },
            pans = List(32) { if (it % 2 == 0) 0L else 100L },
            channelMutes = List(32) { false },
            channelSolos = List(32) { false },
            groupLevels = List(16) { 100L },
            engineerLimitPercent = 60,
            locked = List(32) { it == 0 },
            following = "Ana",
        )

        val state = shown(config(2), MixEvent.MixPushed(limited))

        assertEquals(70, state.mix.masterVolume)
        assertTrue(state.mix.masterMuted)
        assertEquals(90, state.mix.levelCap)
        assertEquals(60, state.mix.effectiveCap)
        assertEquals(60, state.mix.engineerLimitPercent)
        assertTrue(state.mix.limited)
        assertEquals("Ana", state.mix.following)
        assertEquals(listOf(true, false), state.mix.channels.map { it.locked })
        assertEquals(listOf("Ana", "Ana"), state.mix.channels.map { it.following })
    }

    @Test
    fun moreOfMeAndItsNoticeAreTheModelsInTheScreensWords() {
        val asked = shown(config(2), MixEvent.MoreOfMeToggled(on = true))
        val remembered = shown(musicianChannel = 2)

        assertEquals(
            MoreOfMeUiState(enabled = false, musicianChannel = null, notice = MoreOfMeNotice.NEEDS_CHANNEL),
            asked.moreOfMe,
        )
        assertEquals("Choose your channel first, then turn More of me on", MoreOfMeNotice.NEEDS_CHANNEL.message)
        assertEquals(2, remembered.moreOfMe.musicianChannel)
        assertEquals("a remembered channel before any config", "CH 3", remembered.mix.labelOf(2))
    }

    @Test
    fun everyMoreOfMeNoticeHasItsOwnWords() {
        val words = mapOf(
            MoreOfMeNotice.NEEDS_CHANNEL to "Choose your channel first, then turn More of me on",
            MoreOfMeNotice.NO_CHANNEL_SELECTED to "More of me is off: no channel is chosen",
            MoreOfMeNotice.CHANNEL_OUT_OF_RANGE to "More of me is off: your channel is no longer in the mix",
            MoreOfMeNotice.MIX_CHANGED_ELSEWHERE to "More of me is off: your mix was changed from the desktop",
            MoreOfMeNotice.CHANNEL_LOCKED to "More of me is off: the engineer locked your channel",
            MoreOfMeNotice.FOLLOWING to "More of me is off while your mix follows another musician's",
            MoreOfMeNotice.CHANNEL_PAIRED to "More of me is off: your channel is now part of a stereo pair",
        )

        assertEquals(MoreOfMeNotice.entries.toSet(), words.keys)
        for ((notice, message) in words) assertEquals(message, notice.message)
    }

    @Test
    fun theSavedMixesAreListedInSavedOrderWithTheActiveOneMarked() {
        val listed = MixModel(null, emptyList()).use { model ->
            model.apply(config(2))
            model.saveMix("Verse")
            MonitorUiState().showing(model.saveMix(" Chorus ").state)
        }

        assertEquals(listOf(SavedMixUiState("Verse", false), SavedMixUiState("Chorus", true)), listed.savedMixes.mixes)
        assertNull(listed.savedMixes.emptyMessage)
    }

    @Test
    fun showingTheMixLeavesEverythingElseAsItWas() {
        val other = MonitorUiState(musicianName = "Ana", savedMixes = SavedMixesUiState(notice = "Saved \"Verse\""))

        val shown = MixModel(null, emptyList()).use { model -> other.showing(model.state()) }

        assertEquals("Ana", shown.musicianName)
        assertEquals("Saved \"Verse\"", shown.savedMixes.notice)
        assertEquals(other.connection, shown.connection)
    }

    @Test
    fun theNumbersShownAreTheModelsSync() {
        val state: MixModelState = MixModel(null, emptyList()).use { model ->
            model.apply(MixEvent.SessionStarted)
            model.apply(config(2))
            model.apply(MixEvent.ChannelLevelChanged(channel = 0, level = 40)).state
        }

        assertEquals(state.sync, MonitorUiState().showing(state).mixSync)
        assertEquals(1uL, state.sync.latest)
    }
}
