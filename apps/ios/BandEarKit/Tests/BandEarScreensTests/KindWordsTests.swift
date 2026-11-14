import BandEarProtocol
import BandEarScreens
import Testing

/// What each kind of channel is called (`ChannelKind.label`, `ClientMixState.kt:10-21`), which no
/// Android test pins word for word.
@Suite("Kind words")
struct KindWordsTests {
    /// Every kind the desk names, in its order (`apps/server/src/protocol.rs:420-424`). The core's
    /// enum is not `CaseIterable`; a kind added there fails `KindWords.label`'s switch first.
    private static let allKinds: [ChannelKind] = [
        .voice, .guitar, .bass, .keys, .drums, .percussion, .horns, .strings, .playback, .click,
        .other,
    ]

    @Test func everyKindIsNamedAsAndroidNamesIt() {
        #expect(
            Self.allKinds.map(KindWords.label) == [
                "Voice", "Guitar", "Bass", "Keys", "Drums", "Percussion", "Horns", "Strings",
                "Playback", "Click", "Other",
            ])
    }

    /// Adapted from `KindIconsTest.kt:10-14`, which reads the labels through the icons' names.
    @Test func everyKindHasALabelOfItsOwn() {
        #expect(Set(Self.allKinds.map(KindWords.label)).count == Self.allKinds.count)
    }
}
