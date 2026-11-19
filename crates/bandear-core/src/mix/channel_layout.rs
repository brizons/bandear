//! How the desk's channels become strips, and how a stereo pair's balance becomes two pans.
//!
//! Ported from the iOS client's former `ChannelLayout`, itself ported from the Android client's
//! former `ChannelLayout`. How a strip with no name is labelled ("CH 1") is the screen's words and
//! stays with each platform: [`name`] gives the engineer's name or none.

use crate::control::client_message::trimmed_name;
use crate::control::server_message::SourceChannel;

/// The source channels one strip controls: `first` alone, or `first` and `second` as a stereo
/// pair. A strip is known by its first channel everywhere: the edits it reports, the sheet it
/// opens, the musician's channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StripChannels {
    pub first: i64,
    pub second: Option<i64>,
}

impl StripChannels {
    /// The strip of `first` alone, or of the pair `first` and `second`.
    pub fn new(first: i64, second: Option<i64>) -> StripChannels {
        StripChannels { first, second }
    }

    /// Every channel the strip writes, the first one first.
    pub fn sides(&self) -> Vec<i64> {
        std::iter::once(self.first).chain(self.second).collect()
    }
}

/// The two pans a stereo pair writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StereoPans {
    pub first: i64,
    pub second: i64,
}

/// The strips for `channel_count` source channels with the desk's `details`, left to right.
/// `None` details (an older desk announced none) show every channel as a strip of its own.
///
/// A hidden channel has no strip. A channel that links to the next makes one strip of the two
/// when both are shown; a link that cannot make a pair (on the last channel, or into a hidden
/// one, which the desk never sends) leaves each channel to its own flags. A channel past the
/// details is one the engineer never touched.
pub fn strips(channel_count: i64, details: Option<&[SourceChannel]>) -> Vec<StripChannels> {
    let detail = |channel: i64| -> Option<&SourceChannel> {
        details.and_then(|details| element(details, channel))
    };
    let mut strips = Vec::new();
    let mut channel = 0;
    while channel < channel_count {
        let shown = detail(channel).map(|d| d.is_hidden) != Some(true);
        let pairs = detail(channel).map(|d| d.is_linked_to_next) == Some(true)
            && channel + 1 < channel_count
            && shown
            && detail(channel + 1).map(|d| d.is_hidden) != Some(true);
        if pairs {
            strips.push(StripChannels::new(channel, Some(channel + 1)));
        } else if shown {
            strips.push(StripChannels::new(channel, None));
        }
        channel += if pairs { 2 } else { 1 };
    }
    strips
}

/// The two pans a pair's `balance` writes (0 left, 50 centre, 100 right): the first pan is
/// `max(0, 2b - 100)` and the second `min(100, 2b)`, the rule every client of the desk uses
/// (`apps/server/src/protocol.rs:427-432`). The centre is the pair at full width, 0 and 100. A
/// balance out of range reads as the nearest end.
pub fn pans(balance: i64) -> StereoPans {
    let level = balance.clamp(0, 100);
    StereoPans {
        first: (2 * level - 100).max(0),
        second: (2 * level).min(100),
    }
}

/// The balance a pair's two pans show: their mean, halves rounded up, which reads back [`pans`].
///
/// Kotlin's `roundToInt` and Swift's `rounded()` (which `f64::round` matches) disagree on
/// negative halves (-0.5 becomes 0 and -1). Both are then held to 0-100, which makes them agree
/// everywhere.
pub fn balance(pans: StereoPans) -> i64 {
    let mean = (pans.first as f64 + pans.second as f64) / 2.0;
    (mean.round() as i64).clamp(0, 100)
}

/// The engineer's name for a channel with `detail`, without the whitespace around it, or `None`
/// when it has none: the screen then names the strip by its number ("CH n"), in its own words.
pub fn name(detail: Option<&SourceChannel>) -> Option<String> {
    detail
        .map(|detail| trimmed_name(&detail.name))
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
}

/// Where a channel is panned until someone moves it: even channels left, odd ones right, so a
/// desk's adjacent inputs open as stereo (Android's former `defaultPan`).
pub fn default_pan(channel: i64) -> i64 {
    if channel % 2 == 0 {
        0
    } else {
        100
    }
}

/// The element of `list` at `index`, or `None` when the list has none there (a negative index
/// included).
pub(crate) fn element<T>(list: &[T], index: i64) -> Option<&T> {
    usize::try_from(index)
        .ok()
        .and_then(|index| list.get(index))
}
