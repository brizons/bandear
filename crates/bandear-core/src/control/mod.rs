//! The control path: what the phone and the desk say to each other over the control WebSocket, and
//! the rules around that conversation that both phones share.
//!
//! - [`server_message`]: reading the desk's messages (`config`, `mix_ack`, `mix_state`,
//!   `playback`, `redundancy`, `error`), as leniently as the apps always read them.
//! - [`client_message`]: writing the phone's messages (`register`, `mix`, `stats`), normalised as
//!   the desk normalises them.
//! - [`reconnect_policy`]: how long the session waits before opening a lost connection again.
//! - [`client_identity`]: what counts as a client id the desk keeps.
//! - [`mix_bank`]: the JSON the saved mixes are kept in on the phone.
//!
//! Ported from the iOS client (`BandEarProtocol`, `BandEarSession`), whose parity tests already
//! port the Android client's case by case; every Swift test is one Rust test here. The transports
//! (the WebSocket, `UserDefaults`, `SharedPreferences`) stay native: these functions take and
//! return text.
//!
//! The JSON is written byte for byte as the iOS client's Foundation encoders write it (see the
//! private `json` module), and read with the leniency of its decoders: a field that is the wrong
//! type, a fraction where a whole number belongs, an unknown kind or type are each treated exactly
//! as the iOS client treats them.

mod json;

pub mod client_identity;
pub mod client_message;
pub mod mix_bank;
pub mod reconnect_policy;
pub mod server_message;
