import Foundation
import Testing

import BandEarCore

/// Drives the shared receive pipeline through the generated bindings the way the receive loop
/// will: a packet asks for the output, the output opens, later packets play through it, and a
/// timeout carries the link report. The pipeline's rules are tested in Rust
/// (`crates/bandear-core/src/receive_pipeline.rs`); these tests prove each call and type crosses
/// the binding and reads in Swift as the loop expects.
struct ReceivePipelineBindingTests {
    @Test func aPacketOpensTheOutputThenPlaysThroughIt() throws {
        let pipeline = ReceivePipeline(startedNs: 0)
        #expect(pipeline.reportDueAtMs() == 3_000)

        let first = pipeline.onWake(
            wake: PmonFixtures.wake(
                datagram: PmonFixtures.datagram(sequence: 1, samples: PmonFixtures.samples(seed: 0)),
                atMs: 0))
        #expect(
            first.openOutput
                == OutputFormat(sampleRate: 48_000, packetFrames: Int64(PmonFixtures.frames)))
        #expect(first.actions.isEmpty)

        let opened = pipeline.onOutputOpened(
            nowNs: 0, capacityFrames: 24_000, output: PmonFixtures.readings)
        #expect(
            opened.actions == [
                .setTargetFrames(frames: 2 * Int64(PmonFixtures.frames)),
                .write(pcm16LeBytes: PmonFixtures.littleEndian(PmonFixtures.samples(seed: 0))),
            ])
        #expect(opened.openOutput == nil)
        #expect(opened.reportDueAtMs == 1_000)

        let second = pipeline.onWake(
            wake: PmonFixtures.wake(
                datagram: PmonFixtures.datagram(sequence: 2, samples: PmonFixtures.samples(seed: 1)),
                atMs: 10))
        #expect(second.actions.count == 1)
        guard case .write(let bytes) = try #require(second.actions.first) else {
            Issue.record("the second packet's action is \(second.actions), not a write")
            return
        }
        // The byte order, read by hand: 0x0102, -2, Int16.min, Int16.max.
        #expect(Array(bytes.prefix(8)) == [0x02, 0x01, 0xFE, 0xFF, 0x00, 0x80, 0xFF, 0x7F])
        #expect(bytes == PmonFixtures.littleEndian(PmonFixtures.samples(seed: 1)))
        #expect(second.stats.map(\.packets) == [2])
        #expect(second.error == nil)

        // Nothing came for a while, and the report fell due.
        let timeout = pipeline.onWake(
            wake: PmonFixtures.wake(datagram: nil, atMs: 1_000, atStart: PmonFixtures.readings))
        #expect(timeout.actions.isEmpty)
        #expect(timeout.stats.isEmpty)
        #expect(timeout.error == nil)
        #expect(timeout.openOutput == nil)
        let report = try #require(timeout.report)
        #expect(report.packetsReceived == 2)
        #expect(report.packetsLost == 0)
        #expect(timeout.reportDueAtMs == 2_000)
    }

    @Test func anUndecodableDatagramIsAReceiveError() {
        let pipeline = ReceivePipeline(startedNs: 0)

        var notPmon = PmonFixtures.datagram(sequence: 1, samples: PmonFixtures.samples(seed: 0))
        notPmon[3] = UInt8(ascii: "X")
        #expect(
            pipeline.onWake(wake: PmonFixtures.wake(datagram: notPmon, atMs: 0)).error
                == .undecodable(reason: .unknownMagic(magic: Data("PMOX".utf8))))

        let short = Data(repeating: UInt8(ascii: "P"), count: 5)
        #expect(
            pipeline.onWake(wake: PmonFixtures.wake(datagram: short, atMs: 10)).error
                == .undecodable(reason: .headerTooShort(byteCount: 5)))
    }

    /// The ring's capacity and the channel limit, as the app computed them before the core did.
    @Test func thePlayersRoomAndTheChannelLimitAreTheCores() {
        // 500 ms at 48 kHz is 24 000 frames: 93.75 packets of 256, so 94 of them.
        #expect(playbackCapacityFrames(sampleRate: 48_000, packetFrames: 256) == 94 * 256)
        #expect(playbackCapacityFrames(sampleRate: 48_000, packetFrames: 240) == 24_000)
        #expect(playbackCapacityFrames(sampleRate: 44_100, packetFrames: 0) == 22_050)
        #expect(maximumMixChannels() == 32)
    }
}

/// PMON datagrams built by hand from the layout in `crates/bandear-core/src/packet.rs`, and the
/// wake-ups that carry them.
enum PmonFixtures {
    /// Stereo frames per packet: the S8 packet, 128 frames at 48 kHz.
    static let frames = 128

    /// An output that takes everything, has presented nothing, never ran dry, and plays.
    static let readings = OutputReadings(
        writableFrames: nil, underrunCount: 0, presentedFrame: nil, isPlaying: true)

    /// The packet plays as sent: the desk owns the mix.
    static let settings = PlayerSettings(
        volumePercent: 100, maxLevelPercent: 100, muted: false, serverOwnsMix: true,
        playbackBuffer: .auto, redundancyHoldPackets: 0)

    /// One datagram as the desk sends it: `PMON`, version 1, 2 channels, 48 000 Hz, the sequence,
    /// the sample count, then the samples, every number little endian.
    static func datagram(sequence: UInt64, samples: [Int16]) -> Data {
        var bytes = Data("PMON".utf8)
        bytes.append(1)
        bytes.append(2)
        append(UInt32(48_000), to: &bytes)
        append(sequence, to: &bytes)
        append(UInt16(samples.count), to: &bytes)
        bytes.append(littleEndian(samples))
        return bytes
    }

    /// A packet's interleaved samples: both extremes, a sign and a byte order to read, then a ramp
    /// that differs per `seed`.
    static func samples(seed: Int16) -> [Int16] {
        [0x0102, -2, .min, .max] + (4..<Int16(frames * 2)).map { $0 &* 97 &+ seed }
    }

    static func littleEndian(_ samples: [Int16]) -> Data {
        var bytes = Data(capacity: samples.count * 2)
        for sample in samples { append(sample, to: &bytes) }
        return bytes
    }

    /// A wake-up at `milliseconds`, with fresh readings after the receive.
    static func wake(datagram: Data?, atMs milliseconds: UInt64, atStart: OutputReadings? = nil)
        -> Wake
    {
        Wake(
            startedNs: milliseconds * 1_000_000, outputAtStart: atStart, appliedTargetFrames: nil,
            datagram: datagram, receivedNs: milliseconds * 1_000_000, outputAtReceive: readings,
            settings: settings)
    }

    private static func append<Value: FixedWidthInteger>(_ value: Value, to bytes: inout Data) {
        withUnsafeBytes(of: value.littleEndian) { bytes.append(contentsOf: $0) }
    }
}
