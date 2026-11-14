import BandEarProtocol
import Foundation
import Testing

import enum BandEarCore.ReceiveError
@testable import BandEarPlayback

// MARK: - Datagrams

/// The bytes of one audio datagram as the desk sends it (`apps/server/src/protocol.rs:476-498`).
private func datagram(
    sequence: UInt64, samples: [Int16], channels: UInt8 = 2, sampleRate: UInt32 = 48_000
) -> [UInt8] {
    var bytes = Array("PMON".utf8)
    bytes.append(1)
    bytes.append(channels)
    bytes += littleEndian(sampleRate)
    bytes += littleEndian(sequence)
    bytes += littleEndian(UInt16(samples.count))
    for sample in samples { bytes += littleEndian(UInt16(bitPattern: sample)) }
    return bytes
}

private func littleEndian<Value: FixedWidthInteger>(_ value: Value) -> [UInt8] {
    (0..<(Value.bitWidth / 8)).map { UInt8(truncatingIfNeeded: value >> ($0 * 8)) }
}

/// `frames` stereo frames, every sample `value`.
private func stereo(frames: Int, value: Int16 = 1_000) -> [Int16] {
    Array(repeating: value, count: frames * 2)
}

/// A 10 ms packet at 48 kHz, the size the tests play unless they say otherwise.
private let packetFrames = 480

/// The cushion a new stream starts with, in packets (`bandear-core`'s `START_PACKETS`).
private let startPackets = 2

// MARK: - Fakes

/// Hands the loop one prepared answer per receive: a datagram, or `nil` for a timeout. With nothing
/// left it runs `whenEmpty`, then times out unless that threw. `whileWaiting` runs inside every
/// receive, as something another thread does while the loop waits.
private final class FakeSource: DatagramSource {
    var arrivals: [[UInt8]?] = []
    var failure: (any Error)?
    var whenEmpty: (() throws -> Void)?
    var whileWaiting: (() -> Void)?

    func receive(into buffer: UnsafeMutableRawBufferPointer) throws -> Int? {
        whileWaiting?()
        if let failure { throw failure }
        guard !arrivals.isEmpty else {
            try whenEmpty?()
            return nil
        }
        guard let bytes = arrivals.removeFirst() else { return nil }
        buffer.copyBytes(from: bytes)
        return bytes.count
    }

    func wake() {}
    func close() {}
}

/// Notes everything the loop asks of the output, in order.
private final class FakeSink: PlaybackSink {
    enum Event: Equatable {
        case target(Int)
        case write(samples: Int)
    }

    let capacityFrames: Int
    /// What the output says it can take; `nil` takes every frame.
    var writableFrames: Int?
    /// What a target answers: the target itself unless a test says otherwise.
    var applies: (Int) -> Int? = { $0 }
    var underrunCount = 0
    var presentedFrame: Int64?
    var isPlaying = true

    private(set) var events: [Event] = []
    private(set) var writes: [[Int16]] = []
    private(set) var targets: [Int] = []
    private(set) var closes = 0

    init(capacityFrames: Int) { self.capacityFrames = capacityFrames }

    func write(_ samples: UnsafeBufferPointer<Int16>) -> Int {
        events.append(.write(samples: samples.count))
        writes.append(Array(samples))
        return samples.count / 2
    }

    func setTargetFrames(_ frames: Int) -> Int? {
        events.append(.target(frames))
        targets.append(frames)
        return applies(frames)
    }

    func close() { closes += 1 }
}

private struct Open: Equatable {
    let sampleRate: Int
    let packetFrames: Int
}

private struct OpenFailed: Error, Equatable {}

private final class FakeFactory: PlaybackSinkFactory {
    let sink: any PlaybackSink
    /// How many opens fail before one succeeds.
    var failures = 0
    private(set) var opens: [Open] = []

    init(sink: any PlaybackSink) { self.sink = sink }

    func open(sampleRate: Int, packetFrames: Int) throws -> any PlaybackSink {
        if failures > 0 {
            failures -= 1
            throw OpenFailed()
        }
        opens.append(Open(sampleRate: sampleRate, packetFrames: packetFrames))
        return sink
    }
}

