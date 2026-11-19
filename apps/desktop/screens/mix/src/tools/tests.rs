//! The window's parts of `mixes.test.js` (the engine's rules for presets and copies are the desk's
//! own, tested in the core) and `mix-tools-logic.test.js`, with a stand-in clock.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use bandear_desktop_console::state::ConsoleState;
use bandear_desktop_console::testing::sample_status;
use bandear_desktop_console::types::dto::MirrorSetDto;
use bandear_desktop_console::wire::Mirror;

use super::*;
use crate::testing::{BASS, DRUMS, KEYS, LEAD, reduced, status_of};

/// The sample engine with `mirrors` on its rows: follower to `(leader's address, name)`.
fn with_mirrors(mirrors: &[(&str, &str, &str)]) -> ConsoleState {
    let mirrors: BTreeMap<_, _> = mirrors
        .iter()
        .map(|(follower, leader, name)| (*follower, (*leader, *name)))
        .collect();
    let mut status = sample_status();
    for row in &mut status.musicians {
        row.mirror = mirrors
            .get(row.address.as_str())
            .map(|(leader, name)| Mirror {
                leader: format!("address:{leader}"),
                leader_address: Some((*leader).to_owned()),
                name: (*name).to_owned(),
            });
    }
    reduced(vec![status_of(status)])
}

#[test]
fn a_mirror_that_would_go_round_in_a_circle_is_found_directly_or_through_others() {
    let state = with_mirrors(&[(BASS, LEAD, "Lead vocal"), (KEYS, BASS, "Bass")]);
    let musicians = &state.musicians;
    assert!(closes_circle(musicians, LEAD, BASS), "Bass follows Lead");
    assert!(
        closes_circle(musicians, LEAD, KEYS),
        "Keys follows Bass, who follows Lead"
    );
    assert!(
        closes_circle(musicians, LEAD, LEAD),
        "nobody follows their own mix"
    );
    assert!(!closes_circle(musicians, KEYS, DRUMS));
    assert!(!closes_circle(musicians, DRUMS, KEYS));
}

#[test]
fn the_mirror_control_offers_every_other_musician_but_never_a_circle_and_says_whom_the_mix_follows()
{
    let state = with_mirrors(&[(BASS, LEAD, "Lead vocal")]);
    let lead = mirror_choice(&state, LEAD);
    let bass = mirror_choice(&state, BASS);
    let labels: Vec<_> = lead
        .options
        .iter()
        .map(|option| option.label.as_str())
        .collect();
    assert_eq!(
        labels,
        ["Off", "Keys", "Drums"],
        "Bass follows Lead, so Lead cannot follow Bass"
    );
    assert_eq!(lead.value, "");
    assert_eq!(lead.note, None);
    assert_eq!(bass.value, LEAD);
    let values: Vec<_> = bass
        .options
        .iter()
        .map(|option| option.value.as_str())
        .collect();
    assert_eq!(values, ["", LEAD, KEYS, DRUMS]);
    assert_eq!(
        bass.note.as_deref(),
        Some(
            "Bass follows Lead vocal's mix: its channels and groups move with it, its master stays its own."
        )
    );
}

#[test]
fn a_follower_whose_leader_is_away_keeps_the_last_mix_and_the_mirror_control_says_so() {
    let mut status = sample_status();
    status.musicians[1].mirror = Some(Mirror {
        leader: "client:gone".to_owned(),
        leader_address: None,
        name: "Ana".to_owned(),
    });
    let choice = mirror_choice(&reduced(vec![status_of(status)]), BASS);
    assert_eq!(choice.value, AWAY);
    assert_eq!(
        choice.options.last(),
        Some(&MirrorOption::new(AWAY, "Ana (not here)"))
    );
    assert_eq!(
        choice.note.as_deref(),
        Some(
            "Bass follows Ana's mix. Ana is not here, so Bass's mix stays as Ana left it until they are back."
        )
    );
}

#[test]
fn a_copy_or_a_preset_reaches_every_other_musician_never_one_whose_mix_follows_anothers() {
    let state = with_mirrors(&[(KEYS, BASS, "Bass")]);
    let target = |address: &str, label: &str, disabled: bool, note: Option<&str>| Target {
        address: address.to_owned(),
        label: label.to_owned(),
        disabled,
        note: note.map(str::to_owned),
    };
    assert_eq!(
        mix_targets(&state, LEAD, false),
        [
            target(BASS, "Bass", false, None),
            target(KEYS, "Keys", true, Some("follows Bass")),
            target(DRUMS, "Drums", false, None),
        ]
    );
    let all = mix_targets(&state, KEYS, true);
    assert_eq!(
        all[0],
        target(KEYS, "Keys", true, Some("this mix, follows Bass"))
    );
    assert_eq!(all.len(), 4);
    assert_eq!(
        mix_targets(&state, BASS, true)[0],
        target(BASS, "Bass", false, Some("this mix"))
    );
}

