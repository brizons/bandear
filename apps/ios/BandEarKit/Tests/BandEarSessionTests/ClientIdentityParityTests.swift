import BandEarSession
import Foundation
import Testing

import func BandEarCore.validClientId

/// The Android client's `ClientIdentityTest.kt`
/// (`apps/android/app/src/test/java/com/bandear/android/`): how a new client id is made, and when a
/// stored one is kept or replaced. What counts as a client id (the desk's rule) is the shared
/// core's `validClientId(raw:)`, tested in `crates/bandear-core/src/control/client_identity.rs`.
@Suite("Client identity: Android parity")
struct ClientIdentityParityTests {
    @Test func aNewClientIdIsARandomUuidTheServerAccepts() {
        let first = ClientIdentity.newClientID()
        let second = ClientIdentity.newClientID()

        // Android compares with `UUID.fromString(first).toString()`, which writes it in lowercase.
        #expect(UUID(uuidString: first)?.uuidString.lowercased() == first)
        #expect(validClientId(raw: first) == first)
        #expect(first != second)
    }

    @Test func aStoredValidIdIsKeptAndNothingIsSaved() {
        var saved: [String] = []

        let id = ClientIdentity.resolveClientID(
            stored: "phone-1", generate: { "unused" }, save: { saved.append($0) })

        #expect(id == "phone-1")
        #expect(saved.isEmpty, "nothing new is stored")
    }

    @Test func withNoStoredIdANewOneIsGeneratedAndSaved() {
        var saved: [String] = []

        let id = ClientIdentity.resolveClientID(
            stored: nil, generate: { "fresh-id" }, save: { saved.append($0) })

        #expect(id == "fresh-id")
        #expect(saved == ["fresh-id"])
    }

    @Test func aCorruptedStoredIdIsReplacedAndSaved() {
        var saved: [String] = []

        let id = ClientIdentity.resolveClientID(
            stored: "not valid!", generate: { "fresh-id" }, save: { saved.append($0) })

        #expect(id == "fresh-id")
        #expect(saved == ["fresh-id"])
    }
}
