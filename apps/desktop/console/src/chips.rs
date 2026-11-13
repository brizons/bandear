//! The chips above every screen (`rec-chip.js`): the demo source, a take recording, and a
//! soundcheck playing in place of the inputs. Each is one link whose accessible name and tooltip are
//! its `label`; the word, time and detail are only seen.
//!
//! The demo chip is the console's ([crate::demo_source::demo_chip]); the recording and soundcheck
//! chips are the Record screen's, which builds them from the status.

use crate::router::Section;

/// Which chip, and so how it looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChipKind {
    /// The demo source plays: blue border, blue square dot.
    Demo,
    /// A take records: danger, red dot pulsing.
    Recording,
    /// The take's files are being finished: warn, accent dot.
    Stopping,
    /// The recording stopped on a failure: danger, square dot.
    Failed,
    /// A soundcheck plays: accent, warn border, accent triangle pulsing.
    Soundcheck,
    /// A soundcheck is paused: as [ChipKind::Soundcheck], no pulse.
    SoundcheckPaused,
    /// A soundcheck stopped on a failure: danger, red triangle.
    SoundcheckFailed,
}

/// A chip's content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chip {
    pub kind: ChipKind,
    /// The bold word (`REC`, `SOUNDCHECK`, `Demo source`).
    pub word: String,
    /// The time shown in mono, or empty.
    pub time: String,
    /// The detail after it, or empty.
    pub detail: String,
    /// The chip's accessible name and tooltip.
    pub label: String,
    /// The section the chip opens.
    pub target: Section,
}
