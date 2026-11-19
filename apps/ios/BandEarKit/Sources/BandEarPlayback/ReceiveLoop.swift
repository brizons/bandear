import BandEarCore
import BandEarProtocol
import Foundation

/// A monotonic clock, read in nanoseconds: Android's `System.nanoTime()`.
///
/// The receive loop hands the shared core every instant it needs in nanoseconds of this clock, and
/// the core counts milliseconds of the same reading, as Android divides `nanoTime` by a million.
/// Injected, so the loop's timing is tested without waiting.
public protocol MonotonicClock {
    var nowNanoseconds: UInt64 { get }
}

extension MonotonicClock {
    /// ``nowNanoseconds`` in whole milliseconds.
    public var nowMilliseconds: Int64 { Int64(nowNanoseconds / 1_000_000) }
}

/// The system's monotonic clock, which does not move while the device sleeps, as Android's
/// `nanoTime` does not.
public struct SystemMonotonicClock: MonotonicClock, Sendable {
    public init() {}

    public var nowNanoseconds: UInt64 { DispatchTime.now().uptimeNanoseconds }
}

/// The receive loop: one wake-up per ``step()``, from the socket to the output.
///
/// The rules between the two are the shared core's: `BandEarCore.ReceivePipeline`, the Rust port
/// of the Android client's `PmonPlayer.receiveLoop` (`PmonPlayer.kt:124-357`), which decodes the
/// packet, holds it for the desk's delayed copies, times it, protects it, sizes the cushion,
/// measures the backlog and keeps the link report. This loop owns what the core cannot: the
/// ``DatagramSource``, the clock, the ``PlaybackSink`` and the ``ReceiverListener``. It crosses
/// into the core once per wake-up, twice on the wake-up that opens the output.
///
/// Each wake-up, in order:
/// 1. The clock, and the output's readings when one is open and a link report is due (the core
///    says when, in milliseconds of the same clock).
/// 2. One receive, which answers a datagram or times out. When the session flag went down while
///    it waited, the wake-up ends here: the datagram that woke it for the stop
///    (``DatagramSource/wake()``) never reaches the core, so it is never an error.
/// 3. The clock again, the output's readings, and one snapshot of the ``PlayerControls``, handed to
///    the core with the datagram (or the timeout) and what the output applied of the last cushion.
/// 4. The core's answer, acted on in its order: the link report to the listener, the writes and
///    cushion targets to the output, the stats after each packet played, then the error that ended
///    the wake-up early, if one did.
/// 5. When the first packet asks for the output, the factory opens it at that packet's rate and
///    size, and the core goes on with the packet in its answer to the open (or drops it when the
///    open failed, which is reported, and asks again with the next packet).
///
/// As on Android, a packet that cannot be played is reported (``ReceiverListener/onError(_:)``,
/// with a `BandEarCore.ReceiveError`) and ends only its own wake-up; only the socket failing ends
/// the loop.
///
/// Single-threaded, on the receive thread. One receive buffer is kept for the loop's life; the
/// datagram handed to the core is the one copy per receive.
public final class ReceiveLoop {
    /// The largest UDP payload over IPv4, the size of Android's receive buffer.
    public static let maximumDatagramBytes = 65_507

    private let source: any DatagramSource
    private let sinkFactory: any PlaybackSinkFactory
    private let controls: PlayerControls
    private let listener: any ReceiverListener
    private let clock: any MonotonicClock

    private let pipeline: ReceivePipeline
    private var receiveBuffer = [UInt8](repeating: 0, count: ReceiveLoop.maximumDatagramBytes)
    /// Samples for a write whose bytes do not sit on an `Int16` boundary (see
    /// ``write(pcm16LittleEndian:to:scratch:)``); grows to the largest such write, then stays.
    private var scratch: [Int16] = []

    private var sink: (any PlaybackSink)?
    /// The first instant, in milliseconds, at which a wake-up reads the output for a report.
    private var reportDueAtMilliseconds: Int64
    /// What the output answered to the last cushion target, for the next wake-up.
    private var appliedTargetFrames: Int64?

    /// A loop that reads `source` and plays through an output `sinkFactory` opens at the first
    /// packet, by the `controls`, telling `listener`, timed by `clock` from now.
    public init(
        source: any DatagramSource, sinkFactory: any PlaybackSinkFactory,
        controls: PlayerControls, listener: any ReceiverListener, clock: any MonotonicClock
    ) {
        self.source = source
        self.sinkFactory = sinkFactory
        self.controls = controls
        self.listener = listener
        self.clock = clock
        pipeline = ReceivePipeline(startedNs: clock.nowNanoseconds)
        reportDueAtMilliseconds = pipeline.reportDueAtMs()
    }

    /// Steps while `running` is up, then closes the output. The socket failing ends it too, and is
    /// reported while the session still runs; a socket closed by a stop is not an error.
    public func run(while running: RunningFlag) {
        defer { close() }
        do {
            while running.value { try step(while: running) }
        } catch {
            if running.value { listener.onError(error) }
        }
    }

