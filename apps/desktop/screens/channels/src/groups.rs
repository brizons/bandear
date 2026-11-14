//! The groups of the Channels and groups screen (`channels-view.js`): the groups as the desk takes
//! them, the group edits, the cards, and the boxes held as the engineer clicked them. Pure.
//!
//! Every group edit is a function of the whole list the desk keeps ([GroupDefinition]s with
//! 1-based channels, no id for a new group). An edit returns a new list and leaves the one given
//! as it was; a list the desk would refuse is an `Err` that says why.

use std::collections::BTreeMap;

use bandear_desktop_console::mix::MAX_GROUPS;
use bandear_desktop_console::state::{ConsoleState, pair_units};
use bandear_desktop_console::types::dto::GroupRequest;
use bandear_desktop_console::values::{GroupTone, group_tone};

use crate::channels::label_of;
use crate::table::{ChannelRow, device_channels};

/// A group as the desk takes it (`set_groups`): its id (none for a new group), its name and its
/// inputs, numbered from 1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupDefinition {
    pub id: Option<String>,
    pub name: String,
    pub channels: Vec<usize>,
}

impl GroupDefinition {
    /// The definition as the `set_groups` request carries it.
    pub fn request(&self) -> GroupRequest {
        GroupRequest {
            id: self.id.clone(),
            name: self.name.clone(),
            channels: self.channels.clone(),
        }
    }
}

/// Groups as the desk takes them, from groups whose channels count from `first`: 0 for a status's
/// groups, 1 for the groups a `set_groups` answer carries. Every group edit is built on this,
/// whichever list it starts from.
pub fn group_definitions<'a>(
    groups: impl IntoIterator<Item = (&'a str, &'a str, &'a [usize])>,
    first: usize,
) -> Vec<GroupDefinition> {
    groups
        .into_iter()
        .map(|(id, name, channels)| GroupDefinition {
            id: Some(id.to_owned()),
            name: name.to_owned(),
            channels: channels.iter().map(|channel| channel - first + 1).collect(),
        })
        .collect()
}

/// The groups a status shows, as the desk takes them; none without a status.
pub fn group_definitions_of(state: &ConsoleState) -> Vec<GroupDefinition> {
    let groups = state
        .engine
        .status
        .as_ref()
        .map(|status| status.groups.as_slice())
        .unwrap_or(&[]);
    group_definitions(
        groups.iter().map(|group| {
            (
                group.id.as_str(),
                group.name.as_str(),
                group.channels.as_slice(),
            )
        }),
        0,
    )
}

/// `groups` with a new, empty group at the end, named "Group 3" or the next free number.
pub fn add_group(groups: &[GroupDefinition]) -> Result<Vec<GroupDefinition>, String> {
    if groups.len() >= MAX_GROUPS {
        return Err(format!("there can be at most {MAX_GROUPS} groups"));
    }
    let taken: Vec<String> = groups
        .iter()
        .map(|group| group.name.trim().to_lowercase())
        .collect();
    let mut number = groups.len() + 1;
    while taken.contains(&format!("group {number}")) {
        number += 1;
    }
    let mut next = groups.to_vec();
    next.push(GroupDefinition {
        id: None,
        name: format!("Group {number}"),
        channels: Vec::new(),
    });
    Ok(next)
}

fn require_group(groups: &[GroupDefinition], id: &str) -> Result<(), String> {
    if groups.iter().any(|group| group.id.as_deref() == Some(id)) {
        Ok(())
    } else {
        Err("that group is no longer there".to_owned())
    }
}

/// `groups` with group `id` called `name` (trimmed, never empty).
pub fn rename_group(
    groups: &[GroupDefinition],
    id: &str,
    name: &str,
) -> Result<Vec<GroupDefinition>, String> {
    require_group(groups, id)?;
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("a group needs a name".to_owned());
    }
    Ok(groups
        .iter()
        .map(|group| {
            let mut group = group.clone();
            if group.id.as_deref() == Some(id) {
                group.name = trimmed.to_owned();
            }
            group
        })
        .collect())
}

