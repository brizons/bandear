// swift-tools-version: 6.0
import PackageDescription

/// The parts of the iOS client that have nothing to do with iOS: the wire format the desk speaks,
/// the timing policy that decides when a packet is played, how a desk is found, and the mix the
/// musician is editing. Keeping them here means `swift test` proves them from the command line,
/// with no Xcode project and no simulator, and keeps the app shell thin enough to read in one
/// sitting.
let package = Package(
    name: "BandEarKit",
    platforms: [.iOS(.v16), .macOS(.v13)],
    products: [
        .library(name: "BandEarProtocol", targets: ["BandEarProtocol"]),
        .library(name: "BandEarPlayback", targets: ["BandEarPlayback"]),
        .library(name: "BandEarDiscovery", targets: ["BandEarDiscovery"]),
        .library(name: "BandEarMix", targets: ["BandEarMix"]),
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
        .target(name: "BandEarProtocol"),
        .testTarget(name: "BandEarProtocolTests", dependencies: ["BandEarProtocol"]),
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
        // The mix speaks the desk's published vocabulary (its `config` and `mix_*` values), so it
        // depends on the protocol module; nothing in the protocol depends on the mix.
        .target(name: "BandEarMix", dependencies: ["BandEarProtocol"]),
        .testTarget(name: "BandEarMixTests", dependencies: ["BandEarMix", "BandEarProtocol"]),
        // The control session joins the other four: it connects to a desk found by discovery,
        // speaks the protocol over the connection, runs the mix through it, and plays the audio
        // the desk sends by the session's settings.
        .target(
            name: "BandEarSession",
            dependencies: ["BandEarProtocol", "BandEarMix", "BandEarDiscovery", "BandEarPlayback"]),
        .testTarget(
            name: "BandEarSessionTests",
            dependencies: [
                "BandEarSession", "BandEarProtocol", "BandEarMix", "BandEarDiscovery", "BandEarCore",
            ]),
        // What the screens say and show: the words, verbatim from the Android client, and the values
        // derived for display. Pure, so `swift test` proves the screens' rules without a simulator;
        // it reads the other modules and nothing reads it but the app.
        .target(
            name: "BandEarScreens",
            dependencies: [
                "BandEarProtocol", "BandEarMix", "BandEarDiscovery", "BandEarPlayback", "BandEarSession",
            ]),
        .testTarget(
            name: "BandEarScreensTests",
            dependencies: [
                "BandEarScreens", "BandEarProtocol", "BandEarMix", "BandEarDiscovery", "BandEarPlayback",
                "BandEarSession",
            ]),
    ]
)
