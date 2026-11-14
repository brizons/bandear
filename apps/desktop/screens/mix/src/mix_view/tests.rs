//! `mix-view.test.js` (the engineer's, the local mix's and the strips' links are in `tests_more`).

use bandear_desktop_console::mix::{EditValue, MixField, edits};
use bandear_desktop_console::state::{
    Action, AudioNotice, AudioNoticeKind, CommandAction, ConsoleState, Phase,
};
use bandear_desktop_console::testing::sample_status;
use bandear_desktop_console::values::GroupTone;
use bandear_desktop_kit::top_bar::EngineAction;

use super::*;
use crate::testing::{BASS, DRUMS, KEYS, LEAD, reduced, running, status_of, with_status};

fn one(key: &str, value: impl Into<EditValue>) -> Action {
    mix_edit(LEAD, &[(key, value.into())])
}

pub(super) fn mix_edit(address: &str, changes: &[(&str, EditValue)]) -> Action {
    let mut list = bandear_desktop_console::mix::Edits::new();
    for (key, value) in changes {
        list.extend(edits([(*key, *value)]).unwrap());
    }
    Action::MixEdit {
        address: address.to_owned(),
        edits: list,
    }
}

pub(super) fn engine(phase: Phase, error: Option<&str>) -> Action {
    Action::Engine {
        phase,
        error: error.map(str::to_owned),
    }
}

#[test]
fn the_top_bar_names_the_device_and_format_counts_the_phones_needing_attention_offers_stop() {
    assert_eq!(
        top_bar_view(&running()),
        TopBarView {
            phase: Phase::Running,
            device: Some("Demo interface".to_owned()),
            format: Some("48 kHz \u{b7} 24-bit \u{b7} 256".to_owned()),
            attention: 2,
            attention_label: Some("2 phones need attention".to_owned()),
            action: Some(EngineAction::Stop),
        }
    );
}

#[test]
fn while_the_engine_starts_or_stops_the_top_bar_offers_nothing_stopped_or_failed_offers_start() {
    let starting = with_status(running(), vec![engine(Phase::Starting, None)]);
    assert_eq!(top_bar_view(&starting).action, None);
    assert_eq!(top_bar_view(&starting).device, None);
    let stopping = with_status(running(), vec![engine(Phase::Stopping, None)]);
    assert_eq!(top_bar_view(&stopping).action, None);
    assert_eq!(
        top_bar_view(&ConsoleState::new()).action,
        Some(EngineAction::Start)
    );
    let failed = reduced(vec![engine(Phase::Error, Some("no device"))]);
    assert_eq!(top_bar_view(&failed).action, Some(EngineAction::Start));

    let mut two = sample_status();
    two.musicians.truncate(2);
    assert_eq!(
        top_bar_view(&reduced(vec![status_of(two.clone())])).attention_label,
        Some("1 phone needs attention".to_owned())
    );
    two.musicians.truncate(1);
    assert_eq!(
        top_bar_view(&reduced(vec![status_of(two)])).attention_label,
        None
    );
}

#[test]
fn a_tab_per_musician_in_the_engines_order_with_the_link_in_words() {
    let tabs = musician_tabs(&running());
    let shown: Vec<_> = tabs
        .iter()
        .map(|tab| {
            (
                tab.address.as_str(),
                tab.label.as_str(),
                tab.link.label.as_str(),
                tab.selected,
            )
        })
        .collect();
    assert_eq!(
        shown,
        [
            (LEAD, "Lead vocal", "Good", true),
            (BASS, "Bass", "Warning", false),
            (KEYS, "Keys", "No reports", false),
            (DRUMS, "Drums", "Link lost", false),
        ]
    );
    assert_eq!(tabs[1].link.level, TabLevel::Warning);
}

#[test]
fn a_channel_strip_per_input_named_as_the_engineer_named_it_with_its_kind_group_levels_and_buttons()
{
    let strips = channel_strips(&running(), LEAD);

    assert_eq!(strips.len(), 6);
    assert_eq!(
        strips[0],
        ChannelStrip {
            index: 0,
            sides: vec![0],
            stereo: false,
            label: "Kick".to_owned(),
            kind: "drums".to_owned(),
            group: Some(StripGroup {
                id: "drums".to_owned(),
                name: "Drums".to_owned(),
                tone: GroupTone::Teal,
                invalid: false,
            }),
            gain: 62,
            pan: 50,
            muted: false,
            solo: false,
            tone: StripTone::Default,
            readout: "62%".to_owned(),
            lockable: true,
            locked: false,
            lock_text: "Unlocked",
            following: None,
            processing: None,
        }
    );
    let labels: Vec<_> = strips.iter().map(|strip| strip.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "Kick",
            "Snare",
            "Overheads",
            "Bass",
            "Lead vox",
            "Backing vox"
        ]
    );
    assert_eq!(strips[3].group, None, "CH 4 is in no group");
    assert_eq!(
        strips[4]
            .group
            .as_ref()
            .map(|group| (group.id.as_str(), group.tone)),
        Some(("vocals", GroupTone::Blue))
    );
    assert_eq!(strips[5].pan, 40);
}

