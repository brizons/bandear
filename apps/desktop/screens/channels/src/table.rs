//! The channel table of the Channels and groups screen (`channels-view.js`): one row per input of
//! the device, the stereo cell's choice and reason, and the channel edits. Pure functions.
//!
//! Every channel edit is a function of the whole table the desk keeps (`channel_table`: the details
//! in channel order, entries past the device included). An edit returns a new table and leaves the
//! one given as it was; a table the desk would refuse is an `Err` that says why, so nothing is sent.

use bandear_desktop_console::router::{Route, Section};
use bandear_desktop_console::state::{ConsoleState, pair_units};
use bandear_desktop_console::types::SessionChannel;
use bandear_desktop_console::values::{GroupTone, group_tone};
use bandear_desktop_console::wire::{Channel, Status};

use crate::channels::{
    CHANNEL_KINDS, OTHER_KIND, channel_label, check_channels, default_channel, kind_label,
};

/// One choice of the instrument picker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KindOption {
    pub value: &'static str,
    pub label: &'static str,
}

/// The instrument picker's options, in the engine's order.
pub fn kind_options() -> Vec<KindOption> {
    CHANNEL_KINDS
        .iter()
        .map(|value| KindOption {
            value,
            label: kind_label(value).unwrap_or("Other"),
        })
        .collect()
}

/// "CH 3" for input `index` (0-based).
fn input(index: usize) -> String {
    format!("CH {}", index + 1)
}

/// A status's inputs: as many as the device has.
pub fn device_channels(status: Option<&Status>) -> &[Channel] {
    match status {
        Some(status) => &status.channels[..status.channels.len().min(status.source_channels)],
        None => &[],
    }
}

/// The stereo cell of an input: whether it is linked, the pair's first input (where the link is
/// kept) and the other side, its words and its control's name, and whether it can pair now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkChoice {
    pub linked: bool,
    /// The pair's first input (0-based), or the input itself when it is on its own.
    pub first: usize,
    /// The other side (0-based); for an input on its own, the next input, if any.
    pub partner: Option<usize>,
    /// "Linked with CH 2" or "Mono".
    pub text: String,
    /// "Stereo pair CH 1 and CH 2" or "Stereo pair CH 6 and the next input".
    pub label: String,
    pub available: bool,
    /// Why an input on its own cannot pair with the next one.
    pub reason: Option<String>,
}

/// The stereo cell of input `index` (0-based) among a status's `channels`. A linked input names
/// the other side and the pair's first input; either side can undo it. An input on its own can
/// pair with the next one only when that one exists, is in no other pair and is shown or hidden
/// alike; otherwise it is not available and the reason says why.
pub fn link_choice(channels: &[Channel], index: usize) -> LinkChoice {
    let units = pair_units(channels);
    if let Some(unit) = units
        .iter()
        .find(|unit| unit.contains(&index) && unit.len() == 2)
    {
        let (first, second) = (unit[0], unit[1]);
        let partner = if index == first { second } else { first };
        return LinkChoice {
            linked: true,
            first,
            partner: Some(partner),
            text: format!("Linked with {}", input(partner)),
            label: format!("Stereo pair {} and {}", input(first), input(second)),
            available: true,
            reason: None,
        };
    }
    let next = channels.get(index + 1);
    let mut choice = LinkChoice {
        linked: false,
        first: index,
        partner: next.map(|_| index + 1),
        text: "Mono".to_owned(),
        label: format!(
            "Stereo pair {} and {}",
            input(index),
            if next.is_some() {
                input(index + 1)
            } else {
                "the next input".to_owned()
            }
        ),
        available: false,
        reason: None,
    };
    match next {
        None => {
            choice.reason = Some(format!(
                "{} is the last input, so there is no next input to pair it with.",
                input(index)
            ));
        }
        Some(_)
            if units
                .iter()
                .any(|unit| unit[0] == index + 1 && unit.len() == 2) =>
        {
            choice.reason = Some(format!(
                "{} is already paired with {}.",
                input(index + 1),
                input(index + 2)
            ));
        }
        Some(next) if next.hidden != channels[index].hidden => {
            let (hidden, shown) = if next.hidden {
                (index + 1, index)
            } else {
                (index, index + 1)
            };
            choice.reason = Some(format!(
                "{} is hidden and {} is shown: show or hide both before pairing them.",
                input(hidden),
                input(shown)
            ));
        }
        Some(_) => choice.available = true,
    }
    choice
}

