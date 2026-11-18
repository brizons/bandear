import BandEarCore
import BandEarScreens
import Testing

/// The strips of three announced channels, "Keys" paired with an unnamed second, and an unnamed
/// third, with the strips known by `locked` held by the engineer.
private func mix(locked: Set<Int64> = [], following: String? = nil) -> [ChannelStrip] {
    [
        channelStrip(
            0, name: "Keys", kind: .keys, pairedWith: 1, locked: locked.contains(0),
            following: following),
        channelStrip(2, locked: locked.contains(2), following: following),
    ]
}

private func strip(_ channel: Int64, in strips: [ChannelStrip]) throws -> ChannelStrip {
    try #require(strips.first { $0.index == channel })
}

/// The channel sheet's words that no Android test pins (`ChannelSheet.kt:141-224`).
@Suite("Channel sheet words")
struct ChannelSheetWordsTests {

    @Test func theSheetsCaptionsAreAndroids() {
        #expect(ChannelSheetWords.levelTitle == "LEVEL")
        #expect(ChannelSheetWords.stereoTag == "Stereo pair")
        #expect(ChannelSheetWords.yourChannel == "Your channel")
        #expect(ChannelSheetWords.noGroup == "No group")
        #expect(ChannelSheetWords.done == "Done")
    }

    /// The sheet's pan control (`ChannelSheet.kt:147`): the channel's pan or balance, held as the
    /// strip is held.
    @Test func thePanControlSaysWhoseItIsAndWhyItIsHeld() throws {
        #expect(ChannelSheetWords.panControlDescription(for: try strip(2, in: mix())) == "CH 3 pan")
        #expect(
            ChannelSheetWords.panControlDescription(
                for: try strip(0, in: mix(locked: [0], following: "Ana")))
                == "Keys balance, locked by the engineer, following Ana's mix")
    }
}