fn mirror_set(
    leader: Option<&str>,
    applied: bool,
    saved: bool,
    save_error: Option<&str>,
) -> MirrorSetDto {
    MirrorSetDto {
        address: BASS.to_owned(),
        leader: leader.map(str::to_owned),
        name: None,
        kept_for: "address",
        applied,
        saved,
        save_error: save_error.map(str::to_owned),
        mix: None,
    }
}

#[test]
fn the_desks_answers_to_presets_copies_and_mirrors_read_as_sentences() {
    let names = |list: &[&str]| {
        list.iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(join_names(&names(&["Bass"])), "Bass");
    assert_eq!(
        join_names(&names(&["Bass", "Keys", "Drums"])),
        "Bass, Keys and Drums"
    );
    assert_eq!(
        format_copied("Lead vocal", &names(&["Bass", "Keys"])),
        "Copied Lead vocal's mix to Bass and Keys: channels, pans, mutes, solos and group levels."
    );
    assert_eq!(
        format_preset_applied("Verse", &names(&["Bass"])),
        "Applied \"Verse\" to Bass."
    );
    let followed = mirror_set(Some(LEAD), true, true, None);
    assert_eq!(
        format_mirror_set(&followed, "Bass", Some("Lead vocal")),
        "Bass follows Lead vocal's mix now."
    );
    assert_eq!(
        format_mirror_set(
            &mirror_set(Some(LEAD), false, true, None),
            "Bass",
            Some("Lead vocal")
        ),
        "Bass will follow Lead vocal's mix from the next start."
    );
    assert_eq!(
        format_mirror_set(&mirror_set(None, true, true, None), "Bass", None),
        "Bass no longer follows another mix; their mix stays as it is."
    );
    assert_eq!(
        format_mirror_set(&mirror_set(None, false, true, None), "Bass", None),
        "Bass will follow no other mix from the next start."
    );
    assert_eq!(
        format_mirror_set(
            &mirror_set(Some(LEAD), true, false, Some("the disk is full.")),
            "Bass",
            Some("Lead vocal")
        ),
        "Bass follows Lead vocal's mix now, but this was not saved: the disk is full."
    );
}

// --- The tools' behaviour (`mix-tools-logic.test.js`) ----------------------------------------

fn target(address: &str, label: &str, disabled: bool) -> Target {
    Target {
        address: address.to_owned(),
        label: label.to_owned(),
        disabled,
        note: disabled.then(|| "follows Bass".to_owned()),
    }
}

/// The tools' data for a mix: Bass and Keys reachable unless `followers` names them.
struct Data<'a> {
    address: &'a str,
    followers: &'a [&'a str],
    can_use: bool,
    presets: Option<Vec<PresetItem>>,
    mirror: &'a str,
}

impl Default for Data<'_> {
    fn default() -> Self {
        Self {
            address: LEAD,
            followers: &[],
            can_use: true,
            presets: Some(Vec::new()),
            mirror: "",
        }
    }
}

fn data(data: Data<'_>) -> MixToolsView {
    let others: Vec<_> = [target(BASS, "Bass", false), target(KEYS, "Keys", false)]
        .into_iter()
        .filter(|other| other.address != data.address)
        .map(|other| {
            let follows = data.followers.contains(&other.address.as_str());
            target(&other.address, &other.label, follows)
        })
        .collect();
    let mut presets = vec![target(
        data.address,
        "Lead vocal",
        data.followers.contains(&data.address),
    )];
    presets.extend(others.clone());
    MixToolsView {
        address: data.address.to_owned(),
        name: "Lead vocal".to_owned(),
        mirror: MirrorChoice {
            value: data.mirror.to_owned(),
            options: vec![
                MirrorOption::new("", "Off"),
                MirrorOption::new(BASS, "Bass"),
                MirrorOption::new(KEYS, "Keys"),
            ],
            note: None,
        },
        copy_targets: others,
        preset_targets: presets,
        presets: data.presets,
        can_use: data.can_use,
    }
}

fn verse() -> Option<Vec<PresetItem>> {
    Some(vec![PresetItem {
        id: "p1".to_owned(),
        name: "Verse".to_owned(),
    }])
}

fn tools(view: MixToolsView) -> Tools {
    let mut tools = Tools::new();
    tools.set_data(view);
    tools
}

