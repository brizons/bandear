//! The engineer's channel details and the rules the engine keeps for them (`channels.js`). Pure
//! functions.
//!
//! A status channel ([Channel]) carries its number, name, kind, hidden flag and link; the details
//! the desk sends (`set_channels`) and keeps (`channel_table`) are [SessionChannel]s in channel
//! order, the first entry CH 1. `link`, on the first input of a pair only, makes it and the next
//! input one stereo strip; a hidden input is heard in no mix.

use bandear_desktop_console::mix::{MAX_CHANNELS, Mix};
use bandear_desktop_console::state::PairInput;
use bandear_desktop_console::types::SessionChannel;
use bandear_desktop_console::values::clamp_level;
use bandear_desktop_console::wire::Channel;

/// The engine's instrument kinds, in the order a picker offers them; `other` is the default.
pub const CHANNEL_KINDS: [&str; 11] = [
    "voice",
    "guitar",
    "bass",
    "keys",
    "drums",
    "percussion",
    "horns",
    "strings",
    "playback",
    "click",
    "other",
];

/// The kind an input has when nothing else is said.
pub const OTHER_KIND: &str = "other";

/// The longest channel name the engine takes, in characters.
pub const MAX_CHANNEL_NAME_LENGTH: usize = 32;

/// What the console calls instrument `kind`; `None` for a kind this build does not know.
pub fn kind_label(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "voice" => "Voice",
        "guitar" => "Guitar",
        "bass" => "Bass",
        "keys" => "Keys",
        "drums" => "Drums",
        "percussion" => "Percussion",
        "horns" => "Horns",
        "strings" => "Strings",
        "playback" => "Playback",
        "click" => "Click",
        "other" => "Other",
        _ => return None,
    })
}

/// The details of an input the engineer said nothing about.
pub fn default_channel() -> SessionChannel {
    SessionChannel {
        name: String::new(),
        kind: OTHER_KIND.to_owned(),
        hidden: false,
        link: false,
    }
}

/// What the console calls an input numbered `number` (1-based) and named `name`: the name trimmed,
/// or "CH 3" when it has none.
pub fn label_of(name: &str, number: usize) -> String {
    let name = name.trim();
    if name.is_empty() {
        format!("CH {number}")
    } else {
        name.to_owned()
    }
}

/// What the console calls a status channel: its name, or "CH 3" when it has none.
pub fn channel_label(channel: &Channel) -> String {
    label_of(&channel.name, usize::from(channel.number))
}

/// Refuses the details the engine refuses (its `check_channels`), saying which input: more than 32
/// entries, a kind it does not know, a name longer than 32 characters, a pair whose second side is
/// not listed, an input in two pairs, a pair with one side hidden. A kind this build does not know
/// passes where `kept` (the table the desk keeps, as it was read) has exactly that kind for that
/// input: a newer build wrote it, and the desk keeps it while no one changes it.
pub fn check_channels(channels: &[SessionChannel], kept: &[SessionChannel]) -> Result<(), String> {
    if channels.len() > MAX_CHANNELS {
        return Err(format!(
            "channel details are supported for at most {MAX_CHANNELS} channels, received {}",
            channels.len()
        ));
    }
    for (index, channel) in channels.iter().enumerate() {
        let number = index + 1;
        let known = CHANNEL_KINDS.contains(&channel.kind.as_str());
        let kept_kind = kept
            .get(index)
            .is_some_and(|entry| entry.kind == channel.kind);
        if !known && !kept_kind {
            return Err(format!(
                "CH {number} has the instrument kind `{}`, which is none of {}",
                channel.kind,
                CHANNEL_KINDS.join(", ")
            ));
        }
        if channel.name.chars().count() > MAX_CHANNEL_NAME_LENGTH {
            return Err(format!(
                "the name of CH {number} is longer than {MAX_CHANNEL_NAME_LENGTH} characters"
            ));
        }
        if !channel.link {
            continue;
        }
        let Some(next) = channels.get(index + 1) else {
            return Err(format!(
                "CH {number} is linked to the next channel, but it is the last one"
            ));
        };
        if next.link {
            return Err(format!(
                "CH {} cannot be in two stereo pairs (CH {number}-{} and CH {}-{})",
                number + 1,
                number + 1,
                number + 1,
                number + 2
            ));
        }
        if next.hidden != channel.hidden {
            return Err(format!(
                "CH {number} and CH {} are one stereo pair: hide or show both",
                number + 1
            ));
        }
    }
    Ok(())
}

