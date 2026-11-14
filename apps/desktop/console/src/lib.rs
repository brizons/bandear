//! The BandEar desktop console's model, whatever draws it: the web console's `lib/` (state,
//! controller, lanes, values, formats, routes) in Rust, with no window and no thread, so every rule
//! is tested with plain `#[test]`s.
//!
//! # The contract a screen crate builds on
//!
//! A screen crate (`bandear-desktop-<section>`) depends on this crate and on the kit, never on the
//! app or another screen. Everything it needs from the console is here:
//!
//! - **Reading the state.** [console::Console::state] gives the [state::ConsoleState]: the engine
//!   (`engine.phase`, `engine.status`, `engine.run`), the musicians in the status's order with their
//!   mixes nested, `selected` and `departed`, the audio choices and notices, the connection, and
//!   every list the desk keeps (presets, sessions, backups, strips, recording settings, takes...).
//!   The selectors on it ([state::ConsoleState::view_mix], [state::ConsoleState::channel_strip],
//!   [state::ConsoleState::is_live_run], ...) are the web console's. Read it and let go: never hold
//!   the borrow across an `await` or a dispatch.
//! - **Changing it.** [console::Console::dispatch] applies a [state::Action] (an edit, an answer, a
//!   pending change) and answers whether anything changed. Every action the web console had is
//!   there, every pending kind included; whole-value edits are stamped ([state::Stamped]) so their
//!   answers settle exactly them.
//! - **Asking the desk.** A flow is a future run with [console::Console::spawn]. In it,
//!   [console::Console::call] sends a [command::Command] (all 67, typed) and resolves to what the
//!   answer carries (the desk's own DTOs, re-exported in [types]) or the refusal in the desk's
//!   words; [console::Console::run] gives the raw [command::Reply]. The command goes out tagged with
//!   the handle's [command::Owner] ([console::Console::for_owner]) and its reply wakes exactly the
//!   flow that asked. [console::Console::sleep] waits on a timer the same way. The web console's
//!   `async` code ports line for line, and stays deterministic under test.
//! - **Lanes.** [coalesce::Lanes] is the per-key coalescer (C27): one send in flight per key, the
//!   latest value waiting, `idle(key)` to wait for a key to be quiet. [console::Console::in_turn] is
//!   the serial desk lane (C29) that channel and group edits, session loads, backups, buffers,
//!   redundancy and processing resets, copies and presets share across screens.
//!   [console::Console::hold_sends], [console::Console::holding] and
//!   [console::Console::on_sends_released] are the session-load hold (C31): a lane skips its send
//!   while holding, and sends what waits when released.
//! - **The engine's lifecycle and the shared loads.** [console::Console::connect],
//!   [console::Console::start], [console::Console::stop], [console::Console::apply_audio],
//!   [console::Console::set_audio], [console::Console::switch_device],
//!   [console::Console::load_devices], [console::Console::select] and the loaders
//!   ([console::Console::read_status], [console::Console::load_presets], ...) are the console's own,
//!   and any screen may run them.
//! - **Events.** Status and mix changes reach the state ([console::Console::event]);
//!   [console::Console::subscribe] hears every action that changed it, as the web store's
//!   subscribers did. Meters skip the state (D3): [console::Console::meters] is the latest of the
//!   live run for the views, and [console::Console::on_meters] hears each.
//!
//! What the app does with the console is in [console::Effect]: it runs each command on a background
//! thread through [desk::Desk] (implemented for the core's `DesktopState`), starts the timers,
//! listens to the pump, sets the window's title, and hands each outcome back.
//!
//! With the `testing` feature, `testing` has a simulated desk ([testing::FakeDesk]) and a harness
//! that drives a console with it ([testing::Harness]), for the screens' own tests.

pub mod chips;
pub mod command;
pub mod demo_source;
pub mod desk;
pub mod engine_config;
pub mod exec;
pub mod format;
pub mod mix;
pub mod router;
pub mod scheduling;
pub mod slider;
pub mod state;
pub mod values;
pub mod wire;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

/// The desk's and the engine's types that commands and answers carry, so a screen crate names them
/// through the console alone.
pub mod types {
    pub use bandear_desktop_core::backups::MixBackupSummary;
    pub use bandear_desktop_core::commands::processing::{
        ChannelProcessingDto, EqPointDto, ProcessingSetDto, StripPresetDto, StripPresetsSetDto,
    };
    pub use bandear_desktop_core::dto;
    pub use bandear_desktop_core::logfile::LogLocation;
    pub use bandear_desktop_core::session::{
        SessionAudio, SessionChannel, SessionPreset, SessionPresetChannel, SessionPresetMix,
    };
    pub use bandear_desktop_core::sessions::SnapshotSummary;
    pub use personal_monitoring::{
        CompressorSettings, DelayNote, DelaySettings, Effect, EffectLevels, EffectSettings,
        EqBandSettings, EqSettings, GateSettings, HighPassSettings, LimiterSettings,
        ReverbSettings, StripSettings,
    };
}
