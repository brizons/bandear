import Foundation
import Testing

import BandEarCore

/// Drives the shared mix model through the generated bindings: a level change that sends, the
/// desk's ack that settles it, a saved mix recalled. The rules are tested in Rust
/// (`crates/bandear-core/src/mix/`); these tests prove each call and type crosses the binding and
/// reads in Swift as the session and the screens expect.
struct MixModelBindingTests {
    /// A model whose session started and whose desk announced a voice and a keys pair.
    private func connected() -> MixModel {
        let model = MixModel(musicianChannel: nil, savedMixes: [])
        _ = model.apply(event: .sessionStarted)
        _ = model.apply(
            event: .configReceived(
                DeskConfiguration(
                    sourceChannels: 3, sampleRate: nil,
                    groups: [ChannelGroup(id: "band", name: "Band", channels: [0, 1, 2])],
                    channels: [
                        SourceChannel(
                            name: " Vox ", kind: .voice, isHidden: false, isLinkedToNext: false),
                        SourceChannel(
                            name: "Keys", kind: .keys, isHidden: false, isLinkedToNext: true),
                        SourceChannel(
                            name: "", kind: .keys, isHidden: false, isLinkedToNext: false),
                    ])))
        return model
    }

    @Test func aLevelChangeSendsTheMixUnderItsNumberAndTheDesksAckSettlesIt() throws {
        let model = connected()

        let change = model.apply(event: .channelLevelChanged(channel: 1, level: 40))

        let command = try #require(change.command)
        #expect(command.sequence == change.state.sync.latest)
        #expect(command.sequence == 1)
        // The pair's strip writes both sides.
        #expect(command.channelGains == [100, 40, 40])
        #expect(!change.state.isSettled)
        #expect(change.state.strips.map(\.index) == [0, 1])
        #expect(change.state.strips.map(\.name) == ["Vox", "Keys"])
        #expect(change.state.strips.map(\.level) == [100, 40])
        #expect(change.state.strips.map(\.pairedWith) == [nil, 2])
        #expect(
            change.state.groupFaders.map(\.members) == [
                [NamedChannel(index: 0, name: "Vox"), NamedChannel(index: 1, name: "Keys")]
            ])
        #expect(model.state() == change.state)
        #expect(model.openingMix() == command)

        let ack = try parseServerMessage(
            json: #"""
                {"type":"mix_ack","seq":1,"volume_percent":100,"max_level_percent":100,
                 "muted":false,"channels":[100,40,40],"pans":[0,0,100],"mutes":[false,false,false],
                 "solos":[false,false,false],"group_levels":[100]}
                """#)
        let answer = try #require(mixEventReportedBy(message: ack))
        let settled = model.apply(event: answer)

        #expect(settled.command == nil)
        #expect(settled.state.isSettled)
        #expect(settled.state.sync == MixSync(latest: 1, answered: 1))
        let hold = try parseServerMessage(json: #"{"type":"redundancy","delay_blocks":1}"#)
        #expect(mixEventReportedBy(message: hold) == nil)
    }

    @Test func moreOfMeReportsTheMusiciansChannelAndLiftsOnlyWhatIsSent() throws {
        let model = MixModel(musicianChannel: 2, savedMixes: [])
        _ = model.apply(event: .sessionStarted)

        let announced = model.apply(
            event: .configReceived(
                DeskConfiguration(sourceChannels: 2, sampleRate: nil, groups: [], channels: nil)))
        let chosen = model.apply(event: .musicianChannelChosen(channel: 1))
        let lifted = model.apply(event: .moreOfMeToggled(on: true))

        // Channel 2 is past the two announced, so the remembered channel is forgotten.
        #expect(announced.musicianChannelChanged)
        #expect(announced.state.moreOfMe.musicianChannel == nil)
        #expect(chosen.musicianChannelChanged)
        #expect(chosen.command == nil)
        #expect(
            lifted.state.moreOfMe
                == MoreOfMeState(isEnabled: true, musicianChannel: 1, notice: nil))
        #expect(try #require(lifted.command).channelGains == [50, 100])
        #expect(lifted.state.strips.map(\.level) == [100, 100])
        #expect(model.apply(event: .moreOfMeToggled(on: false)).state.moreOfMe.notice == nil)
    }

    @Test func aSavedMixIsSavedRenamedAndRecalled() throws {
        let model = connected()
        _ = model.apply(event: .channelLevelChanged(channel: 0, level: 30))

        let saved = model.saveMix(name: " Verse ")
        _ = model.apply(event: .channelLevelChanged(channel: 0, level: 90))
        let renamed = model.renameMix(name: "Verse", newName: "Chorus")
        let recall = model.recallMix(name: "Chorus")

        #expect(saved.outcome == .saved(name: "Verse"))
        #expect(saved.savedMixesChanged)
        #expect(saved.state.savedMixNames == ["Verse"])
        #expect(saved.state.activeMixName == "Verse")
        #expect(renamed.outcome == .renamed(from: "Verse", to: "Chorus"))
        #expect(recall.outcome == .recalled(name: "Chorus"))
        #expect(try #require(recall.update.command).channelGains == [30, 100, 100])
        #expect(recall.update.state.strips.map(\.level) == [30, 100])
        #expect(model.savedMixes().map(\.name) == ["Chorus"])
        #expect(model.recallMix(name: "Gone").outcome == .missing(name: "Gone"))
        #expect(model.saveMix(name: "  ").outcome == .nameNeeded)
    }

    @Test func aRecallWhileFollowingIsRefusedAndSendsNothing() throws {
        let model = connected()
        let verse = MixSnapshot(
            channelGains: [10, 20, 30], channelPans: [], channelMutes: [], channelSolos: [],
            groupLevels: [], volumePercent: 100, maxLevelPercent: 100, muted: false)
        _ = model.reloadSavedMixes(mixes: [SavedMix(name: "Verse", snapshot: verse)])
        let push = try #require(
            mixEventReportedBy(
                message: try parseServerMessage(
                    json: #"""
                        {"type":"mix_state","volume_percent":100,"max_level_percent":100,
                         "muted":false,"channels":[70,70,70],"pans":[0,0,100],
                         "mutes":[false,false,false],
                         "solos":[false,false,false],"group_levels":[100],
                         "engineer_limit_percent":60,"following":"Ana"}
                        """#)))
        let followed = model.apply(event: push)

        let recall = model.recallMix(name: "Verse")

        #expect(followed.state.following == "Ana")
        #expect(followed.state.effectiveCap == 60)
        #expect(followed.state.isLimited)
        #expect(followed.state.recallRefusal == .following(leader: "Ana"))
        #expect(recall.outcome == .refused(name: "Verse", refusal: .following(leader: "Ana")))
        #expect(recall.update.command == nil)
        #expect(recall.update.state == followed.state)
    }
}