#[test]
fn the_mirror_control_shows_a_choice_on_its_way_then_the_desks_mirror_after_any_answer() {
    let mut tools = tools(data(Data::default()));

    assert_eq!(
        tools.choose_mirror(BASS),
        Some(ToolRequest::Mirror {
            leader: Some(BASS.to_owned())
        })
    );
    assert_eq!(
        tools.mirror_value(),
        BASS,
        "the choice shows while it is on its way"
    );
    tools.answered(ToolKind::Mirror, false);
    assert_eq!(
        tools.mirror_value(),
        "",
        "a refused choice gives way to the desk's mirror"
    );

    assert!(tools.choose_mirror(KEYS).is_some());
    let mut offered_none = data(Data::default());
    offered_none.mirror.options.truncate(1);
    tools.set_data(offered_none);
    assert_eq!(
        tools.mirror_value(),
        "",
        "a choice no longer offered shows the desk's mirror"
    );
    tools.set_data(data(Data {
        mirror: KEYS,
        ..Data::default()
    }));
    tools.answered(ToolKind::Mirror, true);
    assert_eq!(tools.mirror_value(), KEYS);
    assert_eq!(
        tools.choose_mirror(KEYS),
        None,
        "the mirror it has sends nothing"
    );
    assert_eq!(
        tools.choose_mirror(AWAY),
        None,
        "a leader away sends nothing"
    );
    assert_eq!(
        Tools::new().choose_mirror(BASS),
        None,
        "nothing open, nothing to send"
    );
}

#[test]
fn a_desk_mirror_that_changes_by_any_route_replaces_a_choice_still_on_its_way() {
    let mut tools = tools(data(Data::default()));
    tools.choose_mirror(BASS);

    tools.set_data(data(Data::default()));
    assert_eq!(
        tools.mirror_value(),
        BASS,
        "a status that leaves the desk's mirror keeps it"
    );
    let keys = || {
        data(Data {
            mirror: KEYS,
            ..Data::default()
        })
    };
    tools.set_data(keys());
    assert_eq!(
        tools.mirror_value(),
        KEYS,
        "the desk's new mirror shows, not the old choice"
    );
    tools.set_data(keys());
    assert_eq!(tools.mirror_value(), KEYS);
}

#[test]
fn ticks_go_with_the_open_musician_and_a_musician_who_starts_following_loses_theirs() {
    let mut tools = tools(data(Data::default()));
    tools.tick(TickList::Copy, BASS, true);
    tools.tick(TickList::Copy, KEYS, true);

    tools.set_data(data(Data {
        followers: &[KEYS],
        ..Data::default()
    }));
    tools.tick(TickList::Copy, KEYS, true);
    assert!(
        !tools.is_ticked(TickList::Copy, KEYS),
        "a follower cannot be ticked"
    );
    assert_eq!(
        tools.copy(),
        Some(ToolRequest::Copy {
            to: vec![BASS.to_owned()]
        })
    );
    tools.set_data(data(Data {
        address: BASS,
        ..Data::default()
    }));
    assert!(!tools.is_ticked(TickList::Copy, KEYS));
    assert!(
        !tools.copy_ready(),
        "nothing ticked for another musician's mix"
    );
}

#[test]
fn a_preset_goes_to_the_open_musician_to_begin_with_unless_their_mix_follows_anothers() {
    let mut tools = tools(data(Data {
        presets: verse(),
        ..Data::default()
    }));
    assert!(tools.is_ticked(TickList::Apply, LEAD));
    tools.tick(TickList::Apply, BASS, true);
    assert_eq!(
        tools.apply(Some("p1")),
        Some(ToolRequest::Apply {
            id: "p1".to_owned(),
            to: vec![LEAD.to_owned(), BASS.to_owned()]
        })
    );

    let following = self::tools(data(Data {
        presets: verse(),
        followers: &[LEAD],
        ..Data::default()
    }));
    assert!(!following.is_ticked(TickList::Apply, LEAD));
    assert!(!following.apply_ready(Some("p1")));
}

#[test]
fn nothing_is_copied_saved_or_applied_while_no_engine_runs_but_a_preset_can_be_renamed() {
    let mut tools = tools(data(Data {
        can_use: false,
        presets: verse(),
        ..Data::default()
    }));
    tools.tick(TickList::Copy, BASS, true);

    assert_eq!(tools.copy(), None);
    assert_eq!(tools.save("Chorus"), None);
    assert_eq!(tools.apply(Some("p1")), None);
    assert_eq!(
        tools.rename(Some("p1"), "  Bridge "),
        Some(ToolRequest::Rename {
            id: "p1".to_owned(),
            name: "Bridge".to_owned()
        })
    );
    assert_eq!(tools.rename(Some("p1"), "   "), None);
    assert!(!tools.save_ready("Chorus"));
}

