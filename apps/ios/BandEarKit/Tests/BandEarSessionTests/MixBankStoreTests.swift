import BandEarMix
import BandEarSession
import Foundation
import Testing

/// A throwaway `UserDefaults` suite for `body`, whose every call of `launch` is one launch of the
/// app: a fresh `UserDefaults` over the same suite, as a new process opens it.
///
/// The suite is named by a path in a new temporary directory, which keeps its file there: a suite
/// named like an app keeps an empty file in `~/Library/Preferences` even after its domain is
/// removed. The domain and the directory are removed afterwards, so nothing outlives the test.
func withThrowawayMixesSuite(
    _ body: (_ launch: () throws -> UserDefaults) throws -> Void
) throws {
    let directory = FileManager.default.temporaryDirectory
        .appendingPathComponent("bandear-tests-\(UUID().uuidString)")
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    let suite = directory.appendingPathComponent("mixes").path
    defer {
        UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite)
        try? FileManager.default.removeItem(at: directory)
    }
    try body { try #require(UserDefaults(suiteName: suite)) }
}

/// Where the adapter keeps the saved mixes: Android's `banks` key, under this app's prefix.
let savedMixesKey = "bandear.banks"

/// The JSON object `text` holds, or nil when it holds none.
private func object(_ text: String) -> Any? {
    try? JSONSerialization.jsonObject(with: Data(text.utf8))
}

/// The `UserDefaults` adapter, on a throwaway suite: what one launch saves, the next one reads, in
/// the JSON Android writes. The JSON's own rules (Android's text read, a damaged entry or value)
/// are the shared core's, tested in `crates/bandear-core/src/control/mix_bank.rs`, and the text
/// the phones already hold is pinned by ``SavedMixCompatibilityTests``.
@Suite("Mix bank store: UserDefaults")
struct UserDefaultsMixBankStoreTests {

    @Test("holds no saved mix on the first launch, nor when what it holds is no list")
    func emptyStore() throws {
        try withThrowawayMixesSuite { launch in
            let defaults = try launch()
            let store = UserDefaultsMixBankStore(defaults)

            #expect(store.savedMixes == [])

            let unreadable: [Any] = ["", "   ", #"{"name":"Verse"}"#, 42, Data([1, 2]), ["Verse"]]
            for value in unreadable {
                defaults.set(value, forKey: savedMixesKey)
                #expect(store.savedMixes == [], "\(value) is no list of mixes")
            }

            store.savedMixes = []
            #expect(UserDefaultsMixBankStore(try launch()).savedMixes == [])
        }
    }

    @Test("what one launch saves, the next one reads, written as Android writes it")
    func roundTrip() throws {
        let mixes = [
            SavedMix(
                name: "Verse",
                snapshot: MixSnapshot(
                    channelGains: [30, 60], channelPans: [0, 100], channelMutes: [true, false],
                    channelSolos: [false, true], groupLevels: [40], volumePercent: 70,
                    maxLevelPercent: 90, muted: true)),
            SavedMix(name: "Show \"A\" / ñ", snapshot: MixSnapshot()),
        ]
        try withThrowawayMixesSuite { launch in
            UserDefaultsMixBankStore(try launch()).savedMixes = mixes

            let next = try launch()

            #expect(UserDefaultsMixBankStore(next).savedMixes == mixes)
            // Android's shape (`MixBankStore.kt:74-88`, `MixSnapshot.kt:26-35`): a string holding a
            // list of `name` and `snapshot` objects, each snapshot a string of its own JSON object.
            let stored = try #require(next.string(forKey: savedMixesKey))
            let list = try #require(object(stored) as? [[String: Any]])
            try #require(list.count == 2)
            #expect(Set(list[0].keys) == ["name", "snapshot"])
            #expect(list[0]["name"] as? String == "Verse")
            let snapshot = try #require(list[0]["snapshot"] as? String)
            let fields = try #require(object(snapshot) as? [String: Any])
            #expect(
                Set(fields.keys) == [
                    "channelGains", "channelPans", "channelMutes", "channelSolos", "groupLevels",
                    "volumePercent", "maxLevelPercent", "muted",
                ])
            #expect(fields["channelGains"] as? [Int] == [30, 60])
            #expect(fields["channelMutes"] as? [Bool] == [true, false])
            #expect(fields["volumePercent"] as? Int == 70)
            #expect(fields["muted"] as? Bool == true)
        }
    }
}