    /// One wake-up. Throws only when the source does: the loop is over.
    ///
    /// With `running`, a wake-up whose receive answers after the flag went down ends there,
    /// whatever it received; an empty datagram from anyone else while the session runs is a packet
    /// like any other, and an error, as on Android.
    public func step(while running: RunningFlag? = nil) throws {
        let startedNs = clock.nowNanoseconds
        // The output is read for a report only once one is due; one due while the output is open
        // waits for this reading.
        let outputAtStart = sink.flatMap { sink in
            Self.milliseconds(startedNs) >= reportDueAtMilliseconds ? Self.readings(of: sink) : nil
        }
        let byteCount = try receiveBuffer.withUnsafeMutableBytes { try source.receive(into: $0) }
        // Stopped while waiting: what woke the receive is not audio to play.
        if let running, !running.value { return }
        let datagram = byteCount.map { count in
            Data(receiveBuffer[..<min(max(count, 0), receiveBuffer.count)])
        }
        let wake = Wake(
            startedNs: startedNs, outputAtStart: outputAtStart,
            appliedTargetFrames: appliedTargetFrames, datagram: datagram,
            receivedNs: clock.nowNanoseconds, outputAtReceive: sink.map(Self.readings(of:)),
            // Everything this wake-up plays by, read together once.
            settings: controls.snapshot.core)
        appliedTargetFrames = nil
        var outcome = pipeline.onWake(wake: wake)
        while true {
            act(on: outcome)
            guard let format = outcome.openOutput else { break }
            outcome = open(format)
        }
    }

    /// Stops the output and lets it go, once; nothing is written after this.
    public func close() {
        sink?.close()
        sink = nil
    }

    /// The core's answer, in its order: the report, the output's actions, the stats, the error.
    private func act(on outcome: WakeOutcome) {
        if let report = outcome.report { listener.onLinkStats(LinkStatsReport(report)) }
        for action in outcome.actions {
            // Actions come only once the output is open.
            guard let sink else { break }
            switch action {
            case .write(let bytes):
                Self.write(pcm16LittleEndian: bytes, to: sink, scratch: &scratch)
            case .setTargetFrames(let frames):
                // The cushion is never more than the ring holds, so it fits an `Int`.
                appliedTargetFrames = Int64(sink.setTargetFrames(Int(frames)) ?? 0)
            }
        }
        for stats in outcome.stats { listener.onStats(ReceiverStats(stats)) }
        if let error = outcome.error { listener.onError(error) }
        reportDueAtMilliseconds = outcome.reportDueAtMs
    }

    /// Opens the output the core asked for and lets it go on, or reports why it could not open.
    private func open(_ format: OutputFormat) -> WakeOutcome {
        do {
            // A rate the core accepted is at most `Int32.max`, and the frames are a datagram's.
            let opened = try sinkFactory.open(
                sampleRate: Int(format.sampleRate), packetFrames: Int(format.packetFrames))
            sink = opened
            return pipeline.onOutputOpened(
                nowNs: clock.nowNanoseconds, capacityFrames: Int64(opened.capacityFrames),
                output: Self.readings(of: opened))
        } catch {
            listener.onError(error)
            return pipeline.onOutputFailed()
        }
    }

    /// What `sink` says about itself, read in one go.
    private static func readings(of sink: any PlaybackSink) -> OutputReadings {
        OutputReadings(
            writableFrames: sink.writableFrames.map(Int64.init),
            underrunCount: Int64(sink.underrunCount), presentedFrame: sink.presentedFrame,
            isPlaying: sink.isPlaying)
    }

    /// Hands `bytes`, interleaved PCM16 laid out little endian as the core sends them, to `sink`
    /// without converting sample by sample: on a little-endian CPU, which every Apple device has,
    /// the bytes already are the samples. They are read in place when they sit on an `Int16`
    /// boundary, which a write's own storage does, and copied into `scratch` in one go otherwise.
    /// A trailing odd byte is not a sample and is ignored.
    static func write(
        pcm16LittleEndian bytes: Data, to sink: any PlaybackSink, scratch: inout [Int16]
    ) {
        #if !_endian(little)
            #error("BandEarKit reads the core's PCM16 bytes as native samples: little endian only")
        #endif
        bytes.withUnsafeBytes { raw in
            let count = raw.count / MemoryLayout<Int16>.stride
            guard count > 0, let base = raw.baseAddress else { return }
            let samples = UnsafeRawBufferPointer(
                start: base, count: count * MemoryLayout<Int16>.stride)
            if Int(bitPattern: base) % MemoryLayout<Int16>.alignment == 0 {
                _ = samples.withMemoryRebound(to: Int16.self) { sink.write($0) }
                return
            }
            if scratch.count < count { scratch = [Int16](repeating: 0, count: count) }
            scratch.withUnsafeMutableBufferPointer { buffer in
                let region = UnsafeMutableBufferPointer(rebasing: buffer[..<count])
                UnsafeMutableRawBufferPointer(region).copyMemory(from: samples)
                _ = sink.write(UnsafeBufferPointer(region))
            }
        }
    }

    private static func milliseconds(_ nanoseconds: UInt64) -> Int64 {
        Int64(nanoseconds / 1_000_000)
    }
}

extension PlayerSettings {
    /// These settings as the core plays by them, field for field.
    var core: BandEarCore.PlayerSettings {
        BandEarCore.PlayerSettings(
            volumePercent: Int64(volumePercent), maxLevelPercent: Int64(maxLevelPercent),
            muted: muted, serverOwnsMix: serverOwnsMix, playbackBuffer: playbackBuffer,
            redundancyHoldPackets: Int64(redundancyHoldPackets))
    }
}
