//! The BandEar phones' shared engine: the logic the iOS and Android clients used to implement
//! twice, written once.
//!
//! Pure Rust with no platform APIs and no clock: sockets, threads, audio output, persistence and
//! UI stay on each platform, which passes time in as plain numbers. The bindings live in
//! `bandear-ffi`, so this crate stays testable on the host with `cargo test -p bandear-core`.

pub mod arrival_timing;
pub mod control;
pub mod cushion;
pub mod hold_playout;
pub mod link_stats;
pub mod mix;
pub mod packet;
pub mod pcm16;
pub mod playback_latency;
pub mod receive_pipeline;
pub mod reorder_hold;
