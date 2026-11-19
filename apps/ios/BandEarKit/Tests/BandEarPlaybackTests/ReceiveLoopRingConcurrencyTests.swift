import Foundation
import Testing

@testable import BandEarPlayback

// Regression test from the T5 verification: the real ReceiveLoop and the real RingPlaybackSink, with
// a render thread draining the ring while the loop writes, so the loop's producer-side
// `writableFrames` read (bandear_ring_queued) races the render side's consume.

private let probePacketFrames = 480

/// Frame `index` of the stream: never silent, its index recoverable from both channels.
private func probeFrame(_ index: Int) -> (left: Int16, right: Int16) {
    (Int16(index % 32_000 + 1), Int16((index / 32_000) % 32_000 + 1))
}

private func probeIndex(left: Int, right: Int) -> Int { (right - 1) * 32_000 + (left - 1) }

private func le<Value: FixedWidthInteger>(_ value: Value) -> [UInt8] {
    (0..<(Value.bitWidth / 8)).map { UInt8(truncatingIfNeeded: value >> ($0 * 8)) }
}

private func probeDatagram(sequence: UInt64, firstFrame: Int) -> [UInt8] {
    var bytes = Array("PMON".utf8)
    bytes.append(1)
    bytes.append(2)
    bytes += le(UInt32(48_000))
    bytes += le(sequence)
    bytes += le(UInt16(probePacketFrames * 2))
    for frame in firstFrame..<firstFrame + probePacketFrames {
        let value = probeFrame(frame)
        bytes += le(UInt16(bitPattern: value.left))
        bytes += le(UInt16(bitPattern: value.right))
    }
    return bytes
}

private final class ProbeSource: DatagramSource, @unchecked Sendable {
    private let datagrams: [[UInt8]]
    private var next = 0
    init(_ datagrams: [[UInt8]]) { self.datagrams = datagrams }
    func receive(into buffer: UnsafeMutableRawBufferPointer) throws -> Int? {
        guard next < datagrams.count else { return nil }
        let bytes = datagrams[next]
        next += 1
        buffer.copyBytes(from: bytes)
        return bytes.count
    }
    func wake() {}
    func close() {}
}

private final class ProbeFactory: PlaybackSinkFactory {
    let sink: RingPlaybackSink
    init(_ sink: RingPlaybackSink) { self.sink = sink }
    func open(sampleRate: Int, packetFrames: Int) throws -> any PlaybackSink { sink }
}

private final class ProbeListener: ReceiverListener, @unchecked Sendable {
    var last: ReceiverStats?
    var errors: [any Error] = []
    func onStats(_ stats: ReceiverStats) { last = stats }
    func onLinkStats(_ report: LinkStatsReport) {}
    func onError(_ error: any Error) { errors.append(error) }
    func onStopped() {}
}

/// Hands the loop to the producer thread, its only user, as the receive thread owns it in the app.
private final class LoopOwner: @unchecked Sendable {
    let loop: ReceiveLoop
    init(_ loop: ReceiveLoop) { self.loop = loop }
}

private final class Box: @unchecked Sendable {
    let lock = NSLock()
    var rendered = 0
    var mismatch: String?
    var stop = false
    func get<T>(_ body: (Box) -> T) -> T {
        lock.lock()
        defer { lock.unlock() }
        return body(self)
    }
}

private func probeThread(_ body: @escaping @Sendable () -> Void) -> DispatchSemaphore {
    let done = DispatchSemaphore(value: 0)
    Thread {
        body()
        done.signal()
    }.start()
    return done
}

@Suite("ReceiveLoop with a rendering ring")
struct ReceiveLoopRingConcurrencyTests {
    @Test(arguments: [1_021, 24_000])
    func everyFrameIsRenderedOnceInOrderOrCountedDiscarded(capacity: Int) throws {
        let packets = 6_000
        let datagrams = (0..<packets).map {
            probeDatagram(sequence: UInt64($0 + 1), firstFrame: $0 * probePacketFrames)
        }
        let sink = RingPlaybackSink(capacityFrames: capacity)
        let listener = ProbeListener()
        let controls = PlayerControls(PlayerSettings(serverOwnsMix: true))
        let owner = LoopOwner(
            ReceiveLoop(
                source: ProbeSource(datagrams), sinkFactory: ProbeFactory(sink), controls: controls,
                listener: listener, clock: SystemMonotonicClock()))
        let box = Box()

        let consumer = probeThread {
            var left = [Float](repeating: 0, count: 256)
            var right = [Float](repeating: 0, count: 256)
            var previous = -1
            var rendered = 0
            var turn = 0
            var mismatch: String?
            while mismatch == nil {
                let stopping = box.get { $0.stop }
                let frames = [128, 37, 256, 5][turn % 4]
                turn += 1
                left.withUnsafeMutableBufferPointer { l in
                    right.withUnsafeMutableBufferPointer { r in
                        sink.render(left: l.baseAddress!, right: r.baseAddress!, frameCount: frames)
                    }
                }
                var heardAny = false
                for offset in 0..<frames {
                    let l = Int(left[offset] * 32_768)
                    let r = Int(right[offset] * 32_768)
                    if l == 0 && r == 0 { continue }
                    heardAny = true
                    let index = probeIndex(left: l, right: r)
                    if index <= previous {
                        mismatch = "frame after \(previous) was \(index)"
                        break
                    }
                    previous = index
                    rendered += 1
                }
                if stopping && !heardAny { break }
                if turn % 64 == 0 { usleep(1_000) } else if !heardAny { sched_yield() }
            }
            box.get {
                $0.rendered = rendered
                $0.mismatch = mismatch
            }
        }

        let producer = probeThread {
            for _ in 0..<packets { try? owner.loop.step() }
            // The last frames may sit below the cushion target: a target of one plays them.
            _ = sink.setTargetFrames(1)
            usleep(50_000)
            box.get { $0.stop = true }
        }

        #expect(producer.wait(timeout: .now() + 120) == .success)
        #expect(consumer.wait(timeout: .now() + 120) == .success)
        let rendered = box.get { $0.rendered }
        let mismatch = box.get { $0.mismatch }
        let queued = capacity - (sink.writableFrames ?? 0)
        let stats = try #require(listener.last)
        let discardedFrames = Int(stats.discardedSamples) / 2
        print(
            "PROBE capacity=\(capacity) packets=\(stats.packets) rendered=\(rendered) discardedFrames=\(discardedFrames) packetsWithDiscards=\(stats.packetsWithDiscardedSamples) queued=\(queued) underruns=\(sink.underrunCount)"
        )
        #expect(listener.errors.isEmpty)
        #expect(mismatch == nil)
        #expect(stats.packets == Int64(packets))
        #expect(rendered + discardedFrames + queued == packets * probePacketFrames)
    }
}
