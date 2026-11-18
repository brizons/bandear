import BandEarCore

// The desk's messages, read by the shared core (`parseServerMessage(json:)`, in
// `crates/bandear-core/src/control/server_message.rs`): unknown types are kept as `unrecognized`,
// fields added after a message shipped are read leniently, and anything else unreadable is the
// `ServerMessageError` that says why. These are the core's own types, named here so the mix, the
// session and the screens keep speaking the desk's vocabulary through this module.

/// A control message from the desk.
public typealias ServerMessage = BandEarCore.ServerMessage
/// Why a message from the desk could not be read. `NotJson` also covers a known message whose
/// field holds the wrong type, so it says nothing about the connection the message came on.
public typealias ServerMessageError = BandEarCore.ServerMessageError
/// The `config` message: what the desk is capturing.
public typealias DeskConfiguration = BandEarCore.DeskConfiguration
/// One group in the desk's `config`.
public typealias ChannelGroup = BandEarCore.ChannelGroup
/// One source channel in the desk's `config`, in channel order: entry 0 is CH 1.
public typealias SourceChannel = BandEarCore.SourceChannel
/// What a source channel carries; a kind this build does not know reads as `other`.
public typealias ChannelKind = BandEarCore.ChannelKind
/// A whole mix as the desk reports it (`mix_ack`, `mix_state`).
public typealias MixReport = BandEarCore.MixReport
/// The cushion the desk set for this musician.
public typealias PlaybackBufferSetting = BandEarCore.PlaybackBufferSetting
/// An `error` reply, with the number of the `mix` it refused when that mix had one.
public typealias DeskError = BandEarCore.DeskError