/// The details for a device with `count` inputs, as the engine fits them: kept for the inputs the
/// device has, the defaults for inputs with none, dropped past the last input, and the last input
/// never linked (its pair would be gone). The details given are left as they were.
pub fn fit_channels(channels: &[SessionChannel], count: usize) -> Vec<SessionChannel> {
    let mut fitted: Vec<SessionChannel> = (0..count)
        .map(|index| channels.get(index).cloned().unwrap_or_else(default_channel))
        .collect();
    if let Some(last) = fitted.last_mut() {
        last.link = false;
    }
    fitted
}

/// The two pans a stereo pair's balance writes: balance 0 is hard left, 50 the centre (the full
/// width, pans 0 and 100) and 100 hard right; the first pan is `max(0, 2b - 100)`, the second
/// `min(100, 2b)`, the rule every client of the engine uses.
pub fn balance_to_pans(balance: f64) -> [u8; 2] {
    let level = i32::from(clamp_level(balance));
    [(2 * level - 100).max(0) as u8, (2 * level).min(100) as u8]
}

/// The balance a pair's two pans show: their mean, which reads back what [balance_to_pans] wrote.
pub fn pans_to_balance(first: f64, second: f64) -> u8 {
    clamp_level((first + second) / 2.0)
}

/// A mix table a stereo pair keeps equal on both sides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairedTable {
    ChannelGains,
    ChannelMuted,
    ChannelSolo,
    /// The engineer's locks, which a pair takes the same way.
    LockedChannels,
}

/// The tables a pair keeps equal after a phone's or the desk's change: level, mute and solo.
pub const PAIRED_TABLES: [PairedTable; 3] = [
    PairedTable::ChannelGains,
    PairedTable::ChannelMuted,
    PairedTable::ChannelSolo,
];

/// Makes the two sides of the pair at `first` equal in `table`, as the engine does: a side that
/// moved from `before` carries the other with it, and when both moved the first side wins.
fn keep_side<T: Copy + PartialEq>(before: &[T], table: &mut [T], first: usize) {
    let second = first + 1;
    let (Some(&was_first), Some(&was_second)) = (before.get(first), before.get(second)) else {
        return;
    };
    if second >= table.len() {
        return;
    }
    if table[second] != was_second && table[first] == was_first {
        table[first] = table[second];
    } else {
        table[second] = table[first];
    }
}

/// `merged` with both sides of every linked pair in `channels` at one level, mute and solo (the
/// `fields`), as the engine keeps them after any change from `before`. Pans are left as they are:
/// a pair's balance writes both (see [balance_to_pans]).
pub fn keep_pairs_equal<C: PairInput>(
    before: &Mix,
    merged: &Mix,
    channels: &[C],
    fields: &[PairedTable],
) -> Mix {
    let mut kept = merged.clone();
    for (first, channel) in channels.iter().enumerate() {
        if !channel.link() || first + 1 >= MAX_CHANNELS {
            continue;
        }
        for field in fields {
            match field {
                PairedTable::ChannelGains => {
                    keep_side(&before.channel_gains, &mut kept.channel_gains, first)
                }
                PairedTable::ChannelMuted => {
                    keep_side(&before.channel_muted, &mut kept.channel_muted, first)
                }
                PairedTable::ChannelSolo => {
                    keep_side(&before.channel_solo, &mut kept.channel_solo, first)
                }
                PairedTable::LockedChannels => {
                    keep_side(&before.locked_channels, &mut kept.locked_channels, first)
                }
            }
        }
    }
    kept
}

/// `merged`, a phone's change merged into `before`, with the level, pan, mute and solo of every
/// channel `before` locks put back, as the engine keeps them: the phone cannot change a locked
/// channel, and everything else it sent stands.
pub fn keep_locked(before: &Mix, merged: &Mix) -> Mix {
    fn put_back<T: Copy>(before: &[T], table: &mut [T], locked: &[bool]) {
        for (channel, value) in table.iter_mut().enumerate() {
            if locked.get(channel).copied().unwrap_or(false)
                && let Some(kept) = before.get(channel)
            {
                *value = *kept;
            }
        }
    }
    let mut kept = merged.clone();
    let locked = &before.locked_channels;
    put_back(&before.channel_gains, &mut kept.channel_gains, locked);
    put_back(&before.pans, &mut kept.pans, locked);
    put_back(&before.channel_muted, &mut kept.channel_muted, locked);
    put_back(&before.channel_solo, &mut kept.channel_solo, locked);
    kept
}

