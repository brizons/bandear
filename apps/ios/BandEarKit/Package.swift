// swift-tools-version: 6.0
import PackageDescription

/// The parts of the iOS client that have nothing to do with iOS: the wire format the desk speaks,
/// the timing policy that decides when a packet is played, how a desk is found, and the session that
/// carries the musician's mix. Keeping them here means `swift test` proves them from the command line,
/// with no Xcode project and no simulator, and keeps the app shell thin enough to read in one
/// sitting.
let package = Package(
    name: "BandEarKit",
    platforms: [.iOS(.v16), .macOS(.v13)],
    products: [
        .library(name: "BandEarProtocol", targets: ["BandEarProtocol"]),
        .library(name: "BandEarPlayback", targets: ["BandEarPlayback"]),
        .library(name: "BandEarDiscovery", targets: ["BandEarDiscovery"]),
        .library(name: "BandEarSession", targets: ["BandEarSession"]),
        .library(name: "BandEarScreens", targets: ["BandEarScreens"]),
        .library(name: "BandEarCore", targets: ["BandEarCore"]),
    ],
    targets: [
        // The Rust core both phone apps share (`crates/bandear-core`), compiled into a static
        // xcframework and called through Swift bindings UniFFI generates. Both are build outputs:
        // run `scripts/build-core-ios.sh` before `swift test` or an Xcode build.
        .binaryTarget(name: "BandEarCoreFFI", path: "Frameworks/BandEarCoreFFI.xcframework"),
        .target(name: "BandEarCore", dependencies: ["BandEarCoreFFI"]),
        .testTarget(name: "BandEarCoreTests", dependencies: ["BandEarCore"]),
        // The desk's vocabulary: the core's message types under the names the app speaks. The core
        // reads the desk's messages and writes the phone's.
        .target(name: "BandEarProtocol", dependencies: ["BandEarCore"]),
        // The lock-free ring between the receive loop and the render callback. In C because iOS 16
        // has no `Synchronization.Atomic`, and C11 atomics need no package to fetch.
        .target(name: "BandEarRing"),
        // The phone's receiver: the socket, the receive thread and the output it plays through the
        // ring. When a packet is played, and how it is measured, is the shared core's
        // `ReceivePipeline`, called once per wake-up. The desk's playback settings and the `stats`
        // message come from the protocol; nothing there depends on it.
        .target(
            name: "BandEarPlayback", dependencies: ["BandEarProtocol", "BandEarRing", "BandEarCore"]),
        .testTarget(
            name: "BandEarPlaybackTests",
            dependencies: ["BandEarPlayback", "BandEarProtocol", "BandEarCore"]),
        .target(name: "BandEarDiscovery"),
        .testTarget(name: "BandEarDiscoveryTests", dependencies: ["BandEarDiscovery"]),
        // The control session joins the others: it connects to a desk found by discovery, speaks
        // the protocol over the connection, runs the musician's mix through it, and plays the audio
        // the desk sends by the session's settings.
        // It calls the shared core for the control messages it reads and writes, the reconnect
        // policy, the client-id rule and the mix itself (`MixModel`, with the saved mixes).
        .target(
            name: "BandEarSession",
            dependencies: ["BandEarProtocol", "BandEarDiscovery", "BandEarPlayback", "BandEarCore"]),
        .testTarget(
            name: "BandEarSessionTests",
            dependencies: ["BandEarSession", "BandEarProtocol", "BandEarDiscovery", "BandEarCore"],
            // The saved mixes as the app's own Swift encoder wrote them, which the phones keep.
            resources: [.copy("SavedMixFixtures")]),
        // What the screens say and show: the words, verbatim from the Android client, and the values
        // derived for display, read from the core's mix state. Pure, so `swift test` proves the
        // screens' rules without a simulator; it reads the other modules and nothing reads it but
        // the app.
        .target(
            name: "BandEarScreens",
            dependencies: [
                "BandEarProtocol", "BandEarDiscovery", "BandEarPlayback", "BandEarSession", "BandEarCore",
            ]),
        .testTarget(
            name: "BandEarScreensTests",
            dependencies: [
                "BandEarScreens", "BandEarProtocol", "BandEarDiscovery", "BandEarPlayback",
                "BandEarSession", "BandEarCore",
            ]),
    ]
)
