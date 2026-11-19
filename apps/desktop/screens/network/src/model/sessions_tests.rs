//! `sessions.test.js` and the view half of `mix-backups.test.js`, ported (the console halves are
//! the model's flows' tests).

use bandear_desktop_console::state::{Action, Listing};
use bandear_desktop_console::testing::sample_status;
use bandear_desktop_console::types::dto::{LoadedMixDto, MixDto};

use super::*;

const UTC: Zone = Zone::Fixed(0);
/// 2026-10-16 19:05:07 UTC.
const AT: u64 = 1_792_177_507_000;
/// 2026-10-18 19:04:05 UTC.
const BACKED_UP: u64 = 1_792_350_245_000;

fn saved(id: &str, name: &str, saved_at: Option<u64>, mixes: usize) -> SnapshotSummary {
    SnapshotSummary {
        id: id.to_owned(),
        name: name.to_owned(),
        saved_at,
        mixes,
        problem: None,
    }
}

fn show() -> SnapshotSummary {
    saved("s1", "Show", Some(AT), 3)
}

fn broken() -> SnapshotSummary {
    SnapshotSummary {
        problem: Some("the file is not valid JSON".to_owned()),
        ..saved("broken", "broken", None, 0)
    }
}

#[test]
fn the_saved_sessions_are_listed_with_when_they_were_saved_and_their_mixes_or_why_not() {
    assert_eq!(
        session_rows(&[show(), saved("s2", "Solo", None, 1), broken()], UTC),
        [
            SessionRow {
                id: "s1".to_owned(),
                name: "Show".to_owned(),
                loadable: true,
                label: "Show".to_owned(),
                detail: "Saved 16 Oct 2026, 19:05 \u{b7} 3 musicians' mixes".to_owned(),
            },
            SessionRow {
                id: "s2".to_owned(),
                name: "Solo".to_owned(),
                loadable: true,
                label: "Solo".to_owned(),
                detail: "1 musician's mix".to_owned(),
            },
            SessionRow {
                id: "broken".to_owned(),
                name: "broken".to_owned(),
                loadable: false,
                label: "broken (cannot be loaded)".to_owned(),
                detail: "It cannot be loaded: the file is not valid JSON.".to_owned(),
            },
        ]
    );
    assert_eq!(mixes_text(0), "no mixes");
}

#[test]
fn the_panel_says_what_the_desk_said_about_its_session_and_when_nothing_could_be_listed() {
    let mut state = ConsoleState::new();
    state.reduce(&Action::SessionsFailed("access denied".to_owned()));
    state.reduce(&Action::SessionNotice(Some(
        "the session file x has a `groups` section that cannot be read.".to_owned(),
    )));
    let view = session_panel_view(&state, UTC);
    assert_eq!(
        view.notice.as_deref(),
        Some(
            "When the console opened its saved session: the session file x has a `groups` section that cannot be read."
        )
    );
    assert_eq!(view.unread.as_deref(), Some("access denied"));
    assert!(view.rows.is_empty());
    assert!(!view.engine_running);
    assert_eq!(
        sessions_unread_text("access denied"),
        "The saved sessions could not be listed: access denied."
    );
    assert_eq!(
        sessions_unread_text(""),
        "Listing the saved sessions\u{2026}"
    );

    state.reduce(&Action::Sessions(vec![show()]));
    state.reduce(&Action::SessionNotice(None));
    let listed = session_panel_view(&state, UTC);
    assert_eq!(listed.notice, None);
    assert_eq!(listed.unread, None);
    assert_eq!(listed.rows.len(), 1);
    assert_eq!(
        session_panel_view(&ConsoleState::new(), UTC)
            .unread
            .as_deref(),
        Some(""),
        "listing"
    );
}

#[test]
fn a_save_says_whether_the_musicians_mixes_were_kept() {
    let answer = |mixes, engine_running| SessionSavedDto {
        session: saved("s1", "Show", Some(AT), mixes),
        engine_running,
    };
    assert_eq!(
        format_session_saved(&answer(3, true)),
        "Saved the session as \"Show\", with 3 musicians' mixes."
    );
    assert_eq!(
        format_session_saved(&answer(0, true)),
        "Saved the session as \"Show\". No musician was listed, so no mix was kept."
    );
    assert_eq!(
        format_session_saved(&answer(0, false)),
        "Saved the session as \"Show\" without mixes: the engine is not running."
    );
}

