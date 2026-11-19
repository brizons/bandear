import Foundation
import Testing

import BandEarPlayback

/// The phone's own ``LinkStatsReport`` handed to the shared core, which writes the `stats` message
/// the desk reads. The message's rules (the clamps, the rounding, what is left out) are the core's
/// and tested there (`crates/bandear-core/src/control/client_message.rs`); this proves each field
/// of the report reaches the core as itself.
@Suite("Link stats message")
struct LinkStatsMessageTests {
    /// `report`'s message as the desk reads it.
    private func wire(_ report: LinkStatsReport) throws -> [String: Any] {
        try #require(
            try JSONSerialization.jsonObject(with: Data(report.statsMessage.utf8))
                as? [String: Any])
    }

    @Test("hands every field of the report to the core, each one as itself")
    func everyFieldReachesItsOwnKey() throws {
        let message = try wire(
            LinkStatsReport(
                intervalMilliseconds: 1_250, packetsReceived: 98, packetsLost: 2, underruns: 1,
                receiveTimeouts: 3, jitterMilliseconds: 1.5, cushionMilliseconds: 20.25,
                backlogMilliseconds: 37.75, recovered: 4))

        #expect(
            NSDictionary(dictionary: message)
                == NSDictionary(dictionary: [
                    "type": "stats", "interval_ms": 1_250, "packets_received": 98,
                    "packets_lost": 2, "underruns": 1, "receive_timeouts": 3, "jitter_ms": 1.5,
                    "cushion_ms": 20.25, "backlog_ms": 37.75, "recovered": 4,
                ]))
    }

    @Test("leaves out the extras the report did not measure")
    func anExtraNotMeasuredIsLeftOut() throws {
        let message = try wire(
            LinkStatsReport(
                intervalMilliseconds: 1_000, packetsReceived: 200, packetsLost: 0, underruns: 0,
                receiveTimeouts: 0, cushionMilliseconds: 10))

        #expect(
            Set(message.keys) == [
                "type", "interval_ms", "packets_received", "packets_lost", "underruns",
                "receive_timeouts", "cushion_ms", "recovered",
            ])
    }
}
