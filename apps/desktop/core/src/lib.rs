//! The BandEar desktop's backend, whatever draws its window.
//!
//! Every engine call the window makes is in [commands], answered with the JSON-ready shapes of
//! [dto]; [events] streams the running engine's meters, status and mix changes; [session],
//! [sessions], [backups] and [migration] keep the desk's state on disk, in the [folders] the app
//! has; [logfile] keeps the log. [lifecycle] is what a shell does as the app starts and quits. The
//! engine lives in `personal-monitoring` and knows nothing of any window, and neither does this
//! crate: a shell (the Tauri app, the GPUI app) owns the window and calls in.

pub mod backups;
pub mod commands;
pub mod dto;
pub mod events;
pub mod folders;
pub mod lifecycle;
pub mod logfile;
pub mod migration;
pub mod session;
pub mod sessions;
