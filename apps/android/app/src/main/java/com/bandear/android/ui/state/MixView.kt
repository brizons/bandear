package com.bandear.android.ui.state

import com.bandear.android.ChannelKind
import com.bandear.android.core.ChannelStrip
import com.bandear.android.core.GroupFader
import com.bandear.android.core.MixModelState
import com.bandear.android.core.MoreOfMeNotice
import com.bandear.android.core.NamedChannel
import com.bandear.android.core.ChannelKind as CoreChannelKind

/*
 * The shared core's mix as the screens show it. The core's `MixModelState` carries numbers, enums
 * and the names the engineer gave; the words are the phone's: a channel with no name is "CH n", and
 * every notice is worded here.
 */

/**
 * This state showing the mix model's [mix]: its strips, groups and master section, More of me, the
 * saved mixes' names and the active one, and the numbers of the mix shown. Everything else stays.
 */
internal fun MonitorUiState.showing(mix: MixModelState): MonitorUiState = copy(
    mix = MixUiState(
        channels = mix.strips.map { it.toUiState() },
        groups = mix.groupFaders.map { it.toUiState() },
        musicianChoices = mix.musicianChoices.map { ChannelChoice(it.index.toInt(), it.label()) },
        masterVolume = mix.masterVolume.toInt(),
        masterMuted = mix.masterMuted,
        levelCap = mix.levelCap.toInt(),
        effectiveCap = mix.effectiveCap.toInt(),
        engineerLimitPercent = mix.engineerLimitPercent.toInt(),
        limited = mix.isLimited,
        following = mix.following,
    ),
    moreOfMe = MoreOfMeUiState(
        enabled = mix.moreOfMe.isEnabled,
        musicianChannel = mix.moreOfMe.musicianChannel?.toInt(),
        notice = mix.moreOfMe.notice,
    ),
    savedMixes = savedMixes.copy(names = mix.savedMixNames, activeName = mix.activeMixName),
    mixSync = mix.sync,
)

/** What the More of me caption says for this notice. */
val MoreOfMeNotice.message: String
    get() = when (this) {
        MoreOfMeNotice.NEEDS_CHANNEL -> "Choose your channel first, then turn More of me on"
        MoreOfMeNotice.NO_CHANNEL_SELECTED -> "More of me is off: no channel is chosen"
        MoreOfMeNotice.CHANNEL_OUT_OF_RANGE -> "More of me is off: your channel is no longer in the mix"
        MoreOfMeNotice.MIX_CHANGED_ELSEWHERE -> "More of me is off: your mix was changed from the desktop"
        MoreOfMeNotice.CHANNEL_LOCKED -> "More of me is off: the engineer locked your channel"
        MoreOfMeNotice.FOLLOWING -> "More of me is off while your mix follows another musician's"
        MoreOfMeNotice.CHANNEL_PAIRED -> "More of me is off: your channel is now part of a stereo pair"
    }

/** The app's own kind for what the core read from `config`: the icons and labels are the app's. */
internal fun CoreChannelKind.toChannelKind(): ChannelKind = when (this) {
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

/** How a strip is named: the engineer's name, or "CH n" when it has none. */
private fun label(index: Long, name: String?): String = name ?: channelName(index.toInt())

private fun NamedChannel.label(): String = label(index, name)

private fun ChannelStrip.toUiState() = ChannelUiState(
    index = index.toInt(),
    label = label(index, name),
    group = group?.let { GroupTag(it.index.toInt(), it.name) },
    level = level.toInt(),
    pan = pan.toInt(),
    muted = isMuted,
    soloed = isSoloed,
    kind = kind?.toChannelKind(),
    pairedWith = pairedWith?.toInt(),
    locked = isLocked,
    following = following,
)

private fun GroupFader.toUiState() = GroupUiState(
    index = index.toInt(),
    name = name,
    channels = channels.map(Long::toInt),
    level = level.toInt(),
    members = members.map { it.label() },
    following = following,
)