pub(super) fn mix_dto() -> MixDto {
    MixDto {
        revision: 2,
        volume_percent: 100,
        max_level_percent: 100,
        muted: false,
        channel_gains: Vec::new(),
        pans: Vec::new(),
        channel_muted: Vec::new(),
        channel_solo: Vec::new(),
        group_levels: Vec::new(),
        engineer_limit_percent: 100,
        locked_channels: Vec::new(),
    }
}

pub(super) fn loaded() -> SessionLoadedDto {
    SessionLoadedDto {
        session: show(),
        applied: true,
        saved: true,
        save_error: None,
        mixes: Vec::new(),
        skipped: Vec::new(),
        refused: Vec::new(),
        damaged: Vec::new(),
        audio: None,
        audio_loaded: false,
        audio_skipped: None,
        presets: Vec::new(),
        local_loaded: false,
        local_mix: None,
        local_output_notice: None,
    }
}

fn pushed(address: &str, follows: Option<&str>) -> LoadedMixDto {
    LoadedMixDto {
        address: address.to_owned(),
        follows: follows.map(str::to_owned),
        mix: mix_dto(),
    }
}

fn skipped(
    key: &str,
    label: Option<&str>,
    reason: &'static str,
    error: Option<&str>,
) -> SkippedMixDto {
    SkippedMixDto {
        key: key.to_owned(),
        label: label.map(str::to_owned),
        reason,
        error: error.map(str::to_owned),
    }
}

fn named(address: &str, name: &str) -> Musician {
    Musician {
        address: address.to_owned(),
        name: Some(name.to_owned()),
        ..Musician::default()
    }
}

#[test]
fn a_load_says_what_applies_what_was_pushed_to_whom_and_what_went_to_nobody_and_why() {
    let musicians = [named("a", "Ana"), named("b", "Rack"), named("c", "Bass")];
    let outcome = SessionLoadedDto {
        mixes: vec![
            pushed("a", None),
            pushed("b", None),
            pushed("c", Some("Ana")),
        ],
        skipped: vec![
            skipped("client:x", Some("Keys"), "notConnected", None),
            skipped("client:y", Some("Drums"), "notHere", None),
            skipped("client:z", None, "notHere", None),
            skipped(
                "address:q",
                Some("Pad"),
                "refused",
                Some("the engine is stopping"),
            ),
        ],
        refused: vec![
            "Rack following Rack's mix was not applied: a mirror cannot go round in a circle"
                .to_owned(),
        ],
        damaged: vec!["its `presets` section cannot be read (expected a list)".to_owned()],
        audio_loaded: true,
        ..loaded()
    };
    assert_eq!(
        format_session_loaded(&outcome, &musicians),
        "Loaded \"Show\": its channels, groups, presets, labels, limits, locks and mirrors apply now. \
         Pushed the saved mixes to Ana and Rack. \
         Bass follows Ana's mix, so only their master volume and mute were set. \
         Not pushed to Keys: not connected, and a phone's own mix wins when it connects. \
         Not pushed to Drums and a phone: not here. \
         Not pushed to Pad: the engine is stopping. \
         Not applied: Rack following Rack's mix was not applied: a mirror cannot go round in a circle. \
         Part of the saved session could not be read and was left out: its `presets` section cannot be read (expected a list). \
         Its audio input is used from the next start."
    );
}

#[test]
fn a_load_with_no_engine_applies_at_the_next_start_and_pushes_no_mix() {
    let outcome = SessionLoadedDto {
        applied: false,
        skipped: vec![
            skipped("client:x", None, "notRunning", None),
            skipped("client:y", None, "notRunning", None),
        ],
        audio_skipped: Some(
            "ASIO is kept only with the ASIO driver the engineer picked, and none is named"
                .to_owned(),
        ),
        ..loaded()
    };
    assert_eq!(
        format_session_loaded(&outcome, &[]),
        "Loaded \"Show\": it applies at the next start. \
         The 2 saved mixes were not pushed: the engine is not running. \
         Its audio input was not taken: ASIO is kept only with the ASIO driver the engineer picked, \
         and none is named. The desk keeps its own."
    );
    let one = SessionLoadedDto {
        applied: false,
        skipped: vec![skipped("client:x", None, "notRunning", None)],
        ..loaded()
    };
    assert!(
        format_session_loaded(&one, &[])
            .ends_with("The saved mix was not pushed: the engine is not running.")
    );
    let unsaved = SessionLoadedDto {
        saved: false,
        save_error: Some("the disk is full.".to_owned()),
        ..loaded()
    };
    assert!(
        format_session_loaded(&unsaved, &[])
            .ends_with("It runs, but was not saved: the disk is full.")
    );
}

