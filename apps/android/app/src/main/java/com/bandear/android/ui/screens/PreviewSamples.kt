package com.bandear.android.ui.screens

import com.bandear.android.ChannelKind
import com.bandear.android.core.MoreOfMeNotice
import com.bandear.android.ui.components.LinkHealth
import com.bandear.android.ui.components.MainTab
import com.bandear.android.ui.components.PillKind
import com.bandear.android.ui.state.ChannelChoice
import com.bandear.android.ui.state.ChannelUiState
import com.bandear.android.ui.state.ConnectionUiState
import com.bandear.android.ui.state.DiagnosticsUiState
import com.bandear.android.ui.state.GroupTag
import com.bandear.android.ui.state.GroupUiState
import com.bandear.android.ui.state.ManualAddress
import com.bandear.android.ui.state.MixUiState
import com.bandear.android.ui.state.MonitorUiState
import com.bandear.android.ui.state.MoreOfMeUiState
import com.bandear.android.ui.state.SavedMixesUiState
import com.bandear.android.ui.state.ServerEndpoint
import com.bandear.android.ui.state.channelName

/** The phone the artboards were drawn for, in dp; previews use it so they read like the mockup. */
internal const val PREVIEW_WIDTH_DP = 390
internal const val PREVIEW_HEIGHT_DP = 844

/** The page colour, for previews' backgrounds. */
internal const val PREVIEW_BACKGROUND = 0xFF0D1117

/** Sample states for the screens' previews: one musician, one server, a full mix. */
internal object PreviewSamples {
    val server = ServerEndpoint("192.168.1.34", 50001)

    // Eight inputs as the shared core shows them: named, a keys pair on CH 4 and CH 5 shown as one
    // strip, and a spare input (CH 8) hidden. Previews cannot load the core, so the strips are
    // written out as it would make them.
    private val levels = listOf(72, 64, 88, 55, 60, 70, 45, 80)
    private val pans = listOf(35, 60, 50, 0, 100, 50, 20, 80)
    private val groupTags = listOf("Drums", "Vocals", "Keys", "Guitar").mapIndexed(::GroupTag)

    /** The first group that lists each input, which its strip's chip names. */
    private val groupOf = listOf(0, 0, 1, 2, 2, 3, 3, 1)

    private fun strip(index: Int, label: String, kind: ChannelKind?, pan: Int = pans[index], pairedWith: Int? = null) =
        ChannelUiState(
            index = index,
            label = label,
            group = groupTags[groupOf[index]],
            level = levels[index],
            pan = pan,
            muted = index == 3,
            soloed = index == 5,
            kind = kind,
            pairedWith = pairedWith,
        )

    private val strips = listOf(
        strip(0, "Kick", ChannelKind.DRUMS),
        strip(1, "Snare", ChannelKind.DRUMS),
        strip(2, "Vox", ChannelKind.VOICE),
        // A pair panned 0 and 100 reads as a centred balance.
        strip(3, "Keys", ChannelKind.KEYS, pan = 50, pairedWith = 4),
        strip(5, "Gtr", ChannelKind.GUITAR),
        strip(6, "Bass", ChannelKind.BASS),
    )

    private val groups = listOf(
        GroupUiState(0, "Drums", listOf(0, 1), 80, listOf("Kick", "Snare")),
        GroupUiState(1, "Vocals", listOf(2, 7), 92, listOf("Vox")),
        GroupUiState(2, "Keys", listOf(3, 4), 70, listOf("Keys")),
        GroupUiState(3, "Guitar", listOf(5, 6), 65, listOf("Gtr", "Bass")),
    )

    private fun MixUiState.choosing(): MixUiState =
        copy(musicianChoices = channels.map { ChannelChoice(it.index, it.label) })

    private val mix = MixUiState(channels = strips, groups = groups, masterVolume = 85).choosing()

    /** [mix] with the engineer's [limit], the strips holding [locked] locked, and whose mix it follows. */
    private fun MixUiState.engineered(
        limit: Int = 100,
        locked: Set<Int> = emptySet(),
        following: String? = null,
    ) = copy(
        channels = channels.map { it.copy(locked = it.index in locked, following = following) },
        groups = groups.map { it.copy(following = following) },
        effectiveCap = minOf(levelCap, limit),
        engineerLimitPercent = limit,
        limited = limit < 100,
        following = following,
    )

    private val healthy = DiagnosticsUiState(
        packetsReceived = 182_340,
        packetsLost = 12,
        packetsPerSecond = 100.0,
        interArrivalMs = 10.0,
        jitterMs = 1.4,
        playbackBufferMs = 34.0,
        sampleRateHz = 48_000,
        linkHealth = LinkHealth.GOOD,
    )