private final class FakeClock: MonotonicClock {
    var nowNanoseconds: UInt64 = 0

    func set(milliseconds: Int64) { nowNanoseconds = UInt64(milliseconds) * 1_000_000 }
}

private final class Heard: ReceiverListener {
    private(set) var stats: [ReceiverStats] = []
    private(set) var reports: [LinkStatsReport] = []
    private(set) var errors: [any Error] = []
    private(set) var stops = 0

    var receiveErrors: [ReceiveError] { errors.compactMap { $0 as? ReceiveError } }

    func onStats(_ stats: ReceiverStats) { self.stats.append(stats) }
    func onLinkStats(_ report: LinkStatsReport) { reports.append(report) }
    func onError(_ error: any Error) { errors.append(error) }
    func onStopped() { stops += 1 }
}

private struct SocketFailed: Error, Equatable {}

/// A loop with every collaborator faked, started at 0 ms, driven one wake-up at a time.
private final class Rig {
    let source = FakeSource()
    let sink: FakeSink
    let factory: FakeFactory
    let controls: PlayerControls
    let heard = Heard()
    let clock = FakeClock()
    let loop: ReceiveLoop

    init(_ settings: PlayerSettings = PlayerSettings(), capacityFrames: Int = 24_000) {
        sink = FakeSink(capacityFrames: capacityFrames)
        factory = FakeFactory(sink: sink)
        controls = PlayerControls(settings)
        loop = ReceiveLoop(
            source: source, sinkFactory: factory, controls: controls, listener: heard,
            clock: clock)
    }

    /// One wake-up at `milliseconds` on which `bytes` arrived.
    func arrive(_ bytes: [UInt8], at milliseconds: Int64) throws {
        clock.set(milliseconds: milliseconds)
        source.arrivals = [bytes]
        try loop.step()
    }

    /// One wake-up at `milliseconds` on which packet `sequence` arrived, 10 ms of stereo at 48 kHz.
    func arrive(sequence: UInt64, at milliseconds: Int64, value: Int16 = 1_000) throws {
        try arrive(
            datagram(sequence: sequence, samples: stereo(frames: packetFrames, value: value)),
            at: milliseconds)
    }

    /// One wake-up at `milliseconds` on which nothing arrived.
    func timeOut(at milliseconds: Int64) throws {
        clock.set(milliseconds: milliseconds)
        source.arrivals = [nil]
        try loop.step()
    }
}

// MARK: - Tests

/// The receive loop's own part: the socket, the clock, the output and the listener around the
/// shared core's `ReceivePipeline`. The rules the core applies between them (the hold, the
/// cushion, the timing, local protection, the link report) are tested in `bandear-core`, ported
/// from this suite and the Android client's; these cases pin what the loop hands the core and what
/// it does with each answer.
@Suite("Receive loop")
struct ReceiveLoopTests {

    // MARK: Opening the output

    @Test func theFirstPacketOpensTheOutputAtItsFormatThenPrimesAndPlaysIt() throws {
        let rig = Rig()

        try rig.arrive(
            datagram(sequence: 1, samples: stereo(frames: 441), sampleRate: 44_100), at: 0)

        #expect(rig.factory.opens == [Open(sampleRate: 44_100, packetFrames: 441)])
        // The answer to the open carries the packet that asked for it: the cushion first, so the
        // output primes on it, then the packet.
        #expect(rig.sink.events == [.target(startPackets * 441), .write(samples: 882)])
        #expect(rig.heard.stats.count == 1)
    }

    @Test func laterPacketsPlayThroughTheOutputTheFirstOpened() throws {
        let rig = Rig()

        try rig.arrive(sequence: 1, at: 0)
        try rig.arrive(sequence: 2, at: 10)
        try rig.arrive(sequence: 3, at: 20)

        #expect(rig.factory.opens.count == 1)
        #expect(rig.sink.writes.count == 3)
    }

    @Test func anOutputThatFailsToOpenIsAnErrorAndTheNextPacketOpensItAgain() throws {
        let rig = Rig()
        rig.factory.failures = 1

        try rig.arrive(sequence: 1, at: 0)
        #expect(rig.heard.errors.count == 1)
        #expect(rig.heard.errors.first is OpenFailed)
        #expect(rig.sink.writes.isEmpty)
        #expect(rig.heard.stats.isEmpty)

        try rig.arrive(sequence: 2, at: 10)
        #expect(rig.factory.opens.count == 1)
        #expect(rig.sink.writes.count == 1)
    }