#[test]
fn a_loaded_session_says_it_took_the_saved_local_mix_and_why_its_output_was_not_opened() {
    let notice = "The saved local mix output, Volt 4 \u{b7} outputs 1/2, was not opened: the running audio driver does not offer it. The local mix plays nowhere until you choose an output.";
    let outcome = SessionLoadedDto {
        local_loaded: true,
        local_mix: Some(mix_dto()),
        local_output_notice: Some(notice.to_owned()),
        ..loaded()
    };
    let said = format_session_loaded(&outcome, &[]);
    assert!(said.contains("The local mix is the saved one."), "{said}");
    assert!(said.ends_with(notice), "{said}");
    assert!(!format_session_loaded(&loaded(), &[]).contains("local mix"));
}

#[test]
fn a_skipped_mix_names_its_musician_by_label_by_the_phone_listed_or_by_address() {
    let musicians = [Musician {
        address: "192.168.1.3:50000".to_owned(),
        client_id: Some("lead".to_owned()),
        client_name: Some("Lead vocal".to_owned()),
        name: Some("Lead vocal".to_owned()),
        ..Musician::default()
    }];
    let name = |key: &str, label: Option<&str>| {
        skipped_name(&skipped(key, label, "notHere", None), &musicians)
    };
    assert_eq!(name("client:lead", Some("Ana")), "Ana");
    assert_eq!(name("client:lead", None), "Lead vocal");
    assert_eq!(
        name("address:192.168.1.40:50000", None),
        "192.168.1.40:50000"
    );
    assert_eq!(name("client:gone", None), "a phone");
    assert_eq!(join_names(&[]), "");
    assert_eq!(join_names(&["A".to_owned()]), "A");
    assert_eq!(
        join_names(&["A".to_owned(), "B".to_owned(), "C".to_owned()]),
        "A, B and C"
    );
}

fn rows() -> Vec<SessionRow> {
    session_rows(
        &[show(), saved("s2", "Rehearsal", Some(AT), 3), broken()],
        UTC,
    )
}

#[test]
fn a_load_goes_at_once_or_asks_first_while_edits_are_still_on_their_way() {
    let mut panel = SessionPanel::default();
    panel.set_rows(rows());
    assert_eq!(
        panel.chosen().map(|row| row.id.as_str()),
        Some("s1"),
        "the newest is chosen to begin with"
    );

    assert_eq!(panel.load(0).as_deref(), Some("s1"));

    panel.choose("s2");
    assert_eq!(panel.load(3), None, "nothing is loaded while it asks");
    assert_eq!(
        panel.asking(),
        Some(Asking {
            id: "s2".to_owned(),
            unsaved: 3,
            name: "Rehearsal".to_owned()
        })
    );
    assert_eq!(
        load_ask_text(3, "Rehearsal"),
        "3 changes are still on the way to the engine. Loading \"Rehearsal\" drops them."
    );
    assert_eq!(
        load_ask_text(1, "Show"),
        "1 change is still on the way to the engine. Loading \"Show\" drops them."
    );
    panel.cancel_load();
    assert_eq!(panel.asking(), None);

    panel.load(1);
    assert_eq!(panel.confirm_load().as_deref(), Some("s2"));
    assert_eq!(panel.asking(), None);

    panel.load(2);
    panel.choose("s1");
    assert_eq!(
        panel.asking(),
        None,
        "choosing another session drops the question"
    );
    panel.load(2);
    panel.answered(Kind::Load, false);
    assert_eq!(panel.asking(), None);
}