    /** On stage: connected, the server holds the mix, More of me lifts Vox (CH 3). */
    val connected = MonitorUiState(
        musicianName = "Carlos D",
        connection = ConnectionUiState(
            pill = PillKind.CONNECTED,
            sessionRunning = true,
            server = server,
            mixHeldByServer = true,
            statusDetail = "connected - mix applied on the server",
            manual = ManualAddress(host = server.address),
        ),
        servers = listOf(server),
        mix = mix,
        moreOfMe = MoreOfMeUiState(enabled = true, musicianChannel = 2),
        savedMixes = SavedMixesUiState(
            names = listOf("Soundcheck", "Main set", "Acoustic"),
            activeName = "Soundcheck",
            notice = "Recalled \"Soundcheck\"",
        ),
        diagnostics = healthy,
    )

    val moreOfMeOff = connected.copy(moreOfMe = MoreOfMeUiState(enabled = false, musicianChannel = 2))

    /** A server that announces no channel details: every input by number, no icons, nothing joined. */
    val olderServer = connected.copy(
        mix = mix.copy(
            channels = levels.indices.map { strip(it, channelName(it), kind = null) },
            groups = groups.map { group -> group.copy(members = group.channels.map(::channelName)) },
        ).choosing(),
    )

    /** The engineer locked the drums and limits this musician to 60%, under the phone's cap of 80%. */
    val engineerControls = moreOfMeOff.copy(mix = mix.copy(levelCap = 80).engineered(limit = 60, locked = setOf(0, 1)))

    /**
     * The console as the Mix boards draw it: the kick locked, Vox lifted by More of me, the keys
     * pair, the guitar muted, and the engineer's limit of 85% under a level cap of 90%.
     */
    val console = connected.copy(
        mix = mix.copy(
            channels = mix.channels.map { it.copy(muted = it.index == 5) },
            levelCap = 90,
        ).engineered(limit = 85, locked = setOf(0)),
    )

    /** The engineer has this mix follow Lead vocal's; the kick stays locked as well. */
    val following = moreOfMeOff.copy(mix = mix.engineered(locked = setOf(0), following = "Lead vocal"))

    /** More of me asked for before a channel was chosen. */
    val noChannelChosen = connected.copy(
        moreOfMe = MoreOfMeUiState(enabled = false, musicianChannel = null, notice = MoreOfMeNotice.NEEDS_CHANNEL),
    )

    /** The Wi-Fi dropped: the control channel retries while the server holds the mix. */
    val reconnecting = connected.copy(
        connection = connected.connection.copy(
            pill = PillKind.RECONNECTING,
            statusDetail = "reconnecting - mix held at the server",
        ),
        diagnostics = healthy.copy(linkHealth = LinkHealth.WARNING, underruns = 3),
    )

    /** The control channel gave up; this phone still mixes on its own. */
    val lostServer = connected.copy(
        connection = connected.connection.copy(
            pill = PillKind.NOT_CONNECTED,
            mixHeldByServer = false,
            statusDetail = "unavailable - mixing on this device",
        ),
        diagnostics = healthy.copy(linkHealth = LinkHealth.LOST, controlError = "Connection refused"),
    )

    /** Just connected: the server has not announced its channels yet. */
    val waitingForChannels = connected.copy(
        connection = connected.connection.copy(pill = PillKind.CONNECTING, mixHeldByServer = false),
        mix = MixUiState(),
        moreOfMe = MoreOfMeUiState(),
        diagnostics = DiagnosticsUiState(),
    )

    val emptyMixes = connected.copy(savedMixes = SavedMixesUiState(), tab = MainTab.MIXES)

    /** No session: one server found on the network. */
    val notConnected = MonitorUiState(
        musicianName = "Carlos D",
        servers = listOf(server),
        savedMixes = connected.savedMixes.copy(notice = null),
    )

    /** No session and nothing found yet; the name has not been typed either. */
    val searching = MonitorUiState()

    /** A manual Connect that was refused. */
    val refused = notConnected.copy(
        connection = ConnectionUiState(
            problem = "Enter a valid UDP port between 1 and 65535",
            manual = ManualAddress(host = "192.168.1.34", audioPort = "70000"),
        ),
    )
}

/** Actions that do nothing, for previews. */
internal object PreviewActions : MonitorActions {
    override fun connect(server: ServerEndpoint) = Unit

    override fun connectManual(host: String, controlPort: String, audioPort: String) = Unit

    override fun editManualAddress(address: ManualAddress) = Unit

    override fun disconnect() = Unit

    override fun setChannelLevel(channel: Int, level: Int) = Unit

    override fun setChannelPan(channel: Int, pan: Int) = Unit

    override fun setChannelMuted(channel: Int, muted: Boolean) = Unit

    override fun setChannelSoloed(channel: Int, soloed: Boolean) = Unit

    override fun setGroupLevel(group: Int, level: Int) = Unit

    override fun setMasterVolume(level: Int) = Unit

    override fun setMasterMuted(muted: Boolean) = Unit

    override fun setLevelCap(level: Int) = Unit

    override fun setMoreOfMe(on: Boolean) = Unit

    override fun setMusicianChannel(channel: Int?) = Unit

    override fun saveMix(name: String) = false

    override fun recallMix(name: String) = Unit

    override fun renameMix(from: String, to: String) = Unit

    override fun deleteMix(name: String) = Unit

    override fun setName(name: String) = Unit

    override fun selectTab(tab: MainTab) = Unit
}
