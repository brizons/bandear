package com.bandear.android.ui.state

import com.bandear.android.ChannelKind
import com.bandear.android.core.MixSync
import com.bandear.android.core.MoreOfMeNotice
import com.bandear.android.ui.components.LinkHealth
import com.bandear.android.ui.components.MainTab
import com.bandear.android.ui.components.PillKind

/** The UDP port this phone listens on for audio until the musician sets another. */
const val DEFAULT_AUDIO_PORT = 50000

/** The server's WebSocket control port until the musician or a beacon says otherwise. */
const val DEFAULT_CONTROL_PORT = 50001

/** The longest musician name the server keeps (it cuts longer ones); the name fields stop there. */
const val MAX_NAME_LENGTH = 64

/** A neutral level for a channel, a group or the master: unchanged gain. */
private const val NEUTRAL_LEVEL = 100

/**
 * Everything the Connect, Mix, Saved mixes and Settings screens show, as one immutable value.
 *
 * The screens only read it. The mix, More of me and the saved mixes' list are the shared core's
 * `MixModel` state as the screens show it ([showing]); every other change goes through [reduce], so
 * the same events always give the same screen. Lists here are plain immutable lists and every
 * nested value is a data class, so two states are equal exactly when the screens would look the
 * same and [mixSync], the value no screen shows, is the same too.
 */
data class MonitorUiState(
    /** The name this musician registers under, exactly as typed. */
    val musicianName: String = "",
    val connection: ConnectionUiState = ConnectionUiState(),
    /** The servers advertising on the network right now, in the order they were first heard. */
    val servers: List<ServerEndpoint> = emptyList(),
    val mix: MixUiState = MixUiState(),
    val moreOfMe: MoreOfMeUiState = MoreOfMeUiState(),
    val savedMixes: SavedMixesUiState = SavedMixesUiState(),
    val diagnostics: DiagnosticsUiState = DiagnosticsUiState(),
    /** The tab of the connected app that is showing. */
    val tab: MainTab = MainTab.MIX,
    /**
     * The numbers of the mix shown: the newest the phone sent and the newest the server answered.
     * Not shown; the states are published in the order the model made them, so these never go back.
     */
    val mixSync: MixSync = MixSync(latest = 0uL, answered = 0uL),
)

/** A BandEar server's control endpoint: where it was heard from and its WebSocket port. */
data class ServerEndpoint(val address: String, val controlPort: Int) {
    /** How the server is named on screen. Beacons carry no server name, so the address names it. */
    val label: String
        get() = "$address:$controlPort"
}

/**
 * The manual connection form in Settings, kept as typed so a mistake can be corrected.
 *
 * [audioPort] is this phone's own UDP listen port. It is used for a discovered server too, exactly
 * as the old screen kept its port field when a found server filled the address.
 */
data class ManualAddress(
    val host: String = "",
    val controlPort: String = DEFAULT_CONTROL_PORT.toString(),
    val audioPort: String = DEFAULT_AUDIO_PORT.toString(),
)

/** The session and its control channel, as the status pill, the header and Settings show them. */
data class ConnectionUiState(
    val pill: PillKind = PillKind.NOT_CONNECTED,
    /** True from Connect until Disconnect, or until the receiver ends on its own. */
    val sessionRunning: Boolean = false,
    /** The server of the running session; null when no session runs. */
    val server: ServerEndpoint? = null,
    /**
     * True once the server acknowledged a mix: it applies the mix and keeps it while the control
     * channel is away. False means this phone still mixes on its own. The reconnecting banner says
     * which one holds the mix.
     */
    val mixHeldByServer: Boolean = false,
    /** The control channel's state in words, including who holds the mix. */
    val statusDetail: String = "not connected",
    /** Why the last Connect was refused (an empty address, a bad port); null when it was not. */
    val problem: String? = null,
    val manual: ManualAddress = ManualAddress(),
) {
    /** True while the control channel retries on its own; the reconnecting banner shows then. */
    val reconnecting: Boolean
        get() = pill == PillKind.RECONNECTING
}

/**
 * The mix as the screens show it: the shared core's strips, group faders and master section, with
 * the words added ([showing]). Every rule (which channels a strip writes, a pair's balance, what a
 * lock or a mix being followed allows) is the core's; this only carries the result.
 */
data class MixUiState(
    /** One strip per shown channel or stereo pair, left to right. */
    val channels: List<ChannelUiState> = emptyList(),
    /** One fader per announced group. */
    val groups: List<GroupUiState> = emptyList(),
    /** The channels the musician can call theirs: one per strip of the announced channels, by name. */
    val musicianChoices: List<ChannelChoice> = emptyList(),
    val masterVolume: Int = NEUTRAL_LEVEL,
    val masterMuted: Boolean = false,
    /** The ceiling the phone never plays above, in percent, as the musician set it. */
    val levelCap: Int = NEUTRAL_LEVEL,
    /** The loudest the musician hears: the lower of [levelCap] and the engineer's limit. */
    val effectiveCap: Int = NEUTRAL_LEVEL,
    /** The engineer's limit for this musician, [NEUTRAL_LEVEL] when none is set. */
    val engineerLimitPercent: Int = NEUTRAL_LEVEL,
    /** True when the engineer set a limit below full scale. */
    val limited: Boolean = false,
    /**
     * The musician whose mix this one follows, as the server last reported it, or null: while it is
     * set the channels and groups are the leader's, and only the master, level cap and mute are the
     * phone's.
     */
    val following: String? = null,
) {
    /** How [channel]'s strip is named: the engineer's name, or "CH n" when it has none. */
    fun labelOf(channel: Int): String =
        musicianChoices.firstOrNull { it.index == channel }?.label
            ?: channels.firstOrNull { it.index == channel }?.label
            ?: channelName(channel)
}