#[test]
fn a_session_that_cannot_be_loaded_is_neither_loaded_nor_renamed_and_the_choice_stays_listed() {
    let mut panel = SessionPanel::default();
    panel.set_rows(rows());
    panel.choose("broken");

    assert_eq!(panel.load(0), None);
    assert_eq!(panel.rename("Better"), None);
    assert!(!panel.load_ready());

    panel.set_rows(
        rows()
            .into_iter()
            .filter(|row| row.id != "broken")
            .collect(),
    );
    assert_eq!(
        panel.chosen().map(|row| row.id.as_str()),
        Some("s1"),
        "a session no longer listed gives way to the first"
    );
    panel.choose("nowhere");
    assert_eq!(panel.chosen().map(|row| row.id.as_str()), Some("s1"));
    assert_eq!(
        panel.rename("  Late show "),
        Some(("s1".to_owned(), "Late show".to_owned()))
    );
    assert_eq!(panel.save("  "), None);
    assert_eq!(panel.save(" Encore ").as_deref(), Some("Encore"));
    assert_eq!(
        panel.answered(Kind::Save, true),
        Clear {
            save: true,
            rename: false
        }
    );
    assert_eq!(panel.answered(Kind::Rename, false), Clear::default());
    assert_eq!(
        panel.answered(Kind::Rename, true),
        Clear {
            save: false,
            rename: true
        }
    );
}

#[test]
fn a_delete_is_disarmed_by_another_choice_and_by_its_session_leaving_the_list() {
    let mut panel = SessionPanel::default();
    panel.set_rows(rows());
    let generation = panel.delete.arm("s1".to_owned());
    panel.delete.ready(generation);
    assert_eq!(panel.delete_state(), (true, true));
    panel.choose("s2");
    assert_eq!(panel.delete_state(), (false, false), "on another choice");

    panel.delete.arm("s2".to_owned());
    panel.set_rows(rows().into_iter().filter(|row| row.id != "s2").collect());
    assert_eq!(panel.delete.armed(), None, "a session no longer listed");
}

fn backup(
    stamp: &str,
    saved_at: Option<u64>,
    musicians: usize,
    names: &[&str],
) -> MixBackupSummary {
    MixBackupSummary {
        stamp: stamp.to_owned(),
        saved_at,
        musicians,
        names: names.iter().map(|name| (*name).to_owned()).collect(),
        problem: None,
    }
}

#[test]
fn a_backups_time_is_shown_to_the_second_and_the_rows_say_how_many_and_whose_mixes() {
    let rows = mix_backup_rows(
        &[
            backup(
                "20261018T190405000Z",
                Some(BACKED_UP),
                3,
                &["Ana", "Bass", "Rack"],
            ),
            backup("20261018T190300000Z", Some(BACKED_UP), 1, &["Ana"]),
            MixBackupSummary {
                problem: Some("it is damaged and cannot be read".to_owned()),
                ..backup("20261018T190200000Z", None, 0, &[])
            },
        ],
        UTC,
    );
    assert_eq!(
        rows,
        [
            BackupRow {
                stamp: "20261018T190405000Z".to_owned(),
                restorable: true,
                label: "18 Oct 2026, 19:04:05".to_owned(),
                detail: "3 musicians: Ana, Bass and Rack".to_owned(),
            },
            BackupRow {
                stamp: "20261018T190300000Z".to_owned(),
                restorable: true,
                label: "18 Oct 2026, 19:04:05".to_owned(),
                detail: "1 musician: Ana".to_owned(),
            },
            BackupRow {
                stamp: "20261018T190200000Z".to_owned(),
                restorable: false,
                label: "Backup 20261018T190200000Z".to_owned(),
                detail: "It cannot be restored: it is damaged and cannot be read.".to_owned(),
            },
        ]
    );
    assert_eq!(
        mix_backup_rows(&[backup("x", Some(BACKED_UP), 0, &[])], UTC)[0].detail,
        "no musicians"
    );
    assert_eq!(
        mix_backup_rows(&[backup("x", Some(BACKED_UP), 2, &[])], UTC)[0].detail,
        "2 musicians",
        "without names"
    );
    assert_eq!(
        restore_ask_text(&rows[0]),
        "Restore the mixes backed up at 18 Oct 2026, 19:04:05 (3 musicians: Ana, Bass and Rack)? Each connected musician's phone is sent their backed-up mix now."
    );
}

