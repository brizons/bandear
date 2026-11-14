import BandEarCore

/// What the receiver tells the rest of the app: Android's `PmonPlayer.Listener`
/// (`PmonPlayer.kt:73-106`). Every call comes on the receive thread, which also feeds the output,
/// so an implementation only hands the value over and never blocks.
///
/// Android's `serverOwnsMix()` is not here: it is one of the ``PlayerSettings``.
public protocol ReceiverListener: AnyObject {
    /// The receiver's metrics, after every packet played.
    func onStats(_ stats: ReceiverStats)

    /// One link report for the desk, about once a second once the audio starts or a startup grace
    /// passes without it, packets or not.
    func onLinkStats(_ report: LinkStatsReport)

    /// A packet that could not be played (a `BandEarCore.ReceiveError`), an output that could not
    /// open, or the socket failing. Android passes the message; here the error itself comes.
    func onError(_ error: any Error)

    /// One launch's loop ended, on request or on its own: once per launch, after the session flag
    /// went down if this was the latest launch. The receiver counts as alive until this returns
    /// (``AudioReceiver/isAlive``).
    func onStopped()
}

/// The receiver's own metrics after each packet played: Android's nine `onStats` values
/// (`PmonPlayer.kt:77-87`), all since the loop started, as the shared core's receive pipeline
/// measures them (`BandEarCore.ReceiverStats`), in the units the app's screens read.
///
/// A value not measured yet is `nil`, where Android shows 0: the spacing and jitter until a second
/// packet arrived, the backlog until the output said what it presented. The sample rate is never
/// missing, since only a played packet reports.
public struct ReceiverStats: Equatable, Sendable {
    /// Packets played.
    public let packets: Int64
    /// Packets the stream skipped, each counted once.
    public let lost: Int64
    /// The rate the last packet played announced.
    public let sampleRate: Int
    /// Packets played per second since the loop started.
    public let packetsPerSecond: Double
    /// The smoothed spacing between arrivals.
    public let interArrivalMilliseconds: Double?
    /// The smoothed jitter around that spacing.
    public let jitterMilliseconds: Double?
    /// Samples the output did not take, so never played.
    public let discardedSamples: Int64
    /// Packets that lost any samples that way.
    public let packetsWithDiscardedSamples: Int64
    /// The audio queued ahead of the listener, smoothed by a tenth per packet.
    public let playbackBacklogMilliseconds: Double?
    /// Whether the output was reaching the listener as this packet was written
    /// (``PlaybackSink/isPlaying``). Not one of Android's values. While it is `false` the packets
    /// still count as played, but nobody hears them.
    public let outputPlaying: Bool

    public init(
        packets: Int64, lost: Int64, sampleRate: Int, packetsPerSecond: Double,
        interArrivalMilliseconds: Double?, jitterMilliseconds: Double?, discardedSamples: Int64,
        packetsWithDiscardedSamples: Int64, playbackBacklogMilliseconds: Double?,
        outputPlaying: Bool
    ) {
        self.packets = packets
        self.lost = lost
        self.sampleRate = sampleRate
        self.packetsPerSecond = packetsPerSecond
        self.interArrivalMilliseconds = interArrivalMilliseconds
        self.jitterMilliseconds = jitterMilliseconds
        self.discardedSamples = discardedSamples
        self.packetsWithDiscardedSamples = packetsWithDiscardedSamples
        self.playbackBacklogMilliseconds = playbackBacklogMilliseconds
        self.outputPlaying = outputPlaying
    }

