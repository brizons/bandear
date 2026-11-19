//! The BandEar desktop's Channels and groups screen (parity inventory area D §1, task T6): the
//! channel details (names, kinds, pairs, hidden inputs) and the groups.
//!
//! It depends on the console (state, commands, lanes) and the kit (controls, theme) only. The
//! rules ([channels]), the table ([table]) and the groups ([groups]) are pure; the [model] runs the
//! screen's edits over the console and is tested without a window; the [view] draws it.

pub mod channels;
pub mod groups;
pub mod model;
pub mod table;
pub mod view;

#[cfg(test)]
mod testdesk;

pub use view::ChannelsScreen;