/// The sample with Keys on CH 2-3 as a pair panned `pans`, a hidden spare on CH 4 and CH 5 unnamed.
fn paired_state(pans: [u8; 2]) -> ConsoleState {
    let mut status = sample_status();
    let details = [
        ("Kick", "drums", false, false),
        ("Keys L", "keys", false, true),
        ("Keys R", "keys", false, false),
        ("Spare", "other", true, false),
        ("", "other", false, false),
        ("Backing vox", "voice", false, false),
    ];
    for (channel, (name, kind, hidden, link)) in status.status.channels.iter_mut().zip(details) {
        channel.name = name.to_owned();
        channel.kind = kind.to_owned();
        channel.hidden = hidden;
        channel.link = link;
    }
    let lead = &mut status.musicians[0].mix;
    lead.pans[1] = pans[0];
    lead.pans[2] = pans[1];
    lead.channel_gains[..6].copy_from_slice(&[62, 70, 70, 58, 88, 52]);
    lead.channel_muted[1] = true;
    lead.channel_muted[2] = true;
    reduced(vec![status_of(status)])
}

#[test]
fn a_hidden_input_has_no_strip_and_a_pair_is_one_stereo_strip_whose_pan_is_its_balance() {
    let strips = channel_strips(&paired_state([20, 100]), LEAD);
    let shown: Vec<_> = strips
        .iter()
        .map(|strip| {
            (
                strip.index,
                strip.sides.clone(),
                strip.stereo,
                strip.label.as_str(),
            )
        })
        .collect();
    assert_eq!(
        shown,
        [
            (0, vec![0], false, "Kick"),
            (1, vec![1, 2], true, "Keys L"),
            (4, vec![4], false, "CH 5"),
            (5, vec![5], false, "Backing vox"),
        ]
    );
    let keys = &strips[1];
    assert_eq!(keys.kind, "keys");
    assert_eq!(keys.pan, 60, "the balance the two pans make");
    assert_eq!(keys.gain, 70);
    assert!(
        keys.muted,
        "the pair's level, mute and solo are its first side's"
    );
    assert_eq!(keys.readout, "muted");
    assert_eq!(
        channel_strips(&paired_state([0, 100]), LEAD)[1].pan,
        50,
        "a new pair is centred"
    );
}

#[test]
fn a_stereo_strips_level_mute_and_solo_write_both_inputs_and_its_balance_both_pans() {
    let pair = [1, 2];
    assert_eq!(
        channel_edits(&pair, true, MixField::ChannelGains, 40.into()),
        edits([("channelGains.1", 40.into()), ("channelGains.2", 40.into())]).unwrap()
    );
    assert_eq!(
        channel_edits(&pair, true, MixField::ChannelSolo, true.into()),
        edits([
            ("channelSolo.1", true.into()),
            ("channelSolo.2", true.into())
        ])
        .unwrap()
    );
    assert_eq!(
        channel_edits(&pair, true, MixField::Pans, 70.into()),
        edits([("pans.1", 40.into()), ("pans.2", 100.into())]).unwrap()
    );
    assert_eq!(
        channel_edits(&[3], false, MixField::Pans, 30.into()),
        edits([("pans.3", 30.into())]).unwrap()
    );
    assert_eq!(
        channel_edits(&[3], false, MixField::ChannelMuted, true.into()),
        edits([("channelMuted.3", true.into())]).unwrap()
    );
}

#[test]
fn a_strips_meter_shows_its_input_or_the_louder_side_of_a_pair() {
    assert_eq!(strip_level(&[0], &[10, 20, 30]), 10);
    assert_eq!(strip_level(&[1, 2], &[10, 20, 30]), 30);
    assert_eq!(
        strip_level(&[4], &[10, 20, 30]),
        0,
        "an input the meters do not cover yet"
    );
}

