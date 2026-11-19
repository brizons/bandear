//! The BandEar desktop's Processing screen (parity inventory area D §2, task T6): each input's
//! channel strip (high-pass, equaliser, gate, compressor), its sends to the shared effects and the
//! strip presets.
//!
//! It depends on the console (state, commands, lanes) and the kit (controls, theme) only. The
//! strip's rules ([strip]), words ([words]), graph ([graph]), sliders ([fields]) and choices
//! ([choices]) are pure; [desk] sends the screen's commands on their lanes and [model] is the
//! editor, both tested without a window; the [view] draws it.

pub mod choices;
pub mod desk;
pub mod fields;
pub mod graph;
pub mod model;
pub mod strip;
pub mod view;
pub mod words;

#[cfg(test)]
mod testdesk;

pub use view::ProcessingScreen;