/// The group a row's input is in, as its chip shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowGroup {
    pub id: String,
    pub name: String,
    pub tone: GroupTone,
    pub invalid: bool,
}

/// One row of the channel table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelRow {
    /// 0-based.
    pub index: usize,
    /// 1-based.
    pub number: usize,
    /// "CH 1".
    pub input: String,
    /// The engineer's name, empty when none.
    pub name: String,
    /// What every screen calls the input.
    pub label: String,
    /// A kind this build does not know shows as `other`.
    pub kind: String,
    pub kind_label: &'static str,
    pub hidden: bool,
    pub stereo: LinkChoice,
    pub group: Option<RowGroup>,
}

/// One row per input of the device, each with its group found by id.
pub fn channel_rows(state: &ConsoleState) -> Vec<ChannelRow> {
    let status = state.engine.status.as_ref();
    let channels = device_channels(status);
    let groups = status.map(|status| status.groups.as_slice()).unwrap_or(&[]);
    channels
        .iter()
        .enumerate()
        .map(|(index, channel)| {
            let group = channel.group_id.as_ref().and_then(|id| {
                groups
                    .iter()
                    .position(|group| &group.id == id)
                    .map(|position| {
                        let group = &groups[position];
                        RowGroup {
                            id: group.id.clone(),
                            name: group.name.clone(),
                            tone: group_tone(Some(position as i64)),
                            invalid: group.invalid,
                        }
                    })
            });
            let kind = if kind_label(&channel.kind).is_some() {
                channel.kind.clone()
            } else {
                OTHER_KIND.to_owned()
            };
            ChannelRow {
                index,
                number: usize::from(channel.number),
                input: format!("CH {}", channel.number),
                name: channel.name.clone(),
                label: channel_label(channel),
                kind_label: kind_label(&kind).unwrap_or("Other"),
                kind,
                hidden: channel.hidden,
                stereo: link_choice(channels, index),
                group,
            }
        })
        .collect()
}

/// A copy of `table` with at least `count` entries and room for input `index`.
fn editable(table: &[SessionChannel], count: usize, index: usize) -> Vec<SessionChannel> {
    let length = table.len().max(count).max(index + 1);
    (0..length)
        .map(|entry| table.get(entry).cloned().unwrap_or_else(default_channel))
        .collect()
}

/// `next` when the desk would take it; a kind this build does not know passes where `table` (the
/// table as read) has it for that input.
fn checked(
    next: Vec<SessionChannel>,
    table: &[SessionChannel],
) -> Result<Vec<SessionChannel>, String> {
    check_channels(&next, table)?;
    Ok(next)
}

fn unit_of(table: &[SessionChannel], index: usize) -> Vec<usize> {
    pair_units(table)
        .into_iter()
        .find(|unit| unit.contains(&index))
        .unwrap_or_else(|| vec![index])
}

/// `table` with input `index` named `name` (trimmed; empty for none), for a device of `count`.
pub fn rename_channel(
    table: &[SessionChannel],
    index: usize,
    name: &str,
    count: usize,
) -> Result<Vec<SessionChannel>, String> {
    let mut next = editable(table, count, index);
    next[index].name = name.trim().to_owned();
    checked(next, table)
}

/// `table` with input `index` of instrument `kind`.
pub fn set_channel_kind(
    table: &[SessionChannel],
    index: usize,
    kind: &str,
    count: usize,
) -> Result<Vec<SessionChannel>, String> {
    let mut next = editable(table, count, index);
    next[index].kind = kind.to_owned();
    checked(next, table)
}