/**
 * One channel strip: its fader, pan, Mute and Solo. [index] is the 0-based channel it is known by
 * (a stereo pair's first channel); [label] is the engineer's name for it, or "CH 1" up.
 */
data class ChannelUiState(
    val index: Int,
    val label: String,
    /** The group the strip's chip names; null when no announced group lists the channel. */
    val group: GroupTag?,
    val level: Int,
    /** 0 is hard left, 50 centre, 100 hard right; a stereo pair's balance, read the same way. */
    val pan: Int,
    val muted: Boolean,
    val soloed: Boolean,
    /** What the channel carries; null when the server announced no details, so no icon shows. */
    val kind: ChannelKind? = null,
    /** The pair's second channel when the strip is a stereo pair; null for a single channel. */
    val pairedWith: Int? = null,
    /**
     * True when the engineer locked the strip (a pair when either side is): its level, pan, mute
     * and solo cannot be changed from the phone.
     */
    val locked: Boolean = false,
    /**
     * The musician whose mix this one follows, or null: while it is set the strip's level, pan,
     * mute and solo are the leader's and cannot be changed from the phone.
     */
    val following: String? = null,
) {
    /** True when the strip is a stereo pair, whose pan is a balance. */
    val stereo: Boolean
        get() = pairedWith != null
}

/** A group as a channel's chip names it; [index] picks the chip's tone. */
data class GroupTag(val index: Int, val name: String)

/**
 * One group fader. [channels] are the 0-based channels the server put in the group; [members] names
 * the group's strips as they are shown, a hidden channel left out and a stereo pair named once.
 */
data class GroupUiState(
    val index: Int,
    val name: String,
    val channels: List<Int>,
    val level: Int,
    val members: List<String> = channels.map(::channelName),
    /** The musician whose mix this one follows, or null: while it is set the level is the leader's. */
    val following: String? = null,
)

/** A channel the musician can call theirs, by the 0-based channel its strip is known by. */
data class ChannelChoice(val index: Int, val label: String)

/**
 * More of me and the channel it lifts. [musicianChannel] is remembered on the phone, apart from any
 * saved mix, because it says who the musician is rather than how the mix sounds. [notice] says why
 * More of me is not doing what the musician last asked; the toggle's caption words it ([message]).
 */
data class MoreOfMeUiState(
    val enabled: Boolean = false,
    val musicianChannel: Int? = null,
    val notice: MoreOfMeNotice? = null,
)

/** One saved mix in the list. [active] marks the mix last saved or recalled. */
data class SavedMixUiState(val name: String, val active: Boolean)

/**
 * The saved mixes, in the order the phone keeps them, and the outcome of the last action on them.
 *
 * The empty state is derived from the list itself, never kept as a separate message, so it can
 * never show while a mix exists.
 */
data class SavedMixesUiState(
    val names: List<String> = emptyList(),
    /** The mix last saved or recalled, while it is still saved. */
    val activeName: String? = null,
    /** What the last save, recall, rename or delete did, or why it could not. */
    val notice: String? = null,
) {
    val mixes: List<SavedMixUiState>
        get() = names.map { SavedMixUiState(it, it == activeName) }

    /** Shown in place of the list when there is nothing saved; null whenever one mix exists. */
    val emptyMessage: String?
        get() = if (names.isEmpty()) "No saved mixes yet" else null
}

/**
 * What the receiver and the control channel report, for the diagnostics in Settings.
 *
 * Counters are for the running session and start again at every Connect. [linkHealth] is judged on
 * the phone from the same report the server judges, with the same thresholds.
 */
data class DiagnosticsUiState(
    val packetsReceived: Long = 0,
    /** Packets the stream skipped, from the sequence numbers. */
    val packetsLost: Long = 0,
    val packetsPerSecond: Double = 0.0,
    val interArrivalMs: Double = 0.0,
    val jitterMs: Double = 0.0,
    /** How far playback runs behind the newest sample written, smoothed. */
    val playbackBufferMs: Double = 0.0,
    /** The sample rate of the last packet; null before the first one. */
    val sampleRateHz: Int? = null,
    val discardedSamples: Long = 0,
    val packetsWithDiscardedSamples: Long = 0,
    /** Playback underruns reported in this session's link reports. */
    val underruns: Long = 0,
    val linkHealth: LinkHealth = LinkHealth.UNKNOWN,
    /** The last receiver or connection problem; null when there was none. */
    val lastError: String? = null,
    /** The last control channel problem; null when there was none. */
    val controlError: String? = null,
) {
    /** Lost packets as a percentage of those expected (received plus lost); 0 before any. */
    val lossPercent: Double
        get() {
            val expected = packetsReceived + packetsLost
            return if (expected == 0L) 0.0 else packetsLost * 100.0 / expected
        }
}

/** How a channel is named on its strip and in the More of me caption: "CH 1" for index 0. */
fun channelName(index: Int): String = "CH ${index + 1}"

/** The musician a mix follows, as the phone names them: [following] trimmed, or "another musician". */
fun leaderName(following: String): String = following.trim().ifEmpty { "another musician" }
