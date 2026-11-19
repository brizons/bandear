import Foundation
import Testing

import BandEarCore

/// S8: what one `onWake` costs through the bindings in steady state, for a stereo 128-frame packet
/// at 48 kHz (2.67 ms of audio), with local protection on, a report every second and the cushion's
/// adjustment every second of audio: the core's `measure_one_wake_up`, plus the crossing. The
/// budget is 5 % of the packet, about 133 µs.
///
/// A measurement, so it runs only when asked, in release:
/// `BANDEAR_BENCH=1 swift test -c release --package-path apps/ios/BandEarKit --filter receivePipelineBenchmark`
@Test(
    .enabled(
        if: ProcessInfo.processInfo.environment["BANDEAR_BENCH"] == "1",
        "a measurement: run it with BANDEAR_BENCH=1 swift test -c release"))
func receivePipelineBenchmark() {
    let packetNs: UInt64 = 2_666_667
    let warmUp: UInt64 = 20_000
    let measured: UInt64 = 200_000

    let samples = (0..<Int16(PmonFixtures.frames * 2)).map { $0 &* 97 }
    let settings = PlayerSettings(
        volumePercent: 80, maxLevelPercent: 100, muted: false, serverOwnsMix: false,
        playbackBuffer: .auto, redundancyHoldPackets: 0)
    let output = OutputReadings(
        writableFrames: 24_000, underrunCount: 0, presentedFrame: 0, isPlaying: true)
    let pipeline = ReceivePipeline(startedNs: 0)
    var applied: Int64? = nil
    var dueAt = pipeline.reportDueAtMs()

    // One wake-up as the receive loop runs it, from its received bytes on; what the outcome asks
    // of the output is only noted, as a real output would only copy the bytes.
    func run(_ sequence: UInt64, _ bytes: Data) -> WakeOutcome {
        let nowNs = sequence * packetNs
        var outcome = pipeline.onWake(
            wake: Wake(
                startedNs: nowNs,
                outputAtStart: Int64(nowNs / 1_000_000) >= dueAt ? output : nil,
                appliedTargetFrames: applied, datagram: bytes, receivedNs: nowNs,
                outputAtReceive: output, settings: settings))
        applied = nil
        if outcome.openOutput != nil {
            outcome = pipeline.onOutputOpened(nowNs: nowNs, capacityFrames: 24_000, output: output)
        }
        for action in outcome.actions {
            if case .setTargetFrames(let frames) = action { applied = frames }
        }
        dueAt = outcome.reportDueAtMs
        return outcome
    }

    for sequence in 0..<warmUp {
        _ = run(sequence, PmonFixtures.datagram(sequence: sequence, samples: samples))
    }
    var elapsedNs: UInt64 = 0
    var reports = 0
    var written = 0
    for sequence in warmUp..<warmUp + measured {
        // The bytes are built outside the timing: the platform's socket delivers them.
        let bytes = PmonFixtures.datagram(sequence: sequence, samples: samples)
        let started = DispatchTime.now().uptimeNanoseconds
        let outcome = run(sequence, bytes)
        elapsedNs += DispatchTime.now().uptimeNanoseconds - started
        reports += outcome.report == nil ? 0 : 1
        for case .write(let bytes) in outcome.actions { written += bytes.count }
    }

    let perCallNs = Double(elapsedNs) / Double(measured)
    let budgetNs = Double(packetNs) * 0.05
    print(
        "S8 (Swift bindings): \(Int(perCallNs.rounded())) ns per wake-up over \(measured) wake-ups "
            + "(\(reports) reports, \(written) bytes written), "
            + String(format: "%.3f", perCallNs / Double(packetNs) * 100)
            + " % of a 2.67 ms packet; budget \(Int(budgetNs)) ns (5 %)")
    #expect(written == Int(measured) * PmonFixtures.frames * 4)
    #expect(perCallNs < budgetNs)
}
