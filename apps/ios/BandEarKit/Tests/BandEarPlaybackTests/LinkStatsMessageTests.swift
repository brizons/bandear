import BandEarProtocol
import Foundation
import Testing

import BandEarPlayback

/// A link report as the `stats` message the desk reads: Android's `buildStatsMessage`
/// (`ControlChannel.kt:571-591`), from the phone's own ``LinkStatsReport`` to ``LinkStats``.
@Suite("Link stats message")
struct LinkStatsMessageTests {
    /// The keys every report's message carries, `recovered` among them as on Android.
    private static let countKeys: Set<String> = [
        "type", "interval_ms", "packets_received", "packets_lost", "underruns", "receive_timeouts",
        "recovered",
    ]

    /// `report`'s message as the desk reads it.
    private func wire(_ report: LinkStatsReport) throws -> [String: Any] {
        let json = try ClientMessage.stats(LinkStats(report)).encodedJSON()
        return try #require(try JSONSerialization.jsonObject(with: json) as? [String: Any])
    }

    @Test("carries every count of the report, each in its own field")
    func carriesEveryCount() throws {
        let stats = LinkStats(
            LinkStatsReport(
                intervalMilliseconds: 1_250, packetsReceived: 98, packetsLost: 2, underruns: 1,
                receiveTimeouts: 3, recovered: 4))

        #expect(
            stats
                == (try LinkStats(
                    intervalMilliseconds: 1_250, packetsReceived: 98, packetsLost: 2,
                    underruns: 1, receiveTimeouts: 3, recovered: 4)))
    }

    @Test("always carries recovered, as Android does, even when nothing was recovered")
    func alwaysCarriesRecovered() throws {
        let message = try wire(
            LinkStatsReport(
                intervalMilliseconds: 1_000, packetsReceived: 200, packetsLost: 0, underruns: 0,
                receiveTimeouts: 0))

        #expect(Set(message.keys) == Self.countKeys)
        #expect(message["recovered"] as? Int == 0)
    }

    @Test("carries the extras that were measured, to a hundredth, and leaves out the rest")
    func carriesTheMeasuredExtras() throws {
        let message = try wire(
            LinkStatsReport(
                intervalMilliseconds: 1_000, packetsReceived: 200, packetsLost: 0, underruns: 0,
                receiveTimeouts: 0, jitterMilliseconds: 1.2345, cushionMilliseconds: 10,
                backlogMilliseconds: nil, recovered: 0))

        #expect(Set(message.keys) == Self.countKeys.union(["jitter_ms", "cushion_ms"]))
        #expect(message["jitter_ms"] as? Double == 1.23)
        #expect(message["cushion_ms"] as? Double == 10)
    }

    @Test("sends a count below zero as zero rather than stopping the app")
    func sendsANegativeCountAsZero() throws {
        let stats = LinkStats(
            LinkStatsReport(
                intervalMilliseconds: 1_000, packetsReceived: -1, packetsLost: -2, underruns: -3,
                receiveTimeouts: -4, recovered: -5))

        #expect(stats.packetsReceived == 0)
        #expect(stats.packetsLost == 0)
        #expect(stats.underruns == 0)
        #expect(stats.receiveTimeouts == 0)
        #expect(stats.recovered == 0)
    }

    @Test("holds an interval the desk would refuse to the range it reads")
    func holdsTheIntervalToTheDesksRange() {
        func interval(_ milliseconds: Int64) -> UInt32 {
            LinkStats(
                LinkStatsReport(
                    intervalMilliseconds: milliseconds, packetsReceived: 0, packetsLost: 0,
                    underruns: 0, receiveTimeouts: 0)
            ).intervalMilliseconds
        }

        #expect(interval(0) == 1)
        #expect(interval(-5) == 1)
        #expect(interval(1) == 1)
        #expect(interval(60_000) == 60_000)
        #expect(interval(60_001) == 60_000)
        #expect(interval(.max) == 60_000)
    }

    /// Android's `STATS_AFTER_A_COPY_FILLED_A_GAP` (`HoldPlayoutTest.kt:113-114`): a one-second
    /// report, four packets played, one of them brought by a delayed copy. The shared core's hold
    /// makes this report (`hold_playout` in `bandear-core`); its message is the JSON the server's
    /// own protocol test reads
    /// (`protocol::tests::the_stats_a_phone_sends_after_a_copy_filled_a_gap`), compared parsed, so
    /// key order does not matter.
    @Test("sends the report of a copy that filled a gap as the server's protocol test reads it")
    func aCopyThatFilledAGapReadsAsTheServerExpects() throws {
        let report = LinkStatsReport(
            intervalMilliseconds: 1_000, packetsReceived: 4, packetsLost: 0, underruns: 0,
            receiveTimeouts: 1, recovered: 1)
        let server =
            #"{"type":"stats","interval_ms":1000,"packets_received":4,"packets_lost":0,"underruns":0,"receive_timeouts":1,"recovered":1}"#

        let sent = try JSONSerialization.jsonObject(
            with: ClientMessage.stats(LinkStats(report)).encodedJSON())
        let expected = try JSONSerialization.jsonObject(with: Data(server.utf8))
        #expect(sent as? NSDictionary == expected as? NSDictionary)
    }
}