/// `locked` with both sides of every linked pair in `channels` locked when either side is, as the
/// engine locks a pair linked after one of its sides was locked: a pair is one strip.
pub fn lock_pairs<C: PairInput>(locked: &[bool], channels: &[C]) -> Vec<bool> {
    let mut both = locked.to_vec();
    for (first, channel) in channels.iter().enumerate() {
        let second = first + 1;
        if channel.link()
            && second < MAX_CHANNELS
            && second < both.len()
            && (both[first] || both[second])
        {
            both[first] = true;
            both[second] = true;
        }
    }
    both
}

#[cfg(test)]
pub(crate) fn detail(name: &str, kind: &str, hidden: bool, link: bool) -> SessionChannel {
    SessionChannel {
        name: name.to_owned(),
        kind: kind.to_owned(),
        hidden,
        link,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(name: &str, kind: &str) -> SessionChannel {
        detail(name, kind, false, false)
    }

    #[test]
    fn the_instrument_kinds_are_the_engines_in_the_order_a_picker_offers_them() {
        assert_eq!(
            CHANNEL_KINDS,
            [
                "voice",
                "guitar",
                "bass",
                "keys",
                "drums",
                "percussion",
                "horns",
                "strings",
                "playback",
                "click",
                "other"
            ]
        );
        assert_eq!(kind_label("percussion"), Some("Percussion"));
        assert_eq!(kind_label("theremin"), None);
    }

    #[test]
    fn a_channel_is_called_by_its_name_or_by_its_number_when_it_has_none() {
        let channel = |number: u8, name: &str| Channel {
            number,
            name: name.to_owned(),
            ..Channel::default()
        };
        assert_eq!(channel_label(&channel(1, "Kick")), "Kick");
        assert_eq!(channel_label(&channel(3, "")), "CH 3");
        assert_eq!(channel_label(&channel(4, "   ")), "CH 4");
    }

    #[test]
    fn a_pairs_balance_writes_both_pans_and_the_two_pans_read_back_as_the_balance() {
        assert_eq!(balance_to_pans(50.0), [0, 100], "centre is the full width");
        assert_eq!(balance_to_pans(0.0), [0, 0]);
        assert_eq!(balance_to_pans(100.0), [100, 100]);
        assert_eq!(balance_to_pans(70.0), [40, 100]);
        assert_eq!(balance_to_pans(30.0), [0, 60]);
        assert_eq!(balance_to_pans(130.0), [100, 100], "clamped like any level");
        for balance in 0..=100u8 {
            let [first, second] = balance_to_pans(f64::from(balance));
            assert_eq!(
                pans_to_balance(f64::from(first), f64::from(second)),
                balance,
                "balance {balance}"
            );
        }
        assert_eq!(pans_to_balance(0.0, 100.0), 50);
    }

    #[test]
    fn channel_details_are_checked_as_the_engine_checks_them() {
        assert_eq!(
            check_channels(
                &[
                    plain("Kick", "drums"),
                    detail("Keys L", "keys", false, true),
                    plain("Keys R", "keys"),
                    detail("Spare L", "other", true, true),
                    detail("Spare R", "other", true, false),
                ],
                &[],
            ),
            Ok(())
        );
        let refused = [
            (vec![plain("Kazoo", "kazoo")], "kazoo"),
            (vec![plain("", "other"); 33], "33"),
            (vec![plain(&"x".repeat(33), "other")], "CH 1"),
            (
                vec![plain("Kick", "drums"), detail("Pad", "keys", false, true)],
                "CH 2",
            ),
            (
                vec![
                    detail("A", "keys", false, true),
                    detail("B", "keys", false, true),
                    plain("C", "keys"),
                ],
                "CH 2",
            ),
            (
                vec![
                    detail("L", "keys", false, true),
                    detail("R", "keys", true, false),
                ],
                "CH 1",
            ),
        ];
        for (channels, named) in refused {
            let error = check_channels(&channels, &[]).expect_err(named);
            assert!(error.contains(named), "{error}");
        }
        assert_eq!(
            check_channels(&[plain("x", "other")], &[]),
            Ok(()),
            "a name of one character"
        );
        assert_eq!(
            check_channels(&[plain(&"é".repeat(32), "other")], &[]),
            Ok(()),
            "names count characters, not bytes"
        );
        assert_eq!(
            check_channels(
                &[plain("Kick", "drums"), detail("A", "keys", false, true)],
                &[]
            )
            .unwrap_err(),
            "CH 2 is linked to the next channel, but it is the last one"
        );
        assert_eq!(
            check_channels(
                &[
                    detail("A", "keys", false, true),
                    detail("B", "keys", false, true),
                    plain("C", "keys"),
                ],
                &[]
            )
            .unwrap_err(),
            "CH 2 cannot be in two stereo pairs (CH 1-2 and CH 2-3)"
        );
        assert_eq!(
            check_channels(&[plain("Kazoo", "kazoo")], &[]).unwrap_err(),
            "CH 1 has the instrument kind `kazoo`, which is none of voice, guitar, bass, keys, \
             drums, percussion, horns, strings, playback, click, other"
        );
        assert_eq!(
            check_channels(&[plain("Theremin", "theremin")], &[plain("", "theremin")]),
            Ok(()),
            "a kind the desk keeps for that input passes"
        );
    }

    #[test]
    fn channel_details_fit_a_device_kept_where_it_has_the_input_defaults_past_them_no_half_pair() {
        let details = vec![
            plain("Kick", "drums"),
            detail("Spare", "other", true, false),
            detail("Keys L", "keys", false, true),
            plain("Keys R", "keys"),
        ];
        assert_eq!(fit_channels(&details, 4), details);
        assert_eq!(
            fit_channels(&details, 3),
            vec![
                plain("Kick", "drums"),
                detail("Spare", "other", true, false),
                plain("Keys L", "keys"),
            ]
        );
        assert_eq!(
            fit_channels(&details[..1], 2),
            vec![plain("Kick", "drums"), plain("", "other")]
        );
        assert!(details[2].link, "the details given are left as they were");
    }

    #[test]
    fn both_sides_of_a_pair_keep_one_level_mute_and_solo_as_the_engine_keeps_them() {
        let channels = [
            detail("Keys L", "keys", false, true),
            plain("Keys R", "keys"),
            plain("", "other"),
        ];
        let before = Mix::engine_default();
        let keep = |change: &dyn Fn(&mut Mix)| {
            let mut merged = before.clone();
            change(&mut merged);
            keep_pairs_equal(&before, &merged, &channels, &PAIRED_TABLES)
        };
        assert_eq!(
            keep(&|mix| mix.channel_gains[0] = 80).channel_gains[..3],
            [80, 80, 100],
            "the first side moves the second"
        );
        assert_eq!(
            keep(&|mix| mix.channel_gains[1] = 40).channel_gains[..2],
            [40, 40],
            "the second side moves the first"
        );
        assert_eq!(
            keep(&|mix| {
                mix.channel_gains[0] = 30;
                mix.channel_gains[1] = 60;
            })
            .channel_gains[..2],
            [30, 30],
            "both changed: the first channel wins"
        );
        assert_eq!(
            keep(&|mix| mix.channel_muted[1] = true).channel_muted[..2],
            [true, true]
        );
        assert_eq!(
            keep(&|mix| mix.channel_solo[0] = true).channel_solo[..2],
            [true, true]
        );
        let panned = keep(&|mix| {
            mix.pans[0] = 20;
            mix.pans[1] = 70;
        });
        assert_eq!(panned.pans[..2], [20, 70], "a balance writes both pans");
        assert_eq!(
            keep(&|mix| mix.channel_gains[2] = 50).channel_gains[2..4],
            [50, 100],
            "CH 3 is no pair"
        );
    }

    #[test]
    fn a_phone_cannot_change_a_locked_channel_and_a_pair_locks_as_one() {
        let mut before = Mix::engine_default();
        before.locked_channels[1] = true;
        let mut merged = before.clone();
        merged.channel_gains[0] = 10;
        merged.channel_gains[1] = 20;
        merged.pans[1] = 50;
        merged.channel_muted[1] = true;
        let kept = keep_locked(&before, &merged);
        assert_eq!(kept.channel_gains[..2], [10, 100], "CH 2 is locked");
        assert_eq!(kept.pans[1], before.pans[1]);
        assert!(!kept.channel_muted[1]);

        let channels = [detail("L", "keys", false, true), plain("R", "keys")];
        let locked = lock_pairs(&before.locked_channels, &channels);
        assert_eq!(locked[..3], [true, true, false]);
        assert_eq!(
            keep_pairs_equal(
                &Mix::engine_default(),
                &before,
                &channels,
                &[PairedTable::LockedChannels]
            )
            .locked_channels[..2],
            [true, true],
            "a lock on one side carries the other"
        );
    }
}
