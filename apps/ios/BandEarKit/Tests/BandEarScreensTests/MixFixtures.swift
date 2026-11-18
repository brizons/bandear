import BandEarCore

// The mix as the shared core hands it to the screens. The core decides every value (which strips
// show, which are locked, what the musician hears at most); these build the records it returns,
// so each screen test sets exactly what its words read.

/// One strip as the core shows it: an unnamed mono channel at level 80, centred, neither muted nor
/// soloed, in no group and held by nobody, as Android's `channel(index)` test helper builds one.
func channelStrip(
    _ index: Int64, name: String? = nil, group: GroupTag? = nil, level: Int64 = 80, pan: Int64 = 50,
    muted: Bool = false, soloed: Bool = false, kind: ChannelKind? = .other, pairedWith: Int64? = nil,
    locked: Bool = false, following: String? = nil
) -> ChannelStrip {
    ChannelStrip(
        index: index, name: name, group: group, level: level, pan: pan, isMuted: muted,
        isSoloed: soloed, kind: kind, pairedWith: pairedWith, isLocked: locked, following: following)
}

/// The mix state with what a test leaves out as a new phone has it: no strips, groups or choices,
/// the master and the cap at 100, nothing limited or followed, More of me off with no channel.
func mixState(
    strips: [ChannelStrip] = [], groupFaders: [GroupFader] = [],
    musicianChoices: [NamedChannel] = [], levelCap: Int64 = 100, engineerLimitPercent: Int64 = 100,
    following: String? = nil,
    moreOfMe: MoreOfMeState = MoreOfMeState(isEnabled: false, musicianChannel: nil, notice: nil)
) -> MixModelState {
    MixModelState(
        isSessionRunning: true, sourceChannels: Int64(strips.count), strips: strips,
        groupFaders: groupFaders, musicianChoices: musicianChoices, masterVolume: 100,
        masterMuted: false, levelCap: levelCap,
        effectiveCap: min(levelCap, engineerLimitPercent),
        engineerLimitPercent: engineerLimitPercent, isLimited: engineerLimitPercent < 100,
        following: following, moreOfMe: moreOfMe,
        recallRefusal: following.map { .following(leader: $0) },
        sync: MixSync(latest: 0, answered: 0), isSettled: true, savedMixNames: [],
        activeMixName: nil)
}

/// More of me `on` or off, with `channel` as the musician's.
func moreOfMe(on: Bool = false, channel: Int64? = nil) -> MoreOfMeState {
    MoreOfMeState(isEnabled: on, musicianChannel: channel, notice: nil)
}
