//! The BandEar desktop's Effects screen (parity inventory area D §3, task T5): the shared reverb
//! and delay every input can send to (from the Processing editor) and every mix can return (from
//! its master strip).
//!
//! It depends on the console (state, commands, lanes) and the kit (controls, theme) only. What the
//! screen shows is worked out by [effects]; its effect lane and its outcome are the [model]'s; the
//! [view] draws them. All but the view are tested without a window.

pub mod effects;
pub mod model;
pub mod view;

pub use view::EffectsScreen;
