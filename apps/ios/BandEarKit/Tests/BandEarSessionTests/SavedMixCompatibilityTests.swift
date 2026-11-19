import BandEarCore
import Foundation
import Testing

@testable import BandEarSession

/// The saved mixes already on musicians' phones survive the move of their JSON into the shared
/// core: every fixture the app's own Swift encoder wrote before the move (``SavedMixFixtures``)
/// reads, through the store and the core, as exactly the mixes it holds, and the store now writes
/// each one byte for byte as that encoder did, so the app that wrote them would read them too.
///
/// Before the Swift encoder was deleted, these fixtures were checked against it both ways: it
/// wrote each one, it read the core's text back to the same mixes, and the core wrote each one byte
/// for byte (the commit that added them).
@Suite("Saved mixes: what the phones already hold")
struct SavedMixCompatibilityTests {
    static let files = SavedMixFixtures.banks.map(\.file)

    @Test("the store reads each fixture as the mixes it holds", arguments: files)
    func theStoreReadsEachFixture(file: String) throws {
        let text = try text(file)
        try withThrowawayMixesSuite { launch in
            let defaults = try launch()
            defaults.set(text, forKey: savedMixesKey)

            #expect(UserDefaultsMixBankStore(defaults).savedMixes == (try mixes(file)))
        }
    }

    @Test("the store writes each fixture byte for byte", arguments: files)
    func theStoreWritesEachFixtureByteForByte(file: String) throws {
        let mixes = try mixes(file)
        try withThrowawayMixesSuite { launch in
            let defaults = try launch()

            UserDefaultsMixBankStore(defaults).savedMixes = mixes

            #expect(defaults.string(forKey: savedMixesKey) == (try text(file)))
        }
    }

    private func mixes(_ file: String) throws -> [SavedMix] {
        try #require(SavedMixFixtures.banks.first { $0.file == file }).mixes
    }

    /// The fixture's text, exactly as `UserDefaults` held it.
    private func text(_ file: String) throws -> String {
        let url = try #require(
            Bundle.module.url(
                forResource: file, withExtension: "json", subdirectory: "SavedMixFixtures"))
        return try String(contentsOf: url, encoding: .utf8)
    }
}
