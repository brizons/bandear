import BandEarCore
import Foundation

/// Where the phone keeps the mixes the musician saved, from one launch to the next.
///
/// The session reads and writes it through this port, so its rules are proved with an in-memory
/// fake and the device's storage stays in one adapter, ``UserDefaultsMixBankStore``. Android draws
/// the same seam (`MonitorStore`, `MonitorCollaborators.kt:48-54`), with saving and deleting on
/// the store; here those rules are the shared core's `MixModel`'s, so the store only keeps the list.
public protocol MixBankStore: AnyObject {
    /// The saved mixes, in the order they were saved; empty before the first.
    var savedMixes: [SavedMix] { get set }
}

/// The store the app uses: `UserDefaults`, where Android keeps the same list in
/// `SharedPreferences`, written as the same JSON (`MixBankStore.kt:22-26,74-114`). The JSON is the
/// shared core's (`encodeSavedMixes(mixes:)`, `decodeSavedMixes(json:)`), written byte for byte as
/// this app's own Swift encoder wrote it before (`SavedMixCompatibilityTests`); where it is kept
/// stays here.
///
/// Reading never fails: a list that is missing or cannot be read is no saved mix, and an entry that
/// cannot be read is skipped with every other entry kept, because a saved mix can sit on a phone
/// for weeks and may have been written by an older version of the app.
public final class UserDefaultsMixBankStore: MixBankStore {
    /// Where the list is kept: Android's `banks` key. Changing it forgets every mix saved so far.
    private static let key = "bandear.banks"

    private let defaults: UserDefaults

    public init(_ defaults: UserDefaults) {
        self.defaults = defaults
    }

    public var savedMixes: [SavedMix] {
        get { decodeSavedMixes(json: defaults.string(forKey: Self.key)) }
        set { defaults.set(encodeSavedMixes(mixes: newValue), forKey: Self.key) }
    }
}