/// `table` with input `index` hidden or shown: both sides of a pair go together.
pub fn set_channel_hidden(
    table: &[SessionChannel],
    index: usize,
    hidden: bool,
    count: usize,
) -> Result<Vec<SessionChannel>, String> {
    let mut next = editable(table, count, index);
    for side in unit_of(&next, index) {
        next[side].hidden = hidden;
    }
    checked(next, table)
}

/// `table` with input `index` paired with the next input (`linked`), or with the pair it is in,
/// on either side, undone.
pub fn set_channel_link(
    table: &[SessionChannel],
    index: usize,
    linked: bool,
    count: usize,
) -> Result<Vec<SessionChannel>, String> {
    let mut next = editable(table, count, index);
    let unit = unit_of(&next, index);
    if !linked {
        next[unit[0]].link = false;
    } else if unit.len() == 1 {
        next[index].link = true;
    }
    checked(next, table)
}

/// Whether an input's processing does something, as its link says it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessingState {
    On,
    Off,
    /// The strips were never read.
    Unknown,
}

/// The link to an input's processing editor that the Channels table shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessingLink {
    /// Where it goes: the Processing screen on the strip's first side.
    pub route: Route,
    pub state: ProcessingState,
    /// The table's word: "On", "Off", or nothing while unknown.
    pub word: &'static str,
    /// The strip's text: "Processing on" or "Processing".
    pub text: &'static str,
    /// "Processing of Kick, on" / ", off" / ", not read yet".
    pub aria_label: String,
}

/// The processing link of an input whose strip has `sides` (0-based, both sides of a pair), called
/// `label`, with `active` as the console's `processing_active` answers it (`None`: never read).
/// A pair opens on its first side.
pub fn processing_link(sides: &[usize], label: &str, active: Option<bool>) -> ProcessingLink {
    let first = sides.iter().copied().min().unwrap_or(0) + 1;
    let state = match active {
        None => ProcessingState::Unknown,
        Some(true) => ProcessingState::On,
        Some(false) => ProcessingState::Off,
    };
    let (said, word) = match state {
        ProcessingState::On => ("on", "On"),
        ProcessingState::Off => ("off", "Off"),
        ProcessingState::Unknown => ("not read yet", ""),
    };
    ProcessingLink {
        route: Route {
            section: Section::Processing,
            channel: u8::try_from(first).ok(),
        },
        state,
        word,
        text: if state == ProcessingState::On {
            "Processing on"
        } else {
            "Processing"
        },
        aria_label: format!("Processing of {label}, {said}"),
    }
}

