import BandEarCore
import BandEarMix
import Foundation

/// Where the phone keeps the mixes the musician saved, from one launch to the next.
///
/// The session reads and writes it through this port, so its rules are proved with an in-memory
/// fake and the device's storage stays in one adapter, ``UserDefaultsMixBankStore``. Android draws
/// the same seam (`MonitorStore`, `MonitorCollaborators.kt:48-54`), with saving and deleting on
/// the store; here those rules are ``MixBank``'s, so the store only keeps the list.
public protocol MixBankStore: AnyObject {
    /// The saved mixes, in the order they were saved; empty before the first.
    var savedMixes: [BandEarMix.SavedMix] { get set }
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

    public var savedMixes: [BandEarMix.SavedMix] {
        get {
            decodeSavedMixes(json: defaults.string(forKey: Self.key)).map(BandEarMix.SavedMix.init)
        }
        set {
            let mixes = newValue.map(BandEarCore.SavedMix.init)
            defaults.set(encodeSavedMixes(mixes: mixes), forKey: Self.key)
        }
    }
}

// The mix model keeps its levels as `Int` until it moves into the core; the core keeps them as
// `Int64`. An `Int` holds every `Int64` whole on the 64-bit devices the app runs on, so neither way
// changes a value.

extension BandEarMix.SavedMix {
    /// `mix` as the core read it.
    init(_ mix: BandEarCore.SavedMix) {
        let snapshot = mix.snapshot
        self.init(
            name: mix.name,
            snapshot: BandEarMix.MixSnapshot(
                channelGains: snapshot.channelGains.map { Int($0) },
                channelPans: snapshot.channelPans.map { Int($0) },
                channelMutes: snapshot.channelMutes, channelSolos: snapshot.channelSolos,
                groupLevels: snapshot.groupLevels.map { Int($0) },
                volumePercent: Int(snapshot.volumePercent),
                maxLevelPercent: Int(snapshot.maxLevelPercent), muted: snapshot.muted))
    }
}

extension BandEarCore.SavedMix {
    /// `mix` as the core writes it.
    init(_ mix: BandEarMix.SavedMix) {
        let snapshot = mix.snapshot
        self.init(
            name: mix.name,
            snapshot: BandEarCore.MixSnapshot(
                channelGains: snapshot.channelGains.map(Int64.init),
                channelPans: snapshot.channelPans.map(Int64.init),
                channelMutes: snapshot.channelMutes, channelSolos: snapshot.channelSolos,
                groupLevels: snapshot.groupLevels.map(Int64.init),
                volumePercent: Int64(snapshot.volumePercent),
                maxLevelPercent: Int64(snapshot.maxLevelPercent), muted: snapshot.muted))
    }
}