#[test]
fn a_save_goes_out_trimmed_and_only_with_a_name() {
    let tools = tools(data(Data::default()));
    assert_eq!(
        tools.save("  Verse "),
        Some(ToolRequest::Save {
            name: "Verse".to_owned()
        })
    );
    assert_eq!(tools.save("  "), None);
}

#[test]
fn a_panel_closes_on_escape_and_gives_the_focus_back_to_its_button() {
    let mut tools = tools(data(Data {
        presets: None,
        ..Data::default()
    }));

    assert_eq!(tools.toggle(Panel::Copy), (Focus::Panel(Panel::Copy), None));
    assert_eq!(tools.open(), Some(Panel::Copy));
    assert_eq!(tools.escape(), Focus::Opener(Panel::Copy));
    assert_eq!(tools.open(), None);
    assert_eq!(
        tools.escape(),
        Focus::Stay,
        "nothing open, nothing to give back"
    );
    assert_eq!(
        tools.toggle(Panel::Presets),
        (Focus::Panel(Panel::Presets), Some(ToolRequest::LoadPresets)),
        "presets not read yet are asked for"
    );
    tools.toggle(Panel::Copy);
    assert_eq!(tools.open(), Some(Panel::Copy), "one panel at a time");
}

#[test]
fn a_delete_asks_first_works_only_after_the_pause_and_disarms_itself_after_ten_seconds() {
    let mut tools = tools(data(Data {
        presets: verse(),
        ..Data::default()
    }));
    let start = Instant::now();

    tools.arm("p1", start);
    assert_eq!(
        tools.delete_state("p1", start),
        DeleteState {
            armed: true,
            ready: false
        }
    );
    assert_eq!(
        tools.confirm_delete("p1", start + Duration::from_millis(100)),
        None,
        "a double click does not delete"
    );
    let later = start + DELETE_DELAY;
    assert_eq!(
        tools.delete_state("p1", later),
        DeleteState {
            armed: true,
            ready: true
        }
    );
    assert_eq!(
        tools.confirm_delete("p1", later),
        Some(ToolRequest::Delete {
            id: "p1".to_owned()
        })
    );

    let first = tools.arm("p1", later);
    assert!(tools.expire(first), "ten seconds on, it disarms");
    assert_eq!(
        tools.delete_state("p1", later + ARMED_FOR),
        DeleteState::default()
    );
    let kept = tools.arm("p1", later);
    tools.keep();
    assert!(!tools.expire(kept));
    assert_eq!(
        tools.confirm_delete("p1", later + ARMED_FOR),
        None,
        "Keep keeps it"
    );
    let stale = tools.arm("p1", later);
    let fresh = tools.arm("p1", later + Duration::from_secs(5));
    assert!(
        !tools.expire(stale),
        "an older arming's timer leaves a new one armed"
    );
    assert!(tools.delete_state("p1", later + ARMED_FOR).armed);
    assert!(tools.expire(fresh));
    tools.arm("p1", later);
    tools.set_data(data(Data {
        presets: Some(Vec::new()),
        ..Data::default()
    }));
    assert_eq!(
        tools.delete_state("p1", later + ARMED_FOR),
        DeleteState::default(),
        "a preset gone"
    );
    assert_eq!(ARMED_FOR, Duration::from_secs(10));
}

#[test]
fn an_answer_empties_the_field_of_a_done_save_or_rename_and_unticks_a_done_copy() {
    let mut tools = tools(data(Data::default()));
    tools.tick(TickList::Copy, BASS, true);

    assert_eq!(tools.answered(ToolKind::Save, false), Clears::default());
    assert_eq!(
        tools.answered(ToolKind::Save, true),
        Clears {
            save: true,
            rename: false
        }
    );
    assert_eq!(
        tools.answered(ToolKind::Rename, true),
        Clears {
            save: false,
            rename: true
        }
    );
    tools.answered(ToolKind::Copy, false);
    assert!(
        tools.is_ticked(TickList::Copy, BASS),
        "a refused copy keeps its musicians"
    );
    tools.answered(ToolKind::Copy, true);
    assert!(!tools.is_ticked(TickList::Copy, BASS));
}

#[test]
fn the_tools_show_the_open_musicians_name_the_presets_and_whether_the_engine_runs() {
    let state = with_mirrors(&[]);
    let view = mix_tools_view(&state, LEAD);
    assert_eq!(view.name, "Lead vocal");
    assert_eq!(view.presets, None, "not read yet");
    assert!(view.can_use);
    assert_eq!(view.copy_targets.len(), 3);
    assert_eq!(view.preset_targets[0].note.as_deref(), Some("this mix"));
    assert!(!mix_tools_view(&ConsoleState::new(), LEAD).can_use);
}
