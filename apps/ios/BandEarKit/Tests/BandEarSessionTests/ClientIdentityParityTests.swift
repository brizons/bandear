import BandEarSession
import Foundation
import Testing

import func BandEarCore.validClientId

/// The Android client's `ClientIdentityTest.kt`
/// (`apps/android/app/src/test/java/com/bandear/android/`): how a new client id is made. When a
/// stored one is kept or replaced is the shared core's rule (`chooseClientId(stored:generated:)`),
/// tested in `crates/bandear-core/src/control/client_identity.rs`.
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
}
