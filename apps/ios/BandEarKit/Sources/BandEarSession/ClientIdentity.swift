import Foundation

import func BandEarCore.validClientId

/// This installation's client id: made once, kept in the ``MusicianStore`` from then on, and sent
/// in every registration. Ported from Android's `ClientIdentity.kt`; what counts as a client id is
/// the shared core's rule (`validClientId(raw:)`: 1 to 64 ASCII letters, digits and hyphens),
/// while making one and keeping it stay here.
///
/// The desk keys musicians by address, so a phone that comes back from a new one (DHCP after a
/// Wi-Fi blip) would otherwise appear as a second musician; with the id the desk moves the
/// musician's row, mix included, to the new address. It names this installation only: it is
/// random, carries nothing about the device or the musician, and is never shown.
public enum ClientIdentity {
    /// A new random client id: a UUID, which nobody on the network can guess and the desk keeps.
    /// Lowercase, as Android writes it.
    public static func newClientID() -> String {
        UUID().uuidString.lowercased()
    }

    /// The `stored` id when the desk keeps it. Otherwise (the first launch, or a value damaged on
    /// the device) a new one from `generate`, handed to `save` and returned (Android's
    /// `resolveClientId`).
    public static func resolveClientID(
        stored: String?, generate: () -> String = ClientIdentity.newClientID,
        save: (String) -> Void
    ) -> String {
        if let kept = validClientId(raw: stored) { return kept }
        let created = generate()
        save(created)
        return created
    }

    /// The client id `store` keeps, made and saved there the first time it is asked for.
    public static func clientID(in store: any MusicianStore) -> String {
        resolveClientID(stored: store.clientID) { store.clientID = $0 }
    }
}