    /// The pipeline's `stats`, field for field. A rate comes from a packet's 32-bit header field,
    /// which the core refuses above `Int32.max`, so it always fits an `Int`.
    public init(_ stats: BandEarCore.ReceiverStats) {
        self.init(
            packets: stats.packets, lost: stats.lost, sampleRate: Int(stats.sampleRate),
            packetsPerSecond: stats.packetsPerSecond,
            interArrivalMilliseconds: stats.interArrivalMs, jitterMilliseconds: stats.jitterMs,
            discardedSamples: stats.discardedSamples,
            packetsWithDiscardedSamples: stats.packetsWithDiscardedSamples,
            playbackBacklogMilliseconds: stats.playbackBacklogMs,
            outputPlaying: stats.outputPlaying)
    }
}

/// What the phone experienced over one report interval: the counts the desk classifies the link
/// from. ``packetsReceived`` counts the packets played, so a dropped duplicate or late packet is
/// not received twice, and received plus lost is the stretch of the stream the interval covered.
///
/// The shared core's link report (`BandEarCore.LinkStatsReport`, a port of the Android client's
/// `LinkStatsReport` in `LinkStats.kt`), in the units the app's screens read. It is the phone's own
/// measurement, not the wire message: ``LinkStats`` is what is sent, and it checks and rounds these
/// values on the way out.
///
/// The extras the phone announces with `stats.v2`, in milliseconds, are nil when not measured
/// (Android's `NaN`):
/// - ``jitterMilliseconds``: how much the packets' arrival varied over the interval, the mean
///   distance of each packet's spacing from the smoothed spacing. Not measured when no two packets
///   arrived in the interval.
/// - ``cushionMilliseconds``: the playback cushion the phone aims for at the interval's end, not
///   measured before the first packet opened the player.
/// - ``backlogMilliseconds``: the audio queued for playback at the interval's end, as last read
///   from the player.
///
/// ``recovered`` (announced with `redundancy.recovered`) counts the packets over the interval that
/// were played after filling a gap: each arrived for the first time after a newer one, while the
/// receiver held that newer one back for the desk's delayed copies, which only a delayed redundant
/// copy or Wi-Fi reordering does. They are in ``packetsReceived``, not in ``packetsLost``.
public struct LinkStatsReport: Equatable, Sendable {
    public let intervalMilliseconds: Int64
    public let packetsReceived: Int64
    public let packetsLost: Int64
    public let underruns: Int64
    public let receiveTimeouts: Int64
    public let jitterMilliseconds: Double?
    public let cushionMilliseconds: Double?
    public let backlogMilliseconds: Double?
    public let recovered: Int64

    public init(
        intervalMilliseconds: Int64, packetsReceived: Int64, packetsLost: Int64, underruns: Int64,
        receiveTimeouts: Int64, jitterMilliseconds: Double? = nil,
        cushionMilliseconds: Double? = nil, backlogMilliseconds: Double? = nil, recovered: Int64 = 0
    ) {
        self.intervalMilliseconds = intervalMilliseconds
        self.packetsReceived = packetsReceived
        self.packetsLost = packetsLost
        self.underruns = underruns
        self.receiveTimeouts = receiveTimeouts
        self.jitterMilliseconds = jitterMilliseconds
        self.cushionMilliseconds = cushionMilliseconds
        self.backlogMilliseconds = backlogMilliseconds
        self.recovered = recovered
    }

    /// The pipeline's `report`, field for field.
    public init(_ report: BandEarCore.LinkStatsReport) {
        self.init(
            intervalMilliseconds: report.intervalMs, packetsReceived: report.packetsReceived,
            packetsLost: report.packetsLost, underruns: report.underruns,
            receiveTimeouts: report.receiveTimeouts, jitterMilliseconds: report.jitterMs,
            cushionMilliseconds: report.cushionMs, backlogMilliseconds: report.backlogMs,
            recovered: report.recovered)
    }
}

/// Why the pipeline refused a packet, handed to ``ReceiverListener/onError(_:)`` as it comes: the
/// datagram is not PMON, it announces more channels than the desk mixes, or its rate is not the one
/// the output opened at. The core keeps its texts to itself, so an error describes itself by its
/// case and values.
extension BandEarCore.ReceiveError: Error {}
