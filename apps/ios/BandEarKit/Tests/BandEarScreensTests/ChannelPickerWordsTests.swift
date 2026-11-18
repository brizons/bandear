import BandEarCore
import BandEarScreens
import Testing

/// The choices of five announced channels, "Vox", "Gtr", "Keys" paired with an unnamed fourth, and
/// an unnamed fifth, so the strips are known by 0, 1, 2 and 4.
private let choices = [
    NamedChannel(index: 0, name: "Vox"), NamedChannel(index: 1, name: "Gtr"),
    NamedChannel(index: 2, name: "Keys"), NamedChannel(index: 4, name: nil),
]

/// The mix over those choices with `channel` as the musician's.
private func session(channel: Int64?) -> MixModelState {
    mixState(musicianChoices: choices, moreOfMe: moreOfMe(channel: channel))
}

/// The "My channel" picker's words (`MixScreen.kt:290-374`) and the channel it names.
@Suite("Channel picker words")
struct ChannelPickerWordsTests {

    @Test func thePickersWordsAreAndroids() {
        #expect(ChannelPickerWords.title == "MY CHANNEL")
        #expect(ChannelPickerWords.none == "None")
        #expect(ChannelPickerWords.waiting == "Waiting for the server's channels")
        #expect(ChannelPickerWords.chooseHint == "Choose your channel")
        #expect(ChannelPickerWords.chosen == "Chosen")
    }

    /// Android passes `moreOfMe.musicianChannel?.let(mix::labelOf)` (`MixScreen.kt:147`): the
    /// channel as its strip names it, or nothing when none is chosen.
    @Test func theChosenChannelIsNamedAsItsStripNamesIt() {
        #expect(ChannelPickerWords.chosenLabel(in: session(channel: 2)) == "Keys")
        #expect(ChannelPickerWords.chosenLabel(in: session(channel: 4)) == "CH 5")
        #expect(ChannelPickerWords.chosenLabel(in: session(channel: nil)) == nil)
        #expect(
            ChannelPickerWords.description(ChannelPickerWords.chosenLabel(in: session(channel: 2)))
                == "My channel: Keys. Tap to change")
    }
}
