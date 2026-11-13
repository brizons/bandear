# Shared engine (Rust core for iOS + Android)

Objective: the logic both mobile clients duplicate lives once, in a Rust crate both apps call through generated bindings; UI, sockets, threads, audio output and storage stay native. Branch `feat/shared-engine` (from `main` f95ee7b). Delivery: `ask-on-risk`, chain strategy not chosen yet (asked before the first pull request; commits stay on this branch until then).
Runners: `cargo test -p bandear-core` (core); `swift test --package-path apps/ios/BandEarKit` + `xcodebuild` (iOS); `gradle -p apps/android :app:testDebugUnitTest assembleDebug` with `ANDROID_HOME=~/Library/Android/sdk` (Android); `cargo test --workspace` (server and desktop unaffected).

## Specs

- **S1 Goal.** The user's request, verbatim: "Esto reduce el codigo ya que es un motor compartido para ambas apps, implementalo". Every unit listed in S3-S5 exists once, in Rust, and both apps call it; the Swift and Kotlin copies and their duplicated tests are deleted.
- **S2 Boundary.** Native on each platform: UI (SwiftUI, Compose), sockets and receive threads, audio output (AVAudioEngine with the C ring `BandEarRing` on iOS, AudioTrack on Android), audio session, discovery (Bonjour on iOS, UDP beacon on Android: different mechanisms, not duplicated), persistence (UserDefaults, SharedPreferences), WebSocket transports. Display copy stays native (assumption: keeps localization on each platform).
- **S3 Receive pipeline.** Rust owns, per received datagram in one call: PMON decode, reorder hold and hold playout, arrival timing, cushion controller, playback latency, link stats, Pcm16 local protection. Every existing test of these units on both platforms is ported to Rust and passes. Where the two platforms differ today (L3), the chosen behavior is recorded in the Log and pinned by a test.
- **S4 Control path.** Rust owns server-message parsing, client-message encoding (register, mix, stats), the reconnect policy, client-id validation and the mix-bank JSON format. Transports stay native.
- **S5 Mix model.** Rust owns the mix state and its reducer (mix state, more-of-me, mix banks, channel layout, outgoing mix). Done last: it changes how both UIs read state.
- **S6 Bindings and build.** UniFFI generates the Swift and Kotlin bindings. Crates: `crates/bandear-core` (pure Rust, no UniFFI, tested on the host) and `crates/bandear-ffi` (UniFFI, `staticlib` + `cdylib`). The server and desktop do not depend on either. iOS: an xcframework built by a script, consumed by BandEarKit; BandEarKit stays free of SwiftUI/UIKit/AVFoundation imports. Android: `cargo-ndk` builds `arm64-v8a`, `armeabi-v7a` and `x86_64` libraries into `jniLibs`, plus JNA; JVM unit tests load a host build. `release.yml` builds the Android app with the Rust library.
- **S7 Wire drift guard.** A server test decodes the server's own encoder output (`encode_audio_packet`) with the core's decoder; `bandear-core` is only a dev-dependency of the server. The server's production code is not changed.
- **S8 Performance.** One binding call per datagram on the receive thread; nothing from Rust runs on the iOS render thread. The per-call cost is measured and recorded; it must stay under 5% of one packet's duration.
- **S9 Code size.** Lines removed and added per platform are measured and reported (the user's claim in S1).

## Tasks

| ID | Specs | Route | Status | Evidence |
|----|-------|-------|--------|----------|
| T0 | S6 (crate skeleton, workspace member) | inline | done | |
| T1 | S3 (decode, Pcm16, reorder hold, hold playout, arrival timing, link stats accumulator), S7 | delegated, parallel with T2 (L7) | in progress | |
| T2 | S3 (cushion controller, playback latency) | delegated, parallel with T1 (L7) | in progress | |
| T3 | S3 (one-call pipeline facade), S8 | inline | pending | |
| T4 | S6 (FFI crate, xcframework and cargo-ndk scripts, CI), Risk item 5 | inline | pending, needs toolchain consent (L6) | |
| T5 | S2, S3, S6 (iOS adoption, delete Swift copies) | delegated, parallel with T6 | pending | |
| T6 | S2, S3, S6 (Android adoption, delete Kotlin copies) | delegated, parallel with T5 | pending | |
| T7 | S4 (core + both apps) | inline | pending | |
| T8 | S5 (core + both apps) | inline | pending | |
| T9 | S8, S9, independent verifier (Risk items 4, 5) | delegated verifier | pending | |

Forecast: well over 400 authored changed lines (core and tests about 3000, adoption mostly deletions), so the chain strategy is asked before the first pull request.

## Log

- **L1 (2026-11-12, user)** "Esto reduce el codigo ya que es un motor compartido para ambas apps, implementalo". Earlier in the same conversation: "para mobile es mejor tener un motor compartido en rust?" and "puede llegar a ser mas rapido y tener menos latencia?" (answer given: latency gains on Android would come from moving to AAudio, not from Rust; out of scope here).
- **L2 Map.** Duplicated and pure on both sides: PMON decode (`AudioDatagram.swift` / `PmonPacket.kt`), `ReorderHold`, `HoldPlayout`, arrival timing (`ArrivalTiming.swift` / inline in `PmonPlayer.kt:221-233`), `LinkStatsAccumulator` / `LinkStats`, `CushionController`, `PlaybackLatency`, `Pcm16Processor`, `ReconnectPolicy`, control JSON codecs (`ClientMessage.swift`, `ServerMessage.swift` / `ControlChannel.kt`), client identity validation, mix-bank JSON, the mix model and reducer (`BandEarMix` / `MonitorReducer.kt`, `ClientMixState.kt`, `MixSnapshot.kt`). Constants match across platforms (hold max 4 and restart at 256, cushion 1/4/2 adjusted every 1000 ms, link stats 1000/60000/3000 ms, reconnect max 5 s). The PMON layout is identical in `protocol.rs:476-495`, `PmonPacket.kt:16-41` and `AudioDatagram.swift:42-82`. iOS `*ParityTests.swift` name their Android counterparts, so they give the pairing.
- **L3 Platform differences to resolve in T1-T3.** iOS rejects a sample count not divisible by the channel count (`AudioDatagram.swift:84`); Android rejects a sample rate of 0 or less and reads the u32 rate as a signed Int (`PmonPacket.kt:39,46`); arrival timing starts unset on iOS while Android re-seeds while it is 0.0 (`ArrivalTiming.swift:22`); sequence is `Long` on Android and `UInt64` on iOS.
- **L4 Decisions.** (a) Sockets, threads and audio output stay native and Rust is a "datagram in, samples and reports out" state machine: UniFFI cannot express the generic `ReorderHold<T>`/`HoldPlayout<T>`, and calling each unit separately would cross the binding 5-10 times per packet. (b) On iOS the render callback only drains the C ring (`RingPlaybackSink.swift:84`), so the ring stays in C and no Rust runs at render time. (c) The server keeps its own `protocol.rs` (its serde types are `pub(crate)`, one-directional and import `mix`/`playback`/`redundancy`); S7 guards drift with a test instead, which keeps this feature out of the server's contract. (d) UniFFI lives in a separate crate so the server and desktop never pull it in.
- **L5 Parallel work.** A separate session (`desktop-gpui`, own worktree from `main`) moves the desktop UI from Tauri to GPUI. Shared files: the root `Cargo.toml` members and `Cargo.lock`; expect merge conflicts only there.
- **L7 Routing and porting rules (2026-11-12).** T1 and T2 run as two writers in isolated worktrees under `~/projects/bandear-worktrees/` (writer reasons: parallel units of several minutes each, and the parent's context backstop; they share one compilation unit, so not one tree). `HoldPlayout` calls the link stats accumulator, so the accumulator moved to T1; the `stats` message (`LinkStatsMessage.swift`) stays with the control codecs in T7. Porting source: the Swift units, whose `*ParityTests.swift` already port the Android tests case by case; every Swift test becomes one Rust test. L3 differences: the decoder refuses everything either platform refuses (zero channels, sample count not divisible by channels, sample rate 0 or above `i32::MAX`); everything else follows iOS (arrival timing unset until the first interval; sequences above `i64::MAX` refused), since iOS already documents those as deliberate deviations from Android.
- **L6 Toolchain on this Mac (2026-11-12).** Only the `aarch64-apple-darwin` Rust target is installed; no `cargo-ndk`; the Android SDK is at `~/Library/Android/sdk` without an NDK or `cmdline-tools`; Xcode 27.0; cargo 1.93. iOS and Android Rust targets, the NDK and `cargo-ndk` need the user's consent to download (T4).
