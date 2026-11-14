//! The BandEar desktop's Mix screen (parity inventory area B, task T4): the musicians' mixes: the tabs, the channel and group strips, the master strip with its limiter and returns, the Mix tools, and the top bar (the kit draws it).
//!
//! It depends on the console (state, commands, lanes) and the kit (controls, theme) only. The
//! [model] is pure and tested without a window; the [view] draws it. Its heading is visually hidden (the tabs say where you are) but still read and still focused.

pub mod model;
pub mod view;

pub use view::MixScreen;
