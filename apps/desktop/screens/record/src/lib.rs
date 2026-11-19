//! The BandEar desktop's Record screen (parity inventory area C, task T5): recording takes and the
//! virtual soundcheck, and the REC and SOUNDCHECK chips the shell shows above every screen.
//!
//! It depends on the console (state, commands, lanes) and the kit (controls, theme) only. What the
//! screen shows is worked out by pure functions ([recording], [soundcheck], [record_view],
//! [soundcheck_view]); its flows and its own state are the [model]'s; the [view] draws them. All
//! but the view are tested without a window.

pub mod model;
pub mod peaks;
pub mod record_view;
pub mod recording;
pub mod soundcheck;
pub mod soundcheck_view;
pub mod view;

#[cfg(test)]
mod fixtures;

pub use view::RecordScreen;
