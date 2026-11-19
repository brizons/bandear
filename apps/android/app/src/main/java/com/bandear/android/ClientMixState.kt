package com.bandear.android

/**
 * What a source channel carries, as the engineer set it; a strip shows its icon and [label]. The
 * shared core reads it from `config`, a kind this build does not know as [OTHER], and the screens
 * take it from the core's strips ([com.bandear.android.ui.state.toChannelKind]).
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
 * The master values and desk settings the receive loop reads at each packet, written from the main
 * and control threads.
 *
 * The values keep the original `@Volatile` pattern: every update writes a fresh value, so a reader on
 * another thread always sees a complete one. This type deliberately adds no synchronisation beyond
 * that. The mix itself is the shared core's (`MixModel`); the ViewModel mirrors its master volume,
 * ceiling and mute here as each change lands.
 */
internal class ClientMixState {
    @Volatile var volumePercent = 100

    /** The ceiling the phone plays at: the lower of the musician's cap and the engineer's limit. */
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
     * original's gap (`redundancy`, held by the shared core's reorder hold, which clamps it to the
     * longest delay the desk gives a redundant copy), 0 for none. The control thread writes it,
     * the receive loop follows it. Never stored: a session starts with none and the server sends it
     * again on registering.
     */
    @Volatile var redundancyHoldPackets = 0
}

