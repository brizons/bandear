import BandEarCore
import BandEarScreens
import Testing

/// The core's mix over five announced channels, "Vox", "Gtr", "Keys", an unnamed one and another
/// unnamed one; the third and fourth are one stereo pair, so the strips are Vox (0), Gtr (1), Keys
/// (2-3) and CH 5 (4). The group "Band" holds Vox and Gtr. `locked` and `muted` name strips by the
/// channel each is known by.
private func mix(
    locked: Set<Int64> = [], following: String? = nil, muted: Set<Int64> = []
) -> MixModelState {
    let band = GroupTag(index: 0, name: "Band")
    let shown: [(index: Int64, name: String?, group: GroupTag?, pair: Int64?)] = [
        (0, "Vox", band, nil), (1, "Gtr", band, nil), (2, "Keys", nil, 3), (4, nil, nil, nil),
    ]
    let strips = shown.map { strip in
        channelStrip(
            strip.index, name: strip.name, group: strip.group, muted: muted.contains(strip.index),
            pairedWith: strip.pair, locked: locked.contains(strip.index), following: following)
    }
    let members = [NamedChannel(index: 0, name: "Vox"), NamedChannel(index: 1, name: "Gtr")]
    let group = GroupFader(
        index: 0, name: "Band", channels: [0, 1], level: 100, members: members, following: following)
    return mixState(strips: strips, groupFaders: [group], following: following)
}

private func strip(_ channel: Int64, in mix: MixModelState) throws -> ChannelStrip {
    try #require(mix.strips.first { $0.index == channel })
}

/// How the strip words read the iOS mix: which strip is the musician's, what a strip shows where
/// its group goes, and the words its controls give a screen reader.
@Suite("Strip words")
struct StripWordsTests {

    /// Android's order in `ChannelStrip` (`MixStrips.kt:153-165`): a lock outlasts a mirror, so a
    /// locked strip says "Locked" while the mix follows another's; then "Following"; then the group.
    @Test func theStripTagPutsTheLockFirstThenFollowingThenTheGroup() throws {
        let plain = mix()
        #expect(StripWords.tag(for: try strip(0, in: plain)) == .group(GroupTag(index: 0, name: "Band")))
        #expect(StripWords.tag(for: try strip(4, in: plain)) == .noGroup)

        let held = mix(locked: [0], following: "Ana")
        #expect(StripWords.tag(for: try strip(0, in: held)) == .locked)
        #expect(StripWords.tag(for: try strip(1, in: held)) == .following)
        #expect(StripWords.tag(for: try strip(4, in: held)) == .following)
        #expect(StripWords.tag(for: try strip(2, in: mix(locked: [2]))) == .locked)

        #expect(StripTag.locked.text == "Locked")
        #expect(StripTag.following.text == "Following")
        #expect(StripTag.group(GroupTag(index: 0, name: "Band")).text == "Band")
        #expect(StripTag.noGroup.text == "No group")
        #expect(StripWords.noGroup == "No group")
    }

    /// Android's `ChannelRow` (`MixStrips.kt:104-105`): the musician's strip is the one their channel
    /// names, a pair by its first channel, and it is amber only while More of me is on.
    @Test func theMusiciansStripIsTheOneTheirChannelNames() throws {
        let plain = mix()
        let keys = try strip(2, in: plain)
        let vox = try strip(0, in: plain)
        let lifting = moreOfMe(on: true, channel: 2)

        #expect(StripWords.isMine(keys, moreOfMe: lifting))
        #expect(!StripWords.isMine(vox, moreOfMe: lifting))
        #expect(!StripWords.isMine(keys, moreOfMe: moreOfMe()))
        #expect(StripWords.fill(for: keys, moreOfMe: lifting) == .accent)
        #expect(StripWords.fill(for: vox, moreOfMe: lifting) == .idle)
        #expect(
            StripWords.fill(for: keys, moreOfMe: moreOfMe(channel: 2))
                == .idle)
        #expect(StripWords.fill(for: try strip(2, in: mix(muted: [2])), moreOfMe: lifting) == .muted)
    }

    /// Android's strip fader (`MixStrips.kt:172`) reads the strip's own label, mute, lock and leader;
    /// "your channel" follows the musician's channel whether or not More of me is on.
    @Test func aStripsFaderDescriptionReadsTheStrip() throws {
        let held = mix(locked: [2], following: "Ana", muted: [2])

        #expect(
            StripWords.faderDescription(
                for: try strip(2, in: held), moreOfMe: moreOfMe(channel: 2))
                == "Keys level, your channel, muted, locked by the engineer, following Ana's mix")
        #expect(
            StripWords.faderDescription(for: try strip(4, in: mix()), moreOfMe: moreOfMe())
                == "CH 5 level")
    }

    /// Android's `MuteSoloButtons` (`MuteSoloButtons.kt:55-66`), named with the strip's
    /// `lockedLabel` (`MixStrips.kt:184`), as the channel sheet's keys are (`ChannelSheet.kt:166,175`).
    @Test func theMuteAndSoloKeysNameTheirChannel() throws {
        let plain = mix()
        #expect(StripWords.muteDescription(for: try strip(4, in: plain)) == "Mute CH 5")
        #expect(StripWords.soloDescription(for: try strip(4, in: plain)) == "Solo CH 5")

        let held = mix(locked: [0], following: "Ana")
        #expect(
            StripWords.muteDescription(for: try strip(0, in: held))
                == "Mute Vox, locked by the engineer, following Ana's mix")
        #expect(
            StripWords.soloDescription(for: try strip(0, in: held))
                == "Solo Vox, locked by the engineer, following Ana's mix")

        #expect(StripWords.muteLetter == "M")
        #expect(StripWords.soloLetter == "S")
        #expect(StripWords.openSettingsHint == "Open the channel's settings")
    }

    /// Android's `GroupStrip` reads the group's name and leader from its state, and lists its members
    /// as the mix names them.
    @Test func aGroupFaderReadsItsNameItsLeaderAndItsMembers() throws {
        let band = try #require(mix().groupFaders.first)
        #expect(StripWords.groupFaderDescription(for: band) == "Band group level")
        #expect(StripWords.groupMembersText(band.members.map(\.label)) == "Vox · Gtr")

        let followed = try #require(mix(following: "Ana").groupFaders.first)
        #expect(StripWords.groupFaderDescription(for: followed) == "Band group level, following Ana's mix")
    }
}