/// `groups` with the inputs `numbers` (1-based: one input, or both sides of a pair) in group `id`
/// (`member`) or out of it. An input is in one group at most, so joining one group leaves any other.
pub fn set_group_members(
    groups: &[GroupDefinition],
    id: &str,
    numbers: &[usize],
    member: bool,
) -> Result<Vec<GroupDefinition>, String> {
    require_group(groups, id)?;
    Ok(groups
        .iter()
        .map(|group| {
            let this = group.id.as_deref() == Some(id);
            let mut group = group.clone();
            if this || member {
                group.channels.retain(|channel| !numbers.contains(channel));
            }
            if this && member {
                group.channels.extend_from_slice(numbers);
                group.channels.sort_unstable();
            }
            group
        })
        .collect())
}

/// `groups` without group `id`.
pub fn delete_group(groups: &[GroupDefinition], id: &str) -> Result<Vec<GroupDefinition>, String> {
    require_group(groups, id)?;
    Ok(groups
        .iter()
        .filter(|group| group.id.as_deref() != Some(id))
        .cloned()
        .collect())
}

/// Whether another group can be made: the engine takes at most 16.
pub fn can_add_group(state: &ConsoleState) -> bool {
    state
        .engine
        .status
        .as_ref()
        .map_or(0, |status| status.groups.len())
        < MAX_GROUPS
}

/// One input choice of a group card: a device input, a pair once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupChoice {
    /// The first side, 1-based, as text.
    pub key: String,
    /// The inputs it stands for, 1-based.
    pub numbers: Vec<usize>,
    /// "CH 1 Kick", "CH 2-3 OH L", "CH 4".
    pub label: String,
    pub checked: bool,
    /// The other group an input is in: choosing it moves it.
    pub owner: Option<String>,
    pub hidden: bool,
}

/// One card per group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupCard {
    pub id: String,
    pub name: String,
    pub tone: GroupTone,
    pub invalid: bool,
    /// "Kick", "OH L (stereo)", "CH 9 (not on this device)".
    pub members: Vec<String>,
    /// "1 channel", "2 channels".
    pub count: String,
    pub choices: Vec<GroupChoice>,
}

/// One card per group, in the engine's order, with its members by name (a pair once, an input the
/// device lacks as such) and a choice for every input of the device.
pub fn group_cards(state: &ConsoleState) -> Vec<GroupCard> {
    let status = state.engine.status.as_ref();
    let channels = device_channels(status);
    let units = pair_units(channels);
    let groups = status.map(|status| status.groups.as_slice()).unwrap_or(&[]);
    let first_name = |unit: &[usize]| channels[unit[0]].name.trim().to_owned();
    groups
        .iter()
        .enumerate()
        .map(|(position, group)| {
            let in_group = |unit: &[usize]| unit.iter().any(|index| group.channels.contains(index));
            let mut members: Vec<String> = units
                .iter()
                .filter(|unit| in_group(unit))
                .map(|unit| {
                    let label = label_of(&channels[unit[0]].name, unit[0] + 1);
                    if unit.len() == 2 {
                        format!("{label} (stereo)")
                    } else {
                        label
                    }
                })
                .collect();
            for index in &group.channels {
                if *index >= channels.len() {
                    members.push(format!("CH {} (not on this device)", index + 1));
                }
            }
            let choices = units
                .iter()
                .map(|unit| {
                    let owner = groups.iter().find(|other| {
                        other.id != group.id
                            && unit.iter().any(|index| other.channels.contains(index))
                    });
                    let range = if unit.len() == 2 {
                        format!("CH {}-{}", unit[0] + 1, unit[1] + 1)
                    } else {
                        format!("CH {}", unit[0] + 1)
                    };
                    let name = first_name(unit);
                    GroupChoice {
                        key: (unit[0] + 1).to_string(),
                        numbers: unit.iter().map(|index| index + 1).collect(),
                        label: if name.is_empty() {
                            range
                        } else {
                            format!("{range} {name}")
                        },
                        checked: in_group(unit),
                        owner: owner.map(|owner| owner.name.clone()),
                        hidden: channels[unit[0]].hidden,
                    }
                })
                .collect();
            let count = format!(
                "{} {}",
                members.len(),
                if members.len() == 1 {
                    "channel"
                } else {
                    "channels"
                }
            );
            GroupCard {
                id: group.id.clone(),
                name: group.name.clone(),
                tone: group_tone(Some(position as i64)),
                invalid: group.invalid,
                members,
                count,
                choices,
            }
        })
        .collect()
}