#[test]
fn the_backups_view_says_whether_they_are_listed_yet_and_whether_a_restore_can_run() {
    let unread = mix_backups_view(&ConsoleState::new(), UTC);
    assert_eq!(
        unread,
        BackupsView {
            rows: Vec::new(),
            unread: Some(String::new()),
            engine_running: false
        }
    );
    assert_eq!(backups_unread_text(""), "Listing the mix backups\u{2026}");
    let mut state = ConsoleState::new();
    state.mix_backups = Listing {
        list: None,
        error: Some("denied".to_owned()),
    };
    assert_eq!(
        mix_backups_view(&state, UTC).unread.as_deref(),
        Some("denied")
    );
    assert_eq!(
        backups_unread_text("denied"),
        "The mix backups could not be listed: denied."
    );

    state.reduce(&Action::Status(Box::new(sample_status())));
    state.reduce(&Action::MixBackups(vec![backup(
        "20261018T190405000Z",
        Some(1000),
        2,
        &[],
    )]));
    let listed = mix_backups_view(&state, UTC);
    assert_eq!(listed.unread, None);
    assert_eq!(listed.rows.len(), 1);
    assert!(listed.engine_running);
}

#[test]
fn a_restore_is_said_in_words_who_got_their_mix_back_and_who_was_skipped_and_why() {
    let musicians = [
        named("192.168.1.3:50000", "Ana"),
        named("192.168.1.12:50000", "Bass"),
    ];
    let outcome = MixRestoredDto {
        backup: backup("20261018T190405000Z", Some(BACKED_UP), 2, &[]),
        restored: vec![
            pushed("192.168.1.3:50000", None),
            pushed("192.168.1.12:50000", Some("Ana")),
        ],
        skipped: vec![
            skipped("client:away", Some("Keys"), "notConnected", None),
            skipped("client:gone", Some("Drums"), "notHere", None),
            skipped("address:10.0.0.9:50000", None, "refused", Some("stopping.")),
        ],
    };
    assert_eq!(
        format_mix_restored(&outcome, &musicians, UTC),
        "Restored the mixes backed up at 18 Oct 2026, 19:04:05 to Ana. \
         Bass follows Ana's mix, so only their master volume and mute were set. \
         Not restored to Keys: not connected, and a phone's own mix wins when it connects. \
         Not restored to Drums: not here. \
         Not restored to 10.0.0.9:50000: stopping."
    );
    let nothing = MixRestoredDto {
        backup: backup("x", None, 0, &[]),
        restored: Vec::new(),
        skipped: Vec::new(),
    };
    assert_eq!(
        format_mix_restored(&nothing, &[], UTC),
        "No musician's mix was restored from the backup."
    );
    let unknown_time = MixRestoredDto {
        backup: backup("x", None, 1, &[]),
        restored: vec![pushed("192.168.1.3:50000", None)],
        skipped: Vec::new(),
    };
    assert_eq!(
        format_mix_restored(&unknown_time, &musicians, UTC),
        "Restored the mixes backed up to Ana."
    );
}

#[test]
fn restore_asks_first_and_only_the_confirming_restore_restores_and_cancel_or_a_new_list_drops_it() {
    let mut panel = BackupsPanel::default();
    panel.set_rows(mix_backup_rows(
        &[
            backup("20261018T190405000Z", Some(1000), 2, &["Ana", "Rack"]),
            MixBackupSummary {
                problem: Some("damaged".to_owned()),
                ..backup("20261018T190200000Z", Some(1000), 2, &[])
            },
        ],
        UTC,
    ));

    panel.ask("20261018T190200000Z");
    assert_eq!(
        panel.asking(),
        None,
        "a backup that cannot be restored is not asked about"
    );
    panel.ask("20261018T190405000Z");
    assert_eq!(
        panel.asking().map(|row| row.stamp.as_str()),
        Some("20261018T190405000Z")
    );
    panel.cancel();
    assert_eq!(panel.asking(), None);

    panel.ask("20261018T190405000Z");
    assert_eq!(panel.confirm().as_deref(), Some("20261018T190405000Z"));
    assert_eq!(panel.asking(), None);

    panel.ask("20261018T190405000Z");
    panel.set_rows(mix_backup_rows(
        &[backup("20261018T190500000Z", Some(1000), 1, &[])],
        UTC,
    ));
    assert_eq!(
        panel.asking(),
        None,
        "a backup no longer listed is no longer asked about"
    );
}