    /// 30 ms at 48 kHz is three packets of 480 frames, but an output holding two packets cannot
    /// queue three: the cushion is held to what it holds.
    @Test func theCushionIsSizedToTheCapacityTheOutputOpenedWith() throws {
        let rig = Rig(
            PlayerSettings(playbackBuffer: .fixed(milliseconds: 30)),
            capacityFrames: 2 * packetFrames)

        try rig.arrive(sequence: 1, at: 0)

        #expect(rig.sink.targets == [2 * packetFrames])
    }

    // MARK: Writing

    /// The samples cross the binding as little-endian bytes and land in the ring as the desk sent
    /// them: values whose two bytes differ, so a swapped byte order would show.
    @Test func aDatagramsSamplesReachTheRingAsSent() throws {
        let frames = 4
        let capacity = RingPlaybackSink.capacityFrames(sampleRate: 48_000, packetFrames: frames)
        let ring = RingPlaybackSink(capacityFrames: capacity)
        let first: [Int16] = [0x1234, -0x1234, 1, -1, .max, .min, 0x0102, 0x0201]
        let second: [Int16] = [-300, 300, 0x7F00, 0x00FF, -2, 2, 0, 0x4000]
        let rig = Rig(PlayerSettings(serverOwnsMix: true))
        let loop = ReceiveLoop(
            source: rig.source, sinkFactory: FakeFactory(sink: ring), controls: rig.controls,
            listener: rig.heard, clock: rig.clock)

        // Two packets: the ring primes on the start cushion of two before it plays.
        for (sequence, samples) in [first, second].enumerated() {
            rig.source.arrivals = [datagram(sequence: UInt64(sequence + 1), samples: samples)]
            try loop.step()
        }
        var left = [Float](repeating: .nan, count: 2 * frames)
        var right = [Float](repeating: .nan, count: 2 * frames)
        left.withUnsafeMutableBufferPointer { left in
            right.withUnsafeMutableBufferPointer { right in
                ring.render(
                    left: left.baseAddress!, right: right.baseAddress!, frameCount: 2 * frames)
            }
        }

        let sent = first + second
        #expect(rig.heard.errors.isEmpty)
        #expect(left == stride(from: 0, to: sent.count, by: 2).map { Float(sent[$0]) / 32_768 })
        #expect(right == stride(from: 1, to: sent.count, by: 2).map { Float(sent[$0]) / 32_768 })
    }

    /// A write's own bytes sit on an `Int16` boundary and are read in place; bytes that do not, as
    /// a `Data` slice's may not, are copied first, and still reach the output whole.
    @Test func bytesOnOrOffTheSampleBoundaryReachTheOutputWhole() throws {
        let sink = FakeSink(capacityFrames: 64)
        // Long enough for `Data` to keep the bytes on the heap, where a slice shares them.
        let samples = (0..<20).map { Int16(truncatingIfNeeded: 0x1234 &* ($0 + 1) &- 0x7000) }
        let bytes = Data([0xAA] + samples.flatMap { littleEndian(UInt16(bitPattern: $0)) } + [0x55])
        var scratch: [Int16] = []

        ReceiveLoop.write(pcm16LittleEndian: Data(bytes[1..<41]), to: sink, scratch: &scratch)
        #expect(scratch.isEmpty, "a write's own storage is read in place")
        // One byte in, and one odd byte at the end, which is not a sample.
        ReceiveLoop.write(pcm16LittleEndian: bytes[1...], to: sink, scratch: &scratch)
        #expect(scratch.count == samples.count, "a slice one byte in is copied")

        #expect(sink.writes == [samples, samples])
    }

    @Test func onlyWhatTheOutputCanTakeIsWrittenAndTheRestIsDiscarded() throws {
        let rig = Rig()
        rig.sink.writableFrames = 100

        try rig.arrive(sequence: 1, at: 0)
        try rig.arrive(sequence: 2, at: 10)

        #expect(rig.sink.writes.map(\.count) == [200, 200])
        let stats = try #require(rig.heard.stats.last)
        // 960 samples sent, 200 taken, each time.
        #expect(stats.discardedSamples == 1_520)
        #expect(stats.packetsWithDiscardedSamples == 2)
    }

