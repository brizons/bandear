import BandEarCore
import BandEarMix
import Foundation
import Testing

@testable import BandEarSession

/// The saved mixes already on musicians' phones survive the move of their JSON into the shared
/// core: every fixture the app's own Swift encoder wrote (``SavedMixFixtures``) reads, through the
/// core, as exactly the mixes it holds, and what the core writes the app's Swift reader reads back
/// unchanged.
@Suite("Saved mixes: what the phones already hold")
struct SavedMixCompatibilityTests {
    static let files = SavedMixFixtures.banks.map(\.file)

    @Test("each fixture is what the app's Swift encoder writes for its mixes", arguments: files)
    func eachFixtureIsWhatTheAppWrites(file: String) throws {
        #expect(SavedMixesJSON.json(of: try mixes(file)) == (try text(file)))
    }

    @Test("the core reads each fixture as the app's Swift reader does", arguments: files)
    func theCoreReadsEachFixture(file: String) throws {
        let text = try text(file)

        let read = appMixes(decodeSavedMixes(json: text))

        #expect(read == (try mixes(file)))
        #expect(read == SavedMixesJSON.mixes(fromJSON: text))
    }

    @Test("the app's Swift reader reads what the core writes", arguments: files)
    func theAppReadsWhatTheCoreWrites(file: String) throws {
        let mixes = try mixes(file)

        #expect(SavedMixesJSON.mixes(fromJSON: encodeSavedMixes(mixes: coreMixes(mixes))) == mixes)
    }

    @Test("the core writes each fixture byte for byte", arguments: files)
    func theCoreWritesEachFixtureByteForByte(file: String) throws {
        #expect(encodeSavedMixes(mixes: coreMixes(try mixes(file))) == (try text(file)))
    }

    private func mixes(_ file: String) throws -> [BandEarMix.SavedMix] {
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

/// `mixes` as the core holds them.
private func coreMixes(_ mixes: [BandEarMix.SavedMix]) -> [BandEarCore.SavedMix] {
    mixes.map { mix in
        let snapshot = mix.snapshot
        return BandEarCore.SavedMix(
            name: mix.name,
            snapshot: BandEarCore.MixSnapshot(
                channelGains: snapshot.channelGains.map(Int64.init),
                channelPans: snapshot.channelPans.map(Int64.init),
                channelMutes: snapshot.channelMutes, channelSolos: snapshot.channelSolos,
                groupLevels: snapshot.groupLevels.map(Int64.init),
                volumePercent: Int64(snapshot.volumePercent),
                maxLevelPercent: Int64(snapshot.maxLevelPercent), muted: snapshot.muted))
    }
}

/// `mixes` as the app holds them.
private func appMixes(_ mixes: [BandEarCore.SavedMix]) -> [BandEarMix.SavedMix] {
    mixes.map { mix in
        let snapshot = mix.snapshot
        return BandEarMix.SavedMix(
            name: mix.name,
            snapshot: BandEarMix.MixSnapshot(
                channelGains: snapshot.channelGains.map { Int($0) },
                channelPans: snapshot.channelPans.map { Int($0) },
                channelMutes: snapshot.channelMutes, channelSolos: snapshot.channelSolos,
                groupLevels: snapshot.groupLevels.map { Int($0) },
                volumePercent: Int(snapshot.volumePercent),
                maxLevelPercent: Int(snapshot.maxLevelPercent), muted: snapshot.muted))
    }
}