/// A box the engineer clicked: an input's Shown box, or a group's box for an input choice.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ToggleKey {
    /// Input `index` (0-based).
    Shown(usize),
    /// Group `id`'s box for the choice `key` (see [GroupChoice::key]).
    Member { id: String, key: String },
}

/// The boxes the engineer changed whose edits are not answered yet, so a redraw shows what was
/// clicked rather than the status it is about to replace (a box flipping back invites a second
/// click, which would queue the opposite edit). [PendingToggles::hold] keeps a value and returns
/// its token; [PendingToggles::release] forgets it once that edit is answered, unless a later click
/// holds the key now.
#[derive(Clone, Debug, Default)]
pub struct PendingToggles {
    held: BTreeMap<ToggleKey, (bool, u64)>,
    tokens: u64,
}

impl PendingToggles {
    /// Holds `value` for `key`; the token releases exactly this hold.
    pub fn hold(&mut self, key: ToggleKey, value: bool) -> u64 {
        self.tokens += 1;
        self.held.insert(key, (value, self.tokens));
        self.tokens
    }

    /// Forgets `key`'s value if the hold `token` is still the one kept.
    pub fn release(&mut self, key: &ToggleKey, token: u64) {
        if self.held.get(key).is_some_and(|(_, held)| *held == token) {
            self.held.remove(key);
        }
    }

    /// The value held for `key`.
    pub fn get(&self, key: &ToggleKey) -> Option<bool> {
        self.held.get(key).map(|(value, _)| *value)
    }
}

/// `rows` with the Shown boxes held in `pending` as the engineer set them; both rows of a pair
/// follow a box held on either.
pub fn with_pending_shown(rows: Vec<ChannelRow>, pending: &PendingToggles) -> Vec<ChannelRow> {
    rows.into_iter()
        .map(|mut row| {
            let mut shown = pending.get(&ToggleKey::Shown(row.index));
            if shown.is_none() && row.stereo.linked {
                shown = row
                    .stereo
                    .partner
                    .and_then(|partner| pending.get(&ToggleKey::Shown(partner)));
            }
            if let Some(shown) = shown {
                row.hidden = !shown;
            }
            row
        })
        .collect()
}

