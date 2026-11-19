import Foundation
import Testing

import BandEarCore

/// Calls the shared control path through the generated bindings: a desk message read, the phone's
/// messages written, the reconnect policy stepped. The rules are tested in Rust
/// (`crates/bandear-core/src/control/`); these tests prove each call and type crosses the binding
/// and reads in Swift as the session expects.
struct ControlPathBindingTests {
    @Test func aDeskMessageIsReadIntoSwiftValues() throws {
        let message = try parseServerMessage(
            json: #"""
                {"type":"config","source_channels":2,"sample_rate":48000,
                 "groups":[{"id":"kit","name":"Drums","channels":[0,1]}],
                 "channels":[{"name":"Lead","kind":"voice","hidden":false,"link":true},"junk"]}
                """#)

        #expect(
            message
                == .configuration(
                    DeskConfiguration(
                        sourceChannels: 2, sampleRate: 48_000,
                        groups: [ChannelGroup(id: "kit", name: "Drums", channels: [0, 1])],
                        channels: [
                            SourceChannel(
                                name: "Lead", kind: .voice, isHidden: false, isLinkedToNext: true),
                            SourceChannel(
                                name: "", kind: .other, isHidden: false, isLinkedToNext: false),
                        ])))
        #expect(
            try parseServerMessage(json: #"{"type":"playback","buffer":{"mode":"fixed","ms":40}}"#)
                == .playbackBuffer(.fixed(milliseconds: 40)))
        #expect(
            try parseServerMessage(json: #"{"type":"something_new"}"#)
                == .unrecognized(messageType: "something_new"))
    }

    @Test func anUnreadableDeskMessageThrowsWhy() {
        #expect(throws: ServerMessageError.NotJson) {
            try parseServerMessage(json: "not json")
        }
        #expect(
            throws: ServerMessageError.MissingField(field: "buffer", messageType: "playback")
        ) {
            try parseServerMessage(json: #"{"type":"playback"}"#)
        }
    }

    @Test func thePhonesMessagesAreWrittenAsTheDeskReadsThem() throws {
        let registration = try Registration(
            name: "  Ana  ", udpPort: 50_000, clientId: "phone-1",
            capabilities: allClientCapabilities())
        #expect(registration.name() == "Ana")
        #expect(registration.clientId() == "phone-1")
        #expect(
            registration.encodedJson()
                == #"{"type":"register","name":"Ana","udp_port":50000,"client_id":"phone-1","capabilities":["stats.v2","playback.buffer","redundancy.recovered","redundancy.hold"]}"#
        )
        #expect(throws: ClientMessageError.UdpPortMustNotBeZero) {
            try Registration(name: "Ana", udpPort: 0, clientId: nil, capabilities: [])
        }

        #expect(
            mixMessage(
                command: MixCommand(
                    sequence: 7, volumePercent: 80, maxLevelPercent: 95, muted: false,
                    channelGains: [10, 20]))
                == #"{"type":"mix","seq":7,"volume_percent":80,"max_level_percent":95,"muted":false,"channels":[10,20]}"#
        )
        #expect(
            statsMessage(
                report: LinkStatsReport(
                    intervalMs: 1_000, packetsReceived: 4, packetsLost: 0, underruns: 0,
                    receiveTimeouts: 1, jitterMs: 1.2345, cushionMs: nil, backlogMs: nil,
                    recovered: 1))
                == #"{"type":"stats","interval_ms":1000,"packets_received":4,"packets_lost":0,"underruns":0,"receive_timeouts":1,"jitter_ms":1.23,"recovered":1}"#
        )
    }

    @Test func theReconnectPolicyStepsAlongItsScheduleAndResets() {
        let policy = ReconnectPolicy()

        #expect((0..<6).map { _ in policy.nextDelayMs() } == [500, 1_000, 2_000, 4_000, 5_000, 5_000])
        policy.reset()
        #expect(policy.nextDelayMs() == 500)
        #expect(reconnectDelayForAttemptMs(attempt: -1) == 500)
        #expect(reconnectMaximumDelayMs() == 5_000)
        #expect(shouldReconnect(sessionRunning: true, stopped: false))
        #expect(!shouldReconnect(sessionRunning: true, stopped: true))
    }

    @Test func theClientIdChoiceCrosses() {
        #expect(
            chooseClientId(stored: "phone-1", generated: "fresh-id")
                == ClientIdChoice(clientId: "phone-1", shouldSave: false))
        #expect(
            chooseClientId(stored: nil, generated: "fresh-id")
                == ClientIdChoice(clientId: "fresh-id", shouldSave: true))
        #expect(
            chooseClientId(stored: "not valid!", generated: "fresh-id")
                == ClientIdChoice(clientId: "fresh-id", shouldSave: true))
    }

    @Test func theClientIdRuleAndTheSavedMixesCross() {
        #expect(validClientId(raw: "phone-1") == "phone-1")
        #expect(validClientId(raw: "not valid!") == nil)

        let mixes = [
            SavedMix(
                name: "Show / A",
                snapshot: MixSnapshot(
                    channelGains: [30, 60], channelPans: [0, 100], channelMutes: [true, false],
                    channelSolos: [false, true], groupLevels: [40], volumePercent: 70,
                    maxLevelPercent: 90, muted: true))
        ]
        let json = encodeSavedMixes(mixes: mixes)
        #expect(json.hasPrefix(#"[{"name":"Show / A","snapshot":"{\"channelGains\":[30,60]"#))
        #expect(decodeSavedMixes(json: json) == mixes)
        #expect(decodeSavedMixes(json: nil) == [])
    }
}