#[test]
fn a_channel_strip_finds_its_group_by_id_so_two_groups_may_share_a_name() {
    let mut status = sample_status();
    let group = |id: &str, channel: usize| bandear_desktop_console::wire::Group {
        id: id.to_owned(),
        name: "Drums".to_owned(),
        channels: vec![channel],
        invalid: false,
    };
    status.status.groups = vec![group("kit-a", 0), group("kit-b", 1)];
    for (index, channel) in status.status.channels.iter_mut().enumerate() {
        channel.group = (index < 2).then(|| "Drums".to_owned());
        channel.group_id = ["kit-a", "kit-b"].get(index).map(|id| (*id).to_owned());
    }

    let strips = channel_strips(&reduced(vec![status_of(status)]), LEAD);

    let tone = |strip: &ChannelStrip| {
        strip
            .group
            .as_ref()
            .map(|group| (group.id.clone(), group.tone))
    };
    assert_eq!(
        tone(&strips[0]),
        Some(("kit-a".to_owned(), GroupTone::Teal))
    );
    assert_eq!(
        tone(&strips[1]),
        Some(("kit-b".to_owned(), GroupTone::Blue))
    );
    assert_eq!(strips[2].group, None);
}

#[test]
fn a_muted_channels_strip_says_so_in_words_not_only_in_colour() {
    let strip = &channel_strips(&running(), DRUMS)[3];
    assert!(strip.muted);
    assert_eq!(strip.tone, StripTone::Muted);
    assert_eq!(strip.readout, "muted");
}

#[test]
fn the_strips_show_local_edits_still_on_their_way() {
    let edited = with_status(
        running(),
        vec![mix_edit(
            LEAD,
            &[
                ("channelGains.1", 33.into()),
                ("channelSolo.2", true.into()),
            ],
        )],
    );
    let strips = channel_strips(&edited, LEAD);
    assert_eq!(strips[1].gain, 33);
    assert_eq!(strips[1].readout, "33%");
    assert!(strips[2].solo);
}

#[test]
fn a_group_strip_per_configured_group_in_its_chips_tone_with_the_musicians_group_level() {
    let edited = with_status(running(), vec![one("groupLevels.1", 70)]);
    assert_eq!(
        group_strips(&edited, LEAD),
        [
            GroupStrip {
                index: 0,
                id: "drums".to_owned(),
                name: "Drums".to_owned(),
                tone: GroupTone::Teal,
                invalid: false,
                level: 100,
                readout: "100%".to_owned(),
                following: None,
            },
            GroupStrip {
                index: 1,
                id: "vocals".to_owned(),
                name: "Vocals".to_owned(),
                tone: GroupTone::Blue,
                invalid: false,
                level: 70,
                readout: "70%".to_owned(),
                following: None,
            },
        ]
    );
}

#[test]
fn the_master_strip_carries_the_musicians_level_level_cap_the_engineers_limit_and_mute() {
    assert_eq!(
        master_strip(&running(), LEAD),
        Some(MasterStrip {
            volume: 85,
            cap: Some(90),
            limit: Some(100),
            muted: false,
            readout: "85% \u{b7} cap 90% \u{b7} limit 100%".to_owned(),
            ceiling_note: Some(
                "The lower of the phone's cap and your limit wins: at most 90%.".to_owned()
            ),
        })
    );
    let muted = with_status(running(), vec![one("muted", true)]);
    assert_eq!(
        master_strip(&muted, LEAD).unwrap().readout,
        "muted \u{b7} cap 90% \u{b7} limit 100%"
    );
}

#[test]
fn without_that_musician_there_is_nothing_to_strip() {
    assert!(channel_strips(&running(), "10.0.0.1:50000").is_empty());
    assert!(group_strips(&running(), "10.0.0.1:50000").is_empty());
    assert_eq!(master_strip(&running(), "10.0.0.1:50000"), None);
}