    // MARK: The controls

    @Test func theSettingsReachTheCoreFieldForField() {
        let fixed = PlayerSettings(
            volumePercent: 40, maxLevelPercent: 70, muted: true, serverOwnsMix: true,
            playbackBuffer: .fixed(milliseconds: 30), redundancyHoldPackets: 3)

        #expect(fixed.core.volumePercent == 40)
        #expect(fixed.core.maxLevelPercent == 70)
        #expect(fixed.core.muted)
        #expect(fixed.core.serverOwnsMix)
        #expect(fixed.core.playbackBuffer == .fixed(milliseconds: 30))
        #expect(fixed.core.redundancyHoldPackets == 3)
        #expect(PlayerSettings().core.playbackBuffer == .auto)
        #expect(!PlayerSettings().core.muted && !PlayerSettings().core.serverOwnsMix)
    }

    @Test func aControlChangedOnAnotherThreadAppliesFromTheNextWakeUp() throws {
        let rig = Rig()

        try rig.arrive(sequence: 1, at: 0, value: 1_000)
        rig.controls.update { $0.muted = true }
        try rig.arrive(sequence: 2, at: 10, value: 1_000)

        #expect(
            rig.sink.writes
                == [stereo(frames: packetFrames), stereo(frames: packetFrames, value: 0)])
    }

    // MARK: Timeouts and errors

    @Test func aTimeoutReleasesWhatTheHoldKeeps() throws {
        let rig = Rig(PlayerSettings(redundancyHoldPackets: 1))

        try rig.arrive(sequence: 1, at: 0)
        #expect(rig.sink.writes.isEmpty)
        try rig.timeOut(at: 250)

        #expect(rig.sink.writes.count == 1)
        #expect(rig.heard.errors.isEmpty)
    }

    @Test func anUndecodableDatagramIsReportedAndTheLoopGoesOn() throws {
        let rig = Rig()

        try rig.arrive(Array("not audio".utf8), at: 0)
        try rig.arrive(sequence: 1, at: 10)

        #expect(rig.heard.receiveErrors == [.undecodable(reason: .headerTooShort(byteCount: 9))])
        #expect(rig.heard.errors.count == 1)
        #expect(rig.sink.writes.count == 1)
    }

    /// An empty datagram is a datagram, not a timeout.
    @Test func anEmptyDatagramWhileTheSessionRunsIsAnError() throws {
        let rig = Rig()

        try rig.arrive([], at: 0)

        #expect(rig.heard.receiveErrors == [.undecodable(reason: .headerTooShort(byteCount: 0))])
    }

    // MARK: What the output says

    @Test func aDueReportCarriesTheCushionTheOutputAppliedAndTheFrameItPresented() throws {
        let rig = Rig()
        rig.sink.applies = { _ in 1_200 }

        try rig.arrive(sequence: 1, at: 0)
        rig.sink.presentedFrame = 240
        try rig.timeOut(at: 1_000)

        let report = try #require(rig.heard.reports.first)
        #expect(rig.heard.reports.count == 1)
        // 1 200 frames applied at 48 kHz, and 480 written less 240 presented.
        #expect(report.cushionMilliseconds == 25)
        #expect(report.backlogMilliseconds == 5)
        #expect(report.intervalMilliseconds == 1_000)
        #expect(report.packetsReceived == 1)
    }

    @Test func aReportCarriesTheOutputsUnderrunsAndTheTimeouts() throws {
        let rig = Rig()

        try rig.arrive(sequence: 1, at: 0)
        rig.sink.underrunCount = 3
        try rig.timeOut(at: 500)
        try rig.timeOut(at: 1_000)

        let report = try #require(rig.heard.reports.first)
        #expect(report.underruns == 3)
        #expect(report.receiveTimeouts == 1)
    }

    /// The core holds a report's interval to what the desk reads, so the longest it makes, a
    /// wake-up long after the startup grace, passes into the `stats` message unchanged.
    @Test func theLongestReportIntervalPassesIntoTheStatsMessageUnchanged() throws {
        let rig = Rig()

        try rig.timeOut(at: 600_000)

        let report = try #require(rig.heard.reports.first)
        #expect(report.intervalMilliseconds == Int64(LinkStats.maximumIntervalMilliseconds))
        #expect(LinkStats(report).intervalMilliseconds == LinkStats.maximumIntervalMilliseconds)
    }

    @Test func eachPlayedPacketReportsTheReceiversCounts() throws {
        let rig = Rig()

        try rig.arrive(sequence: 1, at: 1_000)
        // Packet 2 never comes.
        try rig.arrive(sequence: 3, at: 1_010)

        #expect(rig.heard.stats.count == 2)
        let stats = try #require(rig.heard.stats.last)
        #expect(
            stats
                == ReceiverStats(
                    packets: 2, lost: 1, sampleRate: 48_000, packetsPerSecond: 2 / 1.01,
                    interArrivalMilliseconds: 10, jitterMilliseconds: 0, discardedSamples: 0,
                    packetsWithDiscardedSamples: 0, playbackBacklogMilliseconds: nil,
                    outputPlaying: true))
    }

    @Test func eachPacketsStatsSayWhetherTheOutputWasPlaying() throws {
        let rig = Rig()

        try rig.arrive(sequence: 1, at: 0)
        rig.sink.isPlaying = false
        try rig.arrive(sequence: 2, at: 10)

        #expect(rig.heard.stats.map(\.outputPlaying) == [true, false])
    }

    @Test func aSecondOfAudioAdjustsTheCushionWithTheOutputsUnderrunCount() throws {
        let rig = Rig()
        rig.sink.underrunCount = 1

        // 100 packets of 10 ms: one second of audio, so one adjustment.
        for sequence in 1...100 {
            try rig.arrive(sequence: UInt64(sequence), at: Int64(sequence) * 10)
        }

        #expect(rig.sink.targets == [2 * packetFrames, 3 * packetFrames])
    }

    // MARK: Running

    @Test func theLoopRunsWhileTheFlagIsUp() throws {
        let rig = Rig()
        let running = RunningFlag(true)
        rig.source.arrivals = [
            datagram(sequence: 1, samples: stereo(frames: packetFrames)),
            datagram(sequence: 2, samples: stereo(frames: packetFrames)),
        ]
        rig.source.whenEmpty = { running.value = false }

        rig.loop.run(while: running)

        #expect(rig.sink.writes.count == 2)
        #expect(rig.heard.errors.isEmpty)
    }

    @Test func aSourceThatFailsWhileRunningIsReportedAndEndsTheLoop() throws {
        let rig = Rig()
        let running = RunningFlag(true)
        rig.source.failure = SocketFailed()

        rig.loop.run(while: running)

        #expect(rig.heard.errors.count == 1)
        #expect(rig.heard.errors.first is SocketFailed)
    }

    @Test func aSourceClosedByAStopIsNotAnError() throws {
        let rig = Rig()
        let running = RunningFlag(true)
        rig.source.whenEmpty = {
            running.value = false
            throw DatagramSourceError.closed
        }

        rig.loop.run(while: running)

        #expect(rig.heard.errors.isEmpty)
    }

    @Test func whatWakesAReceiveAfterAStopIsNeverRead() throws {
        let rig = Rig()
        let running = RunningFlag(true)
        // The stop's wake-up: the flag goes down while the loop waits, then an empty datagram.
        rig.source.arrivals = [[]]
        rig.source.whileWaiting = { running.value = false }

        rig.loop.run(while: running)

        #expect(rig.heard.errors.isEmpty)
        #expect(rig.source.arrivals.isEmpty)
    }

    @Test func theSinkIsClosedOnceWhenTheLoopEnds() throws {
        let rig = Rig()
        let running = RunningFlag(true)
        rig.source.arrivals = [datagram(sequence: 1, samples: stereo(frames: packetFrames))]
        rig.source.whenEmpty = { running.value = false }

        rig.loop.run(while: running)
        rig.loop.close()

        #expect(rig.sink.closes == 1)
    }
}
