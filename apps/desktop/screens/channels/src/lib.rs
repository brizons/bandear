//! The BandEar desktop's Channels and groups screen (parity inventory area D §1, task T6): the channel details (names, kinds, pairs, hidden inputs) and the groups.
//!
//! It depends on the console (state, commands, lanes) and the kit (controls, theme) only. The
//! [model] is pure and tested without a window; the [view] draws it.

pub mod model;
pub mod view;

pub use view::ChannelsScreen;
