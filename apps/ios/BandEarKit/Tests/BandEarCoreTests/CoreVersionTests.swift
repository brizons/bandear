import Foundation
import Testing

import BandEarCore

/// Proves the Rust core is linked and callable through the generated bindings: the call crosses
/// into `libbandear_ffi` and returns the version written in `crates/bandear-core/Cargo.toml`.
@Test func coreVersionIsTheVersionOfTheRustCore() throws {
    #expect(coreVersion() == (try coreManifestVersion()))
}

/// `version = "..."` from `crates/bandear-core/Cargo.toml`, found from this file's place in the
/// repository (`apps/ios/BandEarKit/Tests/BandEarCoreTests/`).
private func coreManifestVersion() throws -> String {
    var repository = URL(fileURLWithPath: #filePath)
    for _ in 0..<6 { repository.deleteLastPathComponent() }
    let manifest = try String(
        contentsOf: repository.appendingPathComponent("crates/bandear-core/Cargo.toml"),
        encoding: .utf8)
    let prefix = "version = \""
    let line = try #require(manifest.split(separator: "\n").first { $0.hasPrefix(prefix) })
    return String(line.dropFirst(prefix.count).dropLast())
}
