//! The musician's mix: its state, the reducer over it, and the rules around it that both phones
//! share.
//!
//! - [`state`]: the mix ([`MixState`]), what the engineer set, and the strips and group faders
//!   derived from them.
//! - [`channel_layout`]: how the desk's channels become strips, and a stereo pair's balance.
//! - [`more_of_me`]: the lift, and the rules that turn More of me off.
//! - [`sync`]: how the desk's answers are matched to the mixes the phone sent.
//! - [`session`]: the events and the pure reducer over one session's mix ([`MixSession`]).
//! - [`outgoing`]: the `mix` the phone sends, and when it sends one.
//! - [`desk_reports`]: what the desk's messages report about the mix.
//! - [`bank`]: the saved mixes and the rules for saving, renaming and deleting them.
//! - [`model`]: the session and the saved mixes as one phone holds them ([`MixModel`]), one call
//!   per change, each saying what the phone must send, store or tell the musician.
//!
//! Ported from the iOS client's former `BandEarMix` module, a pure, mix-only value reducer whose
//! comments cited the Android lines each rule came from (the mix half of `MonitorReducer`,
//! `MonitorUiState` and `MonitorViewModel`, and the mix rules of `ControlChannel`, `MixSnapshot`
//! and `ClientMixState`, as they stood before this port); every Swift test is one Rust test here.
//! Both apps now run this module, and their copies were deleted. What a screen says (the notices, a
//! strip labelled "CH 1") stays with each platform, as do the tabs, the sockets, the storage and
//! the scheduling: these types carry the reasons and the data, never the words. The desk's messages
//! and the phone's `mix` are the control path's types ([`crate::control`]), reused as they are.

pub mod bank;
pub mod channel_layout;
pub mod desk_reports;
pub mod model;
pub mod more_of_me;
pub mod outgoing;
pub mod session;
pub mod state;
pub mod sync;

pub use bank::{MixBank, MixBankOutcome};
pub use channel_layout::{StereoPans, StripChannels};
pub use model::{MixBankChange, MixModel, MixRecall, MixUpdate, RecallOutcome};
pub use more_of_me::{MoreOfMeNotice, MoreOfMeState};
pub use session::{MixEvent, MixSession, RecallRefusal};
pub use state::{ChannelStrip, EngineerControls, GroupFader, GroupTag, MixState, NamedChannel};
pub use sync::MixSync;

#[cfg(test)]
mod tests;