/// `cards` with the input boxes held in `pending` as the engineer set them.
pub fn with_pending_members(cards: Vec<GroupCard>, pending: &PendingToggles) -> Vec<GroupCard> {
    cards
        .into_iter()
        .map(|mut card| {
            for choice in &mut card.choices {
                let key = ToggleKey::Member {
                    id: card.id.clone(),
                    key: choice.key.clone(),
                };
                if let Some(checked) = pending.get(&key) {
                    choice.checked = checked;
                }
            }
            card
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use bandear_desktop_console::testing::sample_status;
    use bandear_desktop_console::wire::Group;

    use super::*;
    use crate::channels::detail;
    use crate::table::channel_rows;
    use crate::table::tests::{state_of, status_with};

    fn sample() -> Vec<GroupDefinition> {
        group_definitions_of(&state_of(sample_status()))
    }

    fn definition(id: &str, name: &str, channels: &[usize]) -> GroupDefinition {
        GroupDefinition {
            id: Some(id.to_owned()),
            name: name.to_owned(),
            channels: channels.to_vec(),
        }
    }

    #[test]
    fn a_statuss_groups_are_what_the_desk_sends_back() {
        assert_eq!(
            sample(),
            vec![
                definition("drums", "Drums", &[1, 2, 3]),
                definition("vocals", "Vocals", &[5, 6]),
            ]
        );
        assert!(group_definitions_of(&ConsoleState::new()).is_empty());
    }

    #[test]
    fn a_new_group_gets_a_free_name_and_there_are_at_most_16() {
        let groups = sample();
        let added = add_group(&groups).unwrap();
        assert_eq!(
            added.last(),
            Some(&GroupDefinition {
                id: None,
                name: "Group 3".to_owned(),
                channels: Vec::new(),
            })
        );
        assert_eq!(groups.len(), 2, "the list given is left as it was");
        assert_eq!(
            add_group(&[definition("a", "Group 1", &[])]).unwrap()[1].name,
            "Group 2"
        );
        assert_eq!(
            add_group(&[definition("a", " group 2 ", &[])]).unwrap()[1].name,
            "Group 3",
            "a name taken in any case or spacing is skipped"
        );
        let full: Vec<_> = (0..16)
            .map(|index| definition(&format!("g{index}"), &format!("G {index}"), &[]))
            .collect();
        assert_eq!(
            add_group(&full).unwrap_err(),
            "there can be at most 16 groups"
        );
    }

    #[test]
    fn groups_are_renamed_given_members_and_deleted_by_id() {
        let groups = sample();
        assert_eq!(
            rename_group(&groups, "vocals", "  Voices ").unwrap()[1].name,
            "Voices"
        );
        assert_eq!(
            rename_group(&groups, "vocals", "  ").unwrap_err(),
            "a group needs a name"
        );
        assert_eq!(
            rename_group(&groups, "gone", "X").unwrap_err(),
            "that group is no longer there"
        );
        let moved = set_group_members(&groups, "vocals", &[3], true).unwrap();
        let channels = |groups: &[GroupDefinition]| -> Vec<Vec<usize>> {
            groups.iter().map(|group| group.channels.clone()).collect()
        };
        assert_eq!(
            channels(&moved),
            [vec![1, 2], vec![3, 5, 6]],
            "an input is in one group: choosing it here moves it"
        );
        assert_eq!(
            set_group_members(&groups, "drums", &[1], false).unwrap()[0].channels,
            [2, 3]
        );
        assert_eq!(
            channels(&set_group_members(&groups, "drums", &[4, 5], true).unwrap()),
            [vec![1, 2, 3, 4, 5], vec![6]],
            "a pair joins with both inputs"
        );
        let left: Vec<_> = delete_group(&groups, "drums")
            .unwrap()
            .into_iter()
            .map(|group| group.id)
            .collect();
        assert_eq!(left, [Some("vocals".to_owned())]);
    }

    #[test]
    fn a_group_card_lists_its_members_by_name_a_pair_once_and_offers_every_input() {
        let report = status_with(
            &[
                detail("Kick", "drums", false, false),
                detail("OH L", "drums", false, true),
                detail("OH R", "drums", false, false),
                detail("", "other", false, false),
                detail("Spare", "other", true, false),
            ],
            vec![
                Group {
                    id: "kit".to_owned(),
                    name: "Drums".to_owned(),
                    channels: vec![0, 1, 2],
                    invalid: false,
                },
                Group {
                    id: "rack".to_owned(),
                    name: "Rack".to_owned(),
                    channels: vec![3, 8],
                    invalid: true,
                },
            ],
        );
        let cards = group_cards(&state_of(report));
        assert_eq!(cards.len(), 2);
        assert_eq!(
            (
                cards[0].id.as_str(),
                cards[0].name.as_str(),
                cards[0].tone,
                cards[0].invalid
            ),
            ("kit", "Drums", GroupTone::Teal, false)
        );
        assert_eq!(cards[0].members, ["Kick", "OH L (stereo)"]);
        assert_eq!(cards[0].count, "2 channels");
        assert_eq!((cards[1].tone, cards[1].invalid), (GroupTone::Blue, true));
        assert_eq!(cards[1].members, ["CH 4", "CH 9 (not on this device)"]);
        let choice =
            |key: &str, numbers: &[usize], label: &str, checked, owner: Option<&str>, hidden| {
                GroupChoice {
                    key: key.to_owned(),
                    numbers: numbers.to_vec(),
                    label: label.to_owned(),
                    checked,
                    owner: owner.map(str::to_owned),
                    hidden,
                }
            };
        assert_eq!(
            cards[0].choices,
            [
                choice("1", &[1], "CH 1 Kick", true, None, false),
                choice("2", &[2, 3], "CH 2-3 OH L", true, None, false),
                choice("4", &[4], "CH 4", false, Some("Rack"), false),
                choice("5", &[5], "CH 5 Spare", false, None, true),
            ]
        );
        let single = status_with(
            &[detail("Kick", "drums", false, false)],
            vec![Group {
                id: "kit".to_owned(),
                name: "Drums".to_owned(),
                channels: vec![0],
                invalid: false,
            }],
        );
        assert_eq!(group_cards(&state_of(single))[0].count, "1 channel");
    }

    #[test]
    fn groups_become_what_the_desk_sends_1_based_from_a_statuss_groups_or_an_answers() {
        let answer = [
            ("drums", "Drums", vec![1, 2, 3]),
            ("g1", "Group 3", vec![4]),
        ];
        let kept = group_definitions(
            answer
                .iter()
                .map(|(id, name, channels)| (*id, *name, channels.as_slice())),
            1,
        );
        assert_eq!(
            kept,
            vec![
                definition("drums", "Drums", &[1, 2, 3]),
                definition("g1", "Group 3", &[4])
            ],
            "an answer is already numbered from 1"
        );
        let status = sample_status();
        assert_eq!(
            group_definitions(
                status.status.groups.iter().map(|group| (
                    group.id.as_str(),
                    group.name.as_str(),
                    group.channels.as_slice()
                )),
                0
            ),
            sample()
        );
    }

    #[test]
    fn a_toggle_the_engineer_changed_shows_their_choice_until_its_edit_is_answered() {
        let report = status_with(
            &[
                detail("Kick", "drums", false, false),
                detail("Keys L", "keys", false, true),
                detail("Keys R", "keys", false, false),
            ],
            Vec::new(),
        );
        let rows = channel_rows(&state_of(report));
        let mut pending = PendingToggles::default();
        let hidden =
            |rows: Vec<ChannelRow>| -> Vec<bool> { rows.iter().map(|row| row.hidden).collect() };

        let first = pending.hold(ToggleKey::Shown(2), false);
        assert_eq!(
            hidden(with_pending_shown(rows.clone(), &pending)),
            [false, true, true],
            "a pair's two rows follow the box clicked on either"
        );
        let second = pending.hold(ToggleKey::Shown(2), true);
        pending.release(&ToggleKey::Shown(2), first);
        assert_eq!(
            hidden(with_pending_shown(rows.clone(), &pending)),
            [false, false, false],
            "the first edit's answer leaves the later click showing"
        );
        pending.release(&ToggleKey::Shown(2), second);
        assert_eq!(with_pending_shown(rows.clone(), &pending), rows);
        assert_eq!(pending.get(&ToggleKey::Shown(2)), None);

        let cards = group_cards(&state_of(sample_status()));
        pending.hold(
            ToggleKey::Member {
                id: "drums".to_owned(),
                key: "4".to_owned(),
            },
            true,
        );
        let shown = with_pending_members(cards.clone(), &pending);
        let box_four = |card: &GroupCard| {
            card.choices
                .iter()
                .find(|choice| choice.key == "4")
                .map(|choice| choice.checked)
        };
        assert_eq!(box_four(&shown[0]), Some(true));
        assert_eq!(
            shown[1], cards[1],
            "other groups' boxes are as the status has them"
        );
        assert_eq!(box_four(&cards[0]), Some(false));
    }

    #[test]
    fn another_group_can_be_made_until_the_engine_has_16() {
        assert!(can_add_group(&state_of(sample_status())));
        let mut report = sample_status();
        report.status.groups = (0..16)
            .map(|index| Group {
                id: format!("g{index}"),
                name: format!("G {index}"),
                channels: Vec::new(),
                invalid: false,
            })
            .collect();
        assert!(!can_add_group(&state_of(report)));
    }
}
