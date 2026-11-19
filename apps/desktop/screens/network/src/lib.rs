//! The BandEar desktop's Network and system screen (parity inventory area E, task T7): the musicians table and link health, adding and removing phones, named sessions and mix backups, and the audio setup.
//!
//! It depends on the console (state, commands, lanes) and the kit (controls, theme) only. The
//! [model] is pure and tested without a window; the [view] draws it.

pub mod model;
pub mod view;

pub use view::NetworkScreen;
