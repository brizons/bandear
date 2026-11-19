import BandEarCore
import BandEarDiscovery
import Foundation
import Testing

@testable import BandEarSession

/// The saved mixes in memory, as the phone's store holds them.
private final class MemoryMixBankStore: MixBankStore {
    var savedMixes: [SavedMix]

    init(_ mixes: [SavedMix] = []) {
        savedMixes = mixes
    }
}

/// The `mix` message `command` is written as, read back as the desk reads it.
private func sentForm(of command: MixCommand) throws -> [String: Any] {
    let data = Data(mixMessage(command: command).utf8)
    return try #require(try JSONSerialization.jsonObject(with: data) as? [String: Any])
}

/// How the session runs the mix through the shared core's `MixModel`: what the model returns is
/// what goes to the desk, what the desk answers reaches the model, every connection opens with the
/// model's mix, and a recall reads the store before the model recalls. The mix's own rules are the
/// core's, tested in `bandear_core::mix`.
@Suite("Control session and the mix model")
@MainActor
struct ControlSessionMixModelTests {
    private let transport = FakeTransport()
    private let scheduler = ManualScheduler()
    private let session: ControlSession
    private let desk: FakeDesk

    init() {
        session = ControlSession(
            transport: transport, scheduler: scheduler, store: FakeMusicianStore())
        desk = FakeDesk(transport)
    }

    /// The session started, its socket open and two channels announced.
    private func connected() async throws {
        try session.start(toStage, musicianName: "Ana")
        await desk.open()
        await desk.receive(configMessage(channels: 2))
    }

    /// A model that went through what the session's went through until now: a session and a
    /// `config` for two channels.
    private func referenceModel() throws -> MixModel {
        let model = MixModel(musicianChannel: nil, savedMixes: [])
        _ = model.apply(event: .sessionStarted)
        let config = try #require(try? parseServerMessage(json: configMessage(channels: 2)))
        guard case .configuration(let configuration) = config else {
            Issue.record("not a config")
            return model
        }
        _ = model.apply(event: .configReceived(configuration))
        return model
    }

    @Test("a musician's change sends the mix the model returns for it, and shows its state")
    func aChangeSendsTheModelsCommand() async throws {
        try await connected()
        let reference = try referenceModel()
        let expected = try #require(
            reference.apply(event: .channelLevelChanged(channel: 1, level: 40)).command)

        session.apply(.channelLevelChanged(channel: 1, level: 40))

        #expect(NSDictionary(dictionary: try desk.lastMix()) == NSDictionary(dictionary: try sentForm(of: expected)))
        #expect(session.mix == reference.state())
        #expect(session.mix.strips.map(\.level) == [100, 40])
    }

    @Test("a change the model numbers nothing for sends nothing")
    func aChangeThatChangesNothingSendsNothing() async throws {
        try await connected()
        let sent = try transport.sent(ofType: "mix").count

        session.apply(.channelLevelChanged(channel: 5, level: 40))
        session.apply(.masterVolumeChanged(level: 100))

        #expect(try transport.sent(ofType: "mix").count == sent)
    }

    @Test("the desk's answer to the newest mix settles it")
    func theDesksAnswerSettlesTheMix() async throws {
        try await connected()
        session.apply(.channelLevelChanged(channel: 0, level: 30))
        #expect(!session.mix.isSettled)
        #expect(session.mix.sync == MixSync(latest: 1, answered: 0))

        try await desk.answerMixes()

        #expect(session.mix.isSettled)
        #expect(session.mix.sync == MixSync(latest: 1, answered: 1))
        #expect(session.mix.strips.map(\.level) == [30, 100])
    }

    @Test("every connection opens with the model's mix, under its newest number")
    func everyConnectionOpensWithTheModelsMix() async throws {
        try await connected()
        session.apply(.channelLevelChanged(channel: 0, level: 30))
        let before = try desk.lastMix()

        await desk.fails("wifi gone")
        try scheduler.runNextDue()
        await desk.open()

        let reopened = try #require(transport.attempts.last)
        #expect(try reopened.sentTypes() == ["register", "mix"])
        let opening = try #require(try reopened.sent(ofType: "mix").first)
        #expect(NSDictionary(dictionary: opening) == NSDictionary(dictionary: before))
        #expect(opening["seq"] as? Int == Int(session.mix.sync.latest))
    }

    @Test("a recall reads the store's mixes first, then recalls through the model and sends it")
    func aRecallReloadsTheStoreThenRecalls() async throws {
        try await connected()
        let store = MemoryMixBankStore()
        let mixes = SavedMixes(session: session, store: store)
        // Another launch, or another screen, saved a mix the session's model has not seen.
        let verse = MixSnapshot(
            channelGains: [30, 60], channelPans: [0, 100], channelMutes: [false, false],
            channelSolos: [false, false], groupLevels: [], volumePercent: 80, maxLevelPercent: 100,
            muted: false)
        store.savedMixes = [SavedMix(name: "Verse", snapshot: verse)]
        #expect(session.mix.savedMixNames == [])

        mixes.recall("Verse")

        #expect(mixes.notice == .recalled(name: "Verse"))
        #expect(session.mix.savedMixNames == ["Verse"])
        #expect(session.mix.activeMixName == "Verse")
        #expect(session.mix.strips.map(\.level) == [30, 60])
        let sent = try desk.lastMix()
        #expect(ints(sent, "channels") == [30, 60])
        #expect(sent["volume_percent"] as? Int == 80)
        #expect(sent["seq"] as? Int == Int(session.mix.sync.latest))
    }

    @Test("the model recalls only what it was handed: the store is read before the recall")
    func theModelRecallsOnlyWhatItWasHanded() async throws {
        try await connected()
        let verse = SavedMix(name: "Verse", snapshot: MixSnapshot(
            channelGains: [30, 60], channelPans: [0, 100], channelMutes: [false, false],
            channelSolos: [false, false], groupLevels: [], volumePercent: 80, maxLevelPercent: 100,
            muted: false))
        let sent = try transport.sent(ofType: "mix").count

        #expect(session.recallMix("Verse") == .missing(name: "Verse"))
        #expect(try transport.sent(ofType: "mix").count == sent)

        session.reloadSavedMixes([verse])

        #expect(session.recallMix("Verse") == .recalled(name: "Verse"))
        #expect(try transport.sent(ofType: "mix").count == sent + 1)
    }

    @Test("a save writes the model's list to the store, the musician's own levels")
    func aSaveWritesTheModelsList() async throws {
        try await connected()
        let store = MemoryMixBankStore()
        let mixes = SavedMixes(session: session, store: store)
        session.apply(.masterVolumeChanged(level: 70))

        #expect(mixes.save(as: " Verse "))

        #expect(store.savedMixes == session.savedMixes)
        #expect(store.savedMixes.map(\.name) == ["Verse"])
        #expect(store.savedMixes.first?.snapshot.volumePercent == 70)
        #expect(mixes.names == ["Verse"])
        #expect(mixes.activeName == "Verse")
    }
}
