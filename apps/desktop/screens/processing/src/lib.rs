//! The BandEar desktop's Processing screen (parity inventory area D §2, task T6): each input's channel strip (equaliser, gate, compressor, sends) and the strip presets.
//!
//! It depends on the console (state, commands, lanes) and the kit (controls, theme) only. The
//! [model] is pure and tested without a window; the [view] draws it.

pub mod model;
pub mod view;

pub use view::ProcessingScreen;
