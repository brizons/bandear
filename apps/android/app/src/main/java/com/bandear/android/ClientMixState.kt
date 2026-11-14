package com.bandear.android

/** A named group of source channels announced by the server in `config`. Channels are 0-based. */
data class GroupInfo(val name: String, val channels: List<Int>)

/**
 * What a source channel carries, as the engineer set it; a strip shows its icon and [label]. The
 * shared core reads it from `config`, a kind this build does not know as [OTHER].
 */
enum class ChannelKind(val label: String) {
    VOICE("Voice"),
    GUITAR("Guitar"),
    BASS("Bass"),
    KEYS("Keys"),
    DRUMS("Drums"),
    PERCUSSION("Percussion"),
    HORNS("Horns"),
    STRINGS("Strings"),
    PLAYBACK("Playback"),
    CLICK("Click"),
    OTHER("Other"),
}

/**
 * One source channel's details as the server announces them in `config` (its `channels`, one entry
 * per source channel in channel order). [name] is empty when the engineer gave none, and the strip
 * is then named by its number. A [hidden] channel is heard in no mix and has no strip. [link] on a
 * channel makes it and the next one a stereo pair, shown as one strip. The defaults are a channel
 * the engineer never touched, which is how every channel of an older server reads.
 */
data class ChannelInfo(
    val name: String = "",
    val kind: ChannelKind = ChannelKind.OTHER,
    val hidden: Boolean = false,
    val link: Boolean = false,
)

/**
 * What the engineer set for this musician, as `mix_ack` and `mix_state` report it. The phone shows
 * it and never sends it back.
 *
 * [limitPercent] caps what the musician hears, apart from the phone's own level cap: the server
 * plays at most the lower of the two. [lockedChannels] are the 0-based channels whose level, pan,
 * mute and solo the server keeps whatever the phone sends. [following] is the name of the musician
 * whose mix the engineer made this one follow (a mirror), or null: while it is set the server keeps
 * every channel and group from the leader's mix, and only the phone's master volume, level cap and
 * mute apply. The defaults (no limit, nothing locked, following nobody) are what an older server,
 * which reports none of them, means.
 */
data class EngineerControls(
    val limitPercent: Int = 100,
    val lockedChannels: Set<Int> = emptySet(),
    val following: String? = null,
) {
    /** True when the engineer set a limit below full scale. */
    val limited: Boolean
        get() = limitPercent < 100

    fun isLocked(channel: Int): Boolean = channel in lockedChannels
}

/** A whole mix as the server reports it (`mix_ack`, `mix_state`), with the engineer's controls. */
data class MixReport(val mix: MixSnapshot, val engineer: EngineerControls = EngineerControls())

/**
 * The `More of me` rule ([MixSnapshot.moreOfMe]) for a musician whose channel is [channels]: one
 * input, or both sides of a stereo pair, so both sides are lifted alike and the server, which keeps
 * a pair's sides equal, plays exactly what was sent. A channel in [locked] is neither lifted nor
 * lowered: the server keeps it as the engineer left it, so it is sent as it is stored. Null when no
 * valid channel is given.
 */
internal fun MixSnapshot.moreOfMeFor(channels: List<Int>, locked: Set<Int> = emptySet()): MixSnapshot? {
    val lifted = moreOfMe(channels.firstOrNull()) ?: return null
    val gains = lifted.channelGains.copyOf()
    channels.drop(1).filter { it in gains.indices }.forEach { gains[it] = MixSnapshot.MORE_OF_ME_CHANNEL_GAIN }
    locked.filter { it in gains.indices }.forEach { gains[it] = channelGains[it] }
    return lifted.copy(channelGains = gains)
}

/**
 * The device-local mix state shared by the receive loop, the control channel and the UI.
 *
 * The values keep the original `@Volatile` copy-on-write pattern: every update writes a fresh
 * array or Boolean so a reader on another thread always sees a complete value, never a half-written
 * one. This type deliberately adds no synchronisation beyond that.
 */
internal class ClientMixState {
    @Volatile var sourceChannels = 0
    @Volatile var channelGains: IntArray = IntArray(0)
    @Volatile var channelPans: IntArray = IntArray(0)
    @Volatile var channelMutes: BooleanArray = BooleanArray(0)
    @Volatile var channelSolos: BooleanArray = BooleanArray(0)
    @Volatile var groups: List<GroupInfo> = emptyList()
    @Volatile var groupLevels: IntArray = IntArray(0)
    @Volatile var volumePercent = 100
    @Volatile var maxLevelPercent = 100
    @Volatile var muted = false

    /**
     * The playback buffer the desk set (`playback`): a fixed length in milliseconds, or
     * [PlaybackLatency.DESK_BUFFER_AUTO]. The control thread writes it, the receive loop follows it.
     * It is never stored: a session starts on Auto and the server sends it again on registering.
     */
    @Volatile var deskBufferMs = PlaybackLatency.DESK_BUFFER_AUTO

    /**
     * The packets the receive loop holds back so a delayed redundant copy can fill its lost
     * original's gap (`redundancy`, held by the shared core's reorder hold): 0 to [MAX_HOLD_PACKETS],
     * 0 for none. The control thread writes it, the receive loop follows it. Never stored: a
     * session starts with none and the server sends it again on registering.
     */
    @Volatile var redundancyHoldPackets = 0
}

/**
 * Rebuilds the per-channel state for a `config` that announces [channels] source channels.
 *
 * The server re-sends `config` mid-session when a device switch changes the channel count. This
 * resizes the per-channel arrays rather than resetting them, so a channel that still exists keeps
 * the gain, pan, mute and solo the musician set and the controls the client rebuilds stay honest
 * about the mix the server is still applying. New channels start neutral; channels that no longer
 * exist are dropped because they cannot be controlled or heard.
 *
 * Returns `true` when the controls must be rebuilt. An unchanged count is a no-op, so a repeated
 * `config` never restarts the controls.
 */
internal fun ClientMixState.applySourceChannels(channels: Int): Boolean {
    if (sourceChannels == channels && channelGains.size == channels) return false
    sourceChannels = channels
    channelGains = IntArray(channels) { index -> channelGains.getOrElse(index) { 100 } }
    channelPans = IntArray(channels) { index -> channelPans.getOrElse(index) { defaultPan(index) } }
    channelMutes = BooleanArray(channels) { index -> channelMutes.getOrElse(index) { false } }
    channelSolos = BooleanArray(channels) { index -> channelSolos.getOrElse(index) { false } }
    return true
}

/** The PMON protocol's upper bound on source channels, used by the receiver and the control channel. */
internal const val MAX_CHANNELS = 32

/**
 * The longest hold the phone keeps room for, in packets: the longest delay the desk gives a
 * redundant copy (the `redundancy` message, which the shared core reads). The shared core's reorder
 * hold clamps to the same bound.
 */
internal const val MAX_HOLD_PACKETS = 4

/** The default pan keeps the captured layout: even sources hard left, odd sources hard right. */
internal fun defaultPan(index: Int): Int = if (index % 2 == 0) 0 else 100
