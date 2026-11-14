//! The BandEar desktop's Record screen (parity inventory area C, task T5): recording takes and the virtual soundcheck, and the REC and SOUNDCHECK chips the shell shows above every screen.
//!
//! It depends on the console (state, commands, lanes) and the kit (controls, theme) only. The
//! [model] is pure and tested without a window; the [view] draws it.

pub mod model;
pub mod view;

pub use view::RecordScreen;
