//! The BandEar desktop's Effects screen (parity inventory area D §3, task T5): the shared reverb and delay.
//!
//! It depends on the console (state, commands, lanes) and the kit (controls, theme) only. The
//! [model] is pure and tested without a window; the [view] draws it.

pub mod model;
pub mod view;

pub use view::EffectsScreen;