#[test]
fn the_mix_area_says_why_it_is_empty() {
    let mut nobody = sample_status();
    nobody.musicians.clear();
    let mut stopped = sample_status();
    stopped.status.stopped = true;

    assert_eq!(
        mix_notice(&ConsoleState::new()),
        Some(MixNotice::Connecting)
    );
    assert_eq!(
        mix_notice(&reduced(vec![Action::Connected])),
        Some(MixNotice::Stopped)
    );
    assert_eq!(
        mix_notice(&with_status(
            running(),
            vec![Action::ConnectionFailed("no events".to_owned())]
        )),
        Some(MixNotice::Disconnected(Some("no events".to_owned())))
    );
    assert_eq!(
        mix_notice(&reduced(vec![engine(Phase::Starting, None)])),
        Some(MixNotice::Starting)
    );
    assert_eq!(
        mix_notice(&with_status(running(), vec![engine(Phase::Stopping, None)])),
        Some(MixNotice::Stopping)
    );
    assert_eq!(
        mix_notice(&reduced(vec![engine(
            Phase::Error,
            Some("No input device.")
        )])),
        Some(MixNotice::Error(Some("No input device.".to_owned())))
    );
    assert_eq!(
        mix_notice(&reduced(vec![status_of(nobody)])),
        Some(MixNotice::NoMusicians)
    );
    assert_eq!(
        mix_notice(&reduced(vec![status_of(stopped)])),
        Some(MixNotice::Stopped)
    );
    assert_eq!(mix_notice(&running()), None);
}

#[test]
fn each_notice_has_the_web_consoles_words_and_action() {
    let said = |notice: MixNotice| (notice.title(), notice.text(), notice.action());
    assert_eq!(
        said(MixNotice::Disconnected(Some("no events".to_owned()))),
        (
            "The console cannot reach the engine",
            Some("no events".to_owned()),
            Some(("Try again", NoticeAction::Reconnect))
        )
    );
    assert_eq!(
        said(MixNotice::Connecting),
        ("Connecting to the engine\u{2026}", None, None)
    );
    assert_eq!(
        said(MixNotice::Stopped),
        (
            "The engine is stopped",
            Some("Start it to send each musician their mix.".to_owned()),
            Some(("Start engine", NoticeAction::Start))
        )
    );
    assert_eq!(
        said(MixNotice::Starting),
        (
            "Starting the engine\u{2026}",
            Some("Opening the audio input.".to_owned()),
            None
        )
    );
    assert_eq!(
        said(MixNotice::Stopping),
        (
            "Stopping the engine\u{2026}",
            Some("Closing the audio input.".to_owned()),
            None
        )
    );
    assert_eq!(
        said(MixNotice::Error(Some("No input device.".to_owned()))),
        (
            "The engine is not running",
            Some("No input device.".to_owned()),
            Some(("Try again", NoticeAction::Start))
        )
    );
    let none = MixNotice::NoMusicians;
    assert_eq!(none.title(), "No phones yet");
    assert_eq!(
        none.text().unwrap(),
        "Musicians appear here when they open BandEar on their phone and join this computer. You can also add a phone by address in "
    );
    assert!(none.links_network());
    assert_eq!(none.action(), None);
}

#[test]
fn the_console_says_who_was_removed_and_which_musicians_change_was_refused() {
    let mut without_bass = sample_status();
    without_bass
        .musicians
        .retain(|musician| musician.address != BASS);
    let removed = with_status(
        running(),
        vec![Action::Select(BASS.to_owned()), status_of(without_bass)],
    );
    assert_eq!(
        mix_messages(&removed),
        MixMessages {
            departed: Some("Bass was removed; showing Lead vocal's mix.".to_owned()),
            ..MixMessages::default()
        }
    );

    let sent = edits([("volumePercent", 10.into())]).unwrap();
    let refused = with_status(
        running(),
        vec![
            one("volumePercent", 10),
            Action::MixFailed {
                address: LEAD.to_owned(),
                sent,
                error: "the engine is not running".to_owned(),
            },
        ],
    );
    assert_eq!(
        mix_messages(&refused),
        MixMessages {
            error: Some(
                "A change to Lead vocal's mix was not applied: the engine is not running."
                    .to_owned()
            ),
            ..MixMessages::default()
        }
    );

    let unstopped = with_status(
        running(),
        vec![
            engine(Phase::Stopping, None),
            Action::CommandFailed {
                action: CommandAction::Stop,
                message: "could not stop the capture".to_owned(),
            },
        ],
    );
    assert_eq!(
        mix_messages(&unstopped).command.as_deref(),
        Some("The engine could not be stopped: could not stop the capture.")
    );
}

#[test]
fn the_console_says_when_the_engine_runs_on_another_input_than_the_saved_one() {
    let message = "The saved input Volt was not found, so the engine started on the default.";
    let fell_back = with_status(
        running(),
        vec![Action::AudioNotice(Some(AudioNotice {
            kind: AudioNoticeKind::Fallback,
            message: message.to_owned(),
        }))],
    );
    assert_eq!(mix_messages(&fell_back).audio.as_deref(), Some(message));
    assert_eq!(mix_messages(&running()).audio, None);
}
