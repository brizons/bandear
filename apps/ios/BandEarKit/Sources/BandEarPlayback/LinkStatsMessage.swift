import BandEarCore

extension LinkStatsReport {
    /// The `stats` message for this report, as the shared core writes it (`statsMessage(report:)`,
    /// Android's `buildStatsMessage`): every count, `recovered` included and 0 too, as the phone
    /// announces `redundancy.recovered`; a count below zero as 0; the interval held to the 1 to
    /// 60 000 ms the desk reads; and each extra rounded to a hundredth, or left out when it was not
    /// measured or lies outside what the desk reads. It never fails.
    public var statsMessage: String {
        BandEarCore.statsMessage(
            report: BandEarCore.LinkStatsReport(
                intervalMs: intervalMilliseconds, packetsReceived: packetsReceived,
                packetsLost: packetsLost, underruns: underruns, receiveTimeouts: receiveTimeouts,
                jitterMs: jitterMilliseconds, cushionMs: cushionMilliseconds,
                backlogMs: backlogMilliseconds, recovered: recovered))
    }
}