/// The processing link of a table row: its strip's sides, its label and whether the strip of its
/// first side does anything.
pub fn row_processing_link(state: &ConsoleState, row: &ChannelRow) -> ProcessingLink {
    let sides = match (row.stereo.linked, row.stereo.partner) {
        (true, Some(partner)) => vec![row.stereo.first, partner],
        _ => vec![row.index],
    };
    processing_link(
        &sides,
        &row.label,
        state.processing_active(row.stereo.first + 1),
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use bandear_desktop_console::state::Action;
    use bandear_desktop_console::testing::sample_status;
    use bandear_desktop_console::types::ChannelProcessingDto;
    use bandear_desktop_console::types::StripSettings;
    use bandear_desktop_console::wire::{Group, StatusReport};

    use super::*;
    use crate::channels::detail;

    fn plain(name: &str, kind: &str) -> SessionChannel {
        detail(name, kind, false, false)
    }

    /// A status whose inputs carry `details` (one per input) and whose groups are `groups`.
    pub(crate) fn status_with(details: &[SessionChannel], groups: Vec<Group>) -> StatusReport {
        let mut report = sample_status();
        let status = &mut report.status;
        status.source_channels = details.len();
        status.capture_format.channels = details.len() as u16;
        status.channels = details
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let group = groups
                    .iter()
                    .find(|group| !group.invalid && group.channels.contains(&index));
                Channel {
                    number: index as u8 + 1,
                    name: entry.name.clone(),
                    kind: entry.kind.clone(),
                    hidden: entry.hidden,
                    link: entry.link,
                    group: group.map(|group| group.name.clone()),
                    group_id: group.map(|group| group.id.clone()),
                    level: 0,
                }
            })
            .collect();
        status.groups = groups;
        report
    }

    pub(crate) fn state_of(report: StatusReport) -> ConsoleState {
        let mut state = ConsoleState::new();
        state.reduce(&Action::Status(Box::new(report)));
        state
    }

    #[test]
    fn the_kind_picker_offers_every_engine_kind_by_name() {
        let options: Vec<String> = kind_options()
            .iter()
            .map(|option| format!("{}:{}", option.value, option.label))
            .collect();
        assert_eq!(
            options,
            [
                "voice:Voice",
                "guitar:Guitar",
                "bass:Bass",
                "keys:Keys",
                "drums:Drums",
                "percussion:Percussion",
                "horns:Horns",
                "strings:Strings",
                "playback:Playback",
                "click:Click",
                "other:Other"
            ]
        );
    }

    #[test]
    fn a_row_per_input_with_its_name_instrument_group_and_whether_musicians_see_it() {
        let rows = channel_rows(&state_of(sample_status()));
        assert_eq!(rows.len(), 6);
        let kick = &rows[0];
        assert_eq!(
            (
                kick.index,
                kick.number,
                kick.input.as_str(),
                kick.name.as_str(),
                kick.label.as_str()
            ),
            (0, 1, "CH 1", "Kick", "Kick")
        );
        assert_eq!((kick.kind.as_str(), kick.kind_label), ("drums", "Drums"));
        assert!(!kick.hidden);
        assert_eq!(
            kick.group,
            Some(RowGroup {
                id: "drums".to_owned(),
                name: "Drums".to_owned(),
                tone: GroupTone::Teal,
                invalid: false,
            })
        );
        assert!(!kick.stereo.linked);
        assert_eq!(rows[3].group, None, "CH 4 is in no group");
        assert_eq!(
            rows[4].group.as_ref().map(|group| group.tone),
            Some(GroupTone::Blue)
        );
        let unnamed = channel_rows(&state_of(status_with(&[plain("", "other")], Vec::new())));
        assert_eq!(unnamed[0].label, "CH 1");
        assert!(
            channel_rows(&ConsoleState::new()).is_empty(),
            "no engine, no inputs"
        );
        let unknown = channel_rows(&state_of(status_with(
            &[plain("Theremin", "theremin")],
            Vec::new(),
        )));
        assert_eq!(
            (unknown[0].kind.as_str(), unknown[0].kind_label),
            ("other", "Other"),
            "a kind this build does not know shows as other"
        );
    }

    #[test]
    fn an_input_can_pair_with_the_next_one_only_when_that_one_exists_is_free_and_is_shown_alike() {
        let report = status_with(
            &[
                detail("Keys L", "keys", false, true),
                plain("Keys R", "keys"),
                plain("Guitar", "guitar"),
                detail("OH L", "drums", false, true),
                plain("OH R", "drums"),
                detail("Spare", "other", true, false),
            ],
            Vec::new(),
        );
        let channels = &report.status.channels;
        assert_eq!(
            link_choice(channels, 0),
            LinkChoice {
                linked: true,
                first: 0,
                partner: Some(1),
                text: "Linked with CH 2".to_owned(),
                label: "Stereo pair CH 1 and CH 2".to_owned(),
                available: true,
                reason: None,
            }
        );
        let second = link_choice(channels, 1);
        assert_eq!(
            (second.first, second.partner, second.text.as_str()),
            (0, Some(0), "Linked with CH 1")
        );
        assert_eq!(second.label, "Stereo pair CH 1 and CH 2");
        assert_eq!(
            link_choice(channels, 2),
            LinkChoice {
                linked: false,
                first: 2,
                partner: Some(3),
                text: "Mono".to_owned(),
                label: "Stereo pair CH 3 and CH 4".to_owned(),
                available: false,
                reason: Some("CH 4 is already paired with CH 5.".to_owned()),
            }
        );
        assert!(link_choice(channels, 4).linked, "CH 5 is OH R");
        assert_eq!(
            link_choice(channels, 5),
            LinkChoice {
                linked: false,
                first: 5,
                partner: None,
                text: "Mono".to_owned(),
                label: "Stereo pair CH 6 and the next input".to_owned(),
                available: false,
                reason: Some(
                    "CH 6 is the last input, so there is no next input to pair it with.".to_owned()
                ),
            }
        );
        let mixed = status_with(
            &[
                plain("A", "other"),
                detail("B", "other", true, false),
                plain("C", "other"),
            ],
            Vec::new(),
        );
        assert_eq!(
            link_choice(&mixed.status.channels, 0).reason.as_deref(),
            Some("CH 2 is hidden and CH 1 is shown: show or hide both before pairing them.")
        );
        assert!(!link_choice(&mixed.status.channels, 1).available);
        let free = status_with(
            &[
                plain("A", "other"),
                plain("B", "other"),
                plain("C", "other"),
            ],
            Vec::new(),
        );
        assert!(link_choice(&free.status.channels, 1).available);
        assert_eq!(link_choice(&free.status.channels, 1).reason, None);
    }

    #[test]
    fn an_edit_of_one_input_keeps_the_whole_table_past_the_device_included_and_leaves_it_as_it_was()
    {
        let table = vec![
            plain("Kick", "drums"),
            plain("Snare", "drums"),
            plain("", "other"),
            plain("Rack tom", "drums"),
        ];
        let renamed = rename_channel(&table, 0, "  Kick in  ", 2).unwrap();
        assert_eq!(renamed[0], plain("Kick in", "drums"), "names are trimmed");
        assert_eq!(renamed[1..], table[1..], "details past the device are kept");
        assert_eq!(table[0].name, "Kick", "the table given is left as it was");
        assert_eq!(
            rename_channel(&[], 2, "Bass", 4).unwrap(),
            vec![
                plain("", "other"),
                plain("", "other"),
                plain("Bass", "other"),
                plain("", "other")
            ]
        );
        assert_eq!(
            set_channel_kind(&table, 2, "keys", 4).unwrap()[2],
            plain("", "keys")
        );
        assert!(
            rename_channel(&table, 1, &"x".repeat(33), 4)
                .unwrap_err()
                .contains("CH 2")
        );
        assert!(
            set_channel_kind(&table, 1, "kazoo", 4)
                .unwrap_err()
                .contains("kazoo")
        );
    }

    #[test]
    fn hiding_or_showing_one_side_of_a_pair_hides_or_shows_both() {
        let table = vec![
            detail("Keys L", "keys", false, true),
            plain("Keys R", "keys"),
            plain("Vox", "voice"),
        ];
        let hidden = |next: Vec<SessionChannel>| -> Vec<bool> {
            next.iter().map(|entry| entry.hidden).collect()
        };
        assert_eq!(
            hidden(set_channel_hidden(&table, 1, true, 3).unwrap()),
            [true, true, false]
        );
        let shown = set_channel_hidden(
            &set_channel_hidden(&table, 0, true, 3).unwrap(),
            0,
            false,
            3,
        );
        assert_eq!(hidden(shown.unwrap()), [false, false, false]);
        assert_eq!(
            hidden(set_channel_hidden(&table, 2, true, 3).unwrap()),
            [false, false, true]
        );
        assert_eq!(
            hidden(set_channel_hidden(&table, 0, true, 1).unwrap()),
            [true, true, false],
            "a pair whose second side the device lacks is still a pair in the table"
        );
    }

    #[test]
    fn linking_pairs_an_input_with_the_next_unlinking_works_from_either_side() {
        let table = vec![
            plain("A", "other"),
            plain("B", "other"),
            plain("C", "other"),
            plain("D", "other"),
        ];
        let links = |next: &[SessionChannel]| -> Vec<bool> {
            next.iter().map(|entry| entry.link).collect()
        };
        let linked = set_channel_link(&table, 1, true, 4).unwrap();
        assert_eq!(links(&linked), [false, true, false, false]);
        assert_eq!(
            links(&set_channel_link(&linked, 2, false, 4).unwrap()),
            [false; 4],
            "from the second side"
        );
        assert_eq!(
            links(&set_channel_link(&linked, 1, false, 4).unwrap()),
            [false; 4]
        );
        assert!(
            set_channel_link(&linked, 0, true, 4)
                .unwrap_err()
                .contains("CH 2"),
            "CH 2 is already in a pair"
        );
        assert!(
            set_channel_link(&table, 3, true, 4)
                .unwrap_err()
                .contains("CH 4"),
            "the last input"
        );
    }

    #[test]
    fn a_kind_a_newer_build_wrote_passes_an_edit_untouched_and_only_a_choice_replaces_it() {
        let table = vec![
            plain("Kick", "drums"),
            detail("Theremin", "theremin", true, false),
        ];
        assert_eq!(
            rename_channel(&table, 0, "Kick in", 2).unwrap()[1].kind,
            "theremin",
            "sent back as the desk keeps it"
        );
        assert_eq!(
            set_channel_hidden(&table, 1, false, 2).unwrap()[1].kind,
            "theremin"
        );
        assert_eq!(
            set_channel_kind(&table, 1, "other", 2).unwrap()[1].kind,
            "other"
        );
        assert_eq!(
            set_channel_kind(&table, 1, "keys", 2).unwrap()[1].kind,
            "keys"
        );
        assert!(
            set_channel_kind(&table, 0, "theremin", 2)
                .unwrap_err()
                .starts_with("CH 1 has the instrument kind `theremin`"),
            "an unknown kind the desk does not keep for that input"
        );
    }

    #[test]
    fn the_tables_processing_links_open_a_pair_on_its_first_side_and_say_on_only_when_it_is() {
        let report = status_with(
            &[
                plain("Kick", "drums"),
                plain("Snare", "drums"),
                detail("Keys L", "keys", false, true),
                plain("Keys R", "keys"),
            ],
            Vec::new(),
        );
        let mut state = state_of(report);
        let unread = channel_rows(&state);
        let link = row_processing_link(&state, &unread[0]);
        assert_eq!(
            (link.state, link.word, link.text),
            (ProcessingState::Unknown, "", "Processing")
        );
        assert_eq!(link.aria_label, "Processing of Kick, not read yet");

        let mut compressed = StripSettings::default();
        compressed.compressor.enabled = true;
        let strips = (1..=32)
            .map(|channel| ChannelProcessingDto {
                channel,
                settings: if channel == 3 {
                    compressed
                } else {
                    StripSettings::default()
                },
                active: channel == 3,
            })
            .collect();
        state.reduce(&Action::Processing(Some(strips)));
        let rows = channel_rows(&state);
        let second_side = row_processing_link(&state, &rows[3]);
        assert_eq!(
            second_side.route,
            Route {
                section: Section::Processing,
                channel: Some(3)
            },
            "CH 4 is the second side of CH 3-4"
        );
        assert_eq!(second_side.word, "On");
        assert_eq!(row_processing_link(&state, &rows[2]).word, "On");
        assert_eq!(row_processing_link(&state, &rows[2]).text, "Processing on");
        let kick = row_processing_link(&state, &rows[0]);
        assert_eq!((kick.word, kick.route.channel), ("Off", Some(1)));
        assert_eq!(kick.aria_label, "Processing of Kick, off");
    }
}
