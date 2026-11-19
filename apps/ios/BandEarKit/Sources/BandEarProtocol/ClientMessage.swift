import BandEarCore

// The phone's messages, written by the shared core
// (`crates/bandear-core/src/control/client_message.rs`): `Registration.encodedJson()` for
// `register`, `mixMessage(command:)` for `mix`, `statsMessage(report:)` for `stats`. These are the
// core's own types, named here beside the desk's.

/// The `register` message: who is listening, and on which UDP port. Made checked, so it holds what
/// the desk stores: the name trimmed and capped at 64 Unicode scalars, a client id the desk would
/// discard dropped, each capability once; a UDP port of 0 is refused.
public typealias Registration = BandEarCore.Registration
/// The `mix` message: the master values, always, and each list left out when `nil`.
public typealias MixCommand = BandEarCore.MixCommand
/// What this client tells the desk it understands; `allClientCapabilities()` lists them all.
public typealias ClientCapability = BandEarCore.ClientCapability
/// Why a message could not be built: something the desk would refuse.
public typealias ClientMessageError = BandEarCore.ClientMessageError
