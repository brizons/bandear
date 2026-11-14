//! The model's Session panel flows over the console's simulated desk (`session-console.test.js`,
//! the console half of `mix-backups.test.js`, and the panel commands of `sessions.test.js`).

use std::cell::{Cell, RefCell};

use bandear_desktop_console::command::{Answer, Command, Owner};
use bandear_desktop_console::console::Options;
use bandear_desktop_console::engine_config::Host;
use bandear_desktop_console::exec::Slot;
use bandear_desktop_console::mix::{EditKey, EditValue, MixField};
use bandear_desktop_console::router::Section;
use bandear_desktop_console::testing::{FakeDesk, Harness};
use bandear_desktop_console::types::dto::{
    LoadedMixDto, MixDto, MixRestoredDto, SessionLoadedDto, SessionSavedDto, SkippedMixDto,
};
use bandear_desktop_console::types::{MixBackupSummary, SessionAudio, SnapshotSummary};

use super::*;

const LEAD: &str = "192.168.1.3:50000";
const BASS: &str = "192.168.1.12:50000";
/// 2026-10-18 19:04:05 UTC.
const BACKED_UP: u64 = 1_792_350_245_000;

fn saved(id: &str, name: &str) -> SnapshotSummary {
    SnapshotSummary {
        id: id.to_owned(),
        name: name.to_owned(),
        saved_at: Some(500),
        mixes: 1,
        problem: None,
    }
}

fn broken() -> SnapshotSummary {
    SnapshotSummary {
        problem: Some("the file is not valid JSON".to_owned()),
        saved_at: None,
        mixes: 0,
        ..saved("broken", "broken")
    }
}

fn mix_dto() -> MixDto {
    MixDto {
        revision: 9,
        volume_percent: 33,
        max_level_percent: 90,
        muted: false,
        channel_gains: vec![11],
        pans: Vec::new(),
        channel_muted: Vec::new(),
        channel_solo: Vec::new(),
        group_levels: Vec::new(),
        engineer_limit_percent: 100,
        locked_channels: Vec::new(),
    }
}

fn loaded(session: SnapshotSummary) -> SessionLoadedDto {
    SessionLoadedDto {
        session,
        applied: true,
        saved: true,
        save_error: None,
        mixes: vec![LoadedMixDto {
            address: LEAD.to_owned(),
            follows: None,
            mix: mix_dto(),
        }],
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

/// A desk that keeps named sessions as the desktop does: `load` answers a load of a listed one.
fn with_sessions(
    mut desk: FakeDesk,
    list: Vec<SnapshotSummary>,
    load: impl Fn(&FakeDesk, SnapshotSummary) -> SessionLoadedDto + 'static,
) -> FakeDesk {
    let sessions = Rc::new(RefCell::new(list));
    let listed = Rc::clone(&sessions);
    desk.script("sessions", move |_, _| {
        Ok(Answer::Sessions(listed.borrow().clone()))
    });
    let kept = Rc::clone(&sessions);
    desk.script("save_session", move |desk, command| {
        let Command::SaveSession { name } = command else {
            return Err("not a save".to_owned());
        };
        if kept
            .borrow()
            .iter()
            .any(|saved| saved.name.to_lowercase() == name.to_lowercase())
        {
            return Err(format!("a session named \"{name}\" is already saved"));
        }
        let session = SnapshotSummary {
            mixes: if desk.running {
                desk.report.musicians.len()
            } else {
                0
            },
            ..saved(&format!("s{}", kept.borrow().len() + 1), name)
        };
        kept.borrow_mut().push(session.clone());
        Ok(Answer::SessionSaved(SessionSavedDto {
            session,
            engine_running: desk.running,
        }))
    });
    let named = Rc::clone(&sessions);
    desk.script("rename_session", move |_, command| {
        let Command::RenameSession { id, name } = command else {
            return Err("not a rename".to_owned());
        };
        let mut list = named.borrow_mut();
        let saved = list
            .iter_mut()
            .find(|saved| &saved.id == id)
            .ok_or_else(|| format!("no saved session `{id}`"))?;
        if saved.problem.is_some() {
            return Err(format!("the saved session `{id}` cannot be read"));
        }
        saved.name = name.clone();
        Ok(Answer::SessionRenamed(saved.clone()))
    });
    let gone = Rc::clone(&sessions);
    desk.script("delete_session", move |_, command| {
        let Command::DeleteSession { id } = command else {
            return Err("not a delete".to_owned());
        };
        gone.borrow_mut().retain(|saved| &saved.id != id);
        Ok(Answer::Done)
    });
    let opened = Rc::clone(&sessions);
    desk.script("load_session", move |desk, command| {
        let Command::LoadSession { id } = command else {
            return Err("not a load".to_owned());
        };
        let session = opened
            .borrow()
            .iter()
            .find(|saved| &saved.id == id)
            .cloned()
            .ok_or_else(|| format!("no saved session `{id}`"))?;
        if session.problem.is_some() {
            return Err(format!("the saved session `{id}` cannot be loaded"));
        }
        Ok(Answer::SessionLoaded(load(desk, session)))
    });
    desk
}

fn sunday() -> FakeDesk {
    with_sessions(
        FakeDesk::running(),
        vec![saved("s9", "Sunday"), broken()],
        |_, session| loaded(session),
    )
}

fn network(desk: FakeDesk, options: Options) -> (Harness, Model) {
    let harness = Harness::connected(desk, options);
    let model = Model::new(harness.console.for_owner(Owner::Screen(Section::Network)));
    model.set_zone(Zone::Fixed(0));
    model.screen();
    (harness, model)
}

fn said(model: &Model) -> Option<Said> {
    model.screen().session.said
}

fn edit(harness: &Harness, address: &str, channel: usize, level: u8) {
    let key = EditKey::new(MixField::ChannelGains, Some(channel)).expect("a key");
    harness.console.dispatch(Action::MixEdit {
        address: address.to_owned(),
        edits: [(key, EditValue::Level(level))].into_iter().collect(),
    });
}

#[test]
fn the_console_reads_the_saved_sessions_and_what_the_desk_said_about_its_session_when_it_connects()
{
    let mut desk = sunday();
    desk.script("session_notice", |_, _| {
        Ok(Answer::SessionNotice(Some(
            "imported 2 labels from musicians.json".to_owned(),
        )))
    });
    let (_, model) = network(desk, Options::default());
    let view = model.screen().session;
    assert_eq!(
        view.panel.notice.as_deref(),
        Some("When the console opened its saved session: imported 2 labels from musicians.json.")
    );
    let names: Vec<_> = view
        .panel
        .rows
        .iter()
        .map(|row| row.name.as_str())
        .collect();
    assert_eq!(names, ["Sunday", "broken"]);
    assert_eq!(view.chosen.map(|row| row.id), Some("s9".to_owned()));
}

#[test]
fn a_save_keeps_the_desk_session_says_whether_the_mixes_were_kept_and_lists_the_sessions_again() {
    let (mut harness, model) = network(sunday(), Options::default());
    let listings = harness.calls_to("sessions").len();
    model.save_session("  Show ");
    harness.settle();
    assert!(matches!(
        harness.calls_to("save_session").last(),
        Some(Command::SaveSession { name }) if name == "Show"
    ));
    assert_eq!(
        said(&model),
        Some(Said::new(
            true,
            "Saved the session as \"Show\", with 4 musicians' mixes."
        ))
    );
    assert_eq!(
        harness.calls_to("sessions").len(),
        listings + 1,
        "listed again"
    );
    let view = model.screen();
    assert!(view.session.panel.rows.iter().any(|row| row.name == "Show"));
    assert_eq!(view.cleared.save_name, 1, "an ok save empties its field");

    model.save_session("SHOW");
    harness.settle();
    assert_eq!(
        said(&model),
        Some(Said::new(
            false,
            "The session was not saved: a session named \"SHOW\" is already saved."
        ))
    );
    assert_eq!(
        model.screen().cleared.save_name,
        1,
        "a refused save keeps it"
    );
    let saves = harness.calls_to("save_session").len();
    model.save_session("   ");
    harness.settle();
    assert_eq!(
        harness.calls_to("save_session").len(),
        saves,
        "a blank name does nothing"
    );
}

#[test]
fn a_load_goes_at_once_or_asks_first_while_edits_are_still_on_their_way() {
    let (mut harness, model) = network(sunday(), Options::default());
    model.press_load();
    harness.settle();
    assert_eq!(
        harness.calls_to("load_session").len(),
        1,
        "nothing on its way: at once"
    );
    assert_eq!(
        said(&model),
        Some(Said::new(
            true,
            "Loaded \"Sunday\": its channels, groups, presets, labels, limits, locks and mirrors apply now. Pushed the saved mix to Lead vocal."
        ))
    );

    edit(&harness, BASS, 0, 70);
    model.press_load();
    harness.settle();
    assert_eq!(harness.calls_to("load_session").len(), 1, "it asks first");
    let asking = model.screen().session.asking.expect("asking");
    assert_eq!((asking.unsaved, asking.name.as_str()), (1, "Sunday"));
    model.cancel_load();
    assert_eq!(model.screen().session.asking, None);
    harness.settle();
    assert_eq!(
        harness.calls_to("load_session").len(),
        1,
        "Cancel loads nothing"
    );

    model.press_load();
    model.confirm_load();
    harness.settle();
    assert_eq!(harness.calls_to("load_session").len(), 2, "Load anyway");
    assert_eq!(model.screen().session.asking, None);
}

#[test]
fn a_load_holds_the_sends_and_drops_the_edits_still_waiting_once_it_loaded() {
    let (mut harness, model) = network(sunday(), Options::default());
    let released = Rc::new(Cell::new(0));
    let heard = Rc::clone(&released);
    harness
        .console
        .on_sends_released(move |_| heard.set(heard.get() + 1));
    harness.hold("load_session");
    edit(&harness, BASS, 0, 70);
    edit(&harness, BASS, 1, 60);
    assert_eq!(harness.console.unsaved_edits(), 2);

    model.load_session("s9".to_owned());
    harness.settle();
    assert!(harness.console.holding(), "nothing goes out while it loads");
    assert_eq!(
        harness.console.unsaved_edits(),
        2,
        "kept while the desk has not answered"
    );
    harness.release();

    assert!(!harness.console.holding());
    assert_eq!(released.get(), 1, "the lanes are told to send what is left");
    assert_eq!(
        harness.console.unsaved_edits(),
        0,
        "none was meant for the loaded session"
    );
    assert!(harness.console.state().pending.is_empty());
    let lead = harness
        .console
        .state()
        .musician_by_address(LEAD)
        .cloned()
        .expect("lead");
    assert_eq!(
        lead.mix.channel_gains[0], 62,
        "the status read after the load is the engine's"
    );
    assert!(
        !harness.calls_to("processing").is_empty(),
        "the strips are read again"
    );
}

#[test]
fn a_refused_load_keeps_the_edits_still_waiting_and_they_go_out() {
    let mut desk = sunday();
    desk.fail_next("load_session", "the disk is gone", 1);
    let (mut harness, model) = network(desk, Options::default());
    let released = Rc::new(Cell::new(0));
    let heard = Rc::clone(&released);
    harness
        .console
        .on_sends_released(move |_| heard.set(heard.get() + 1));
    edit(&harness, BASS, 0, 70);
    edit(&harness, BASS, 1, 60);

    model.load_session("s9".to_owned());
    harness.settle();

    assert_eq!(
        said(&model),
        Some(Said::new(
            false,
            "\"Sunday\" was not loaded: the disk is gone."
        ))
    );
    assert_eq!(
        harness.console.unsaved_edits(),
        2,
        "the engineer's edits are not lost for nothing"
    );
    assert_eq!(released.get(), 1, "and the lanes send them");
    assert!(!harness.console.holding());
}

#[test]
fn a_channel_or_group_edit_on_its_way_counts_and_the_load_waits_for_it() {
    let (mut harness, model) = network(sunday(), Options::default());
    let gate = Slot::new();
    let held = gate.clone();
    let editing = harness
        .console
        .in_turn(true, async move { held.wait().await });
    harness.console.spawn(editing);
    assert_eq!(harness.console.unsaved_edits(), 1);

    model.load_session("s9".to_owned());
    harness.settle();
    assert!(
        harness.calls_to("load_session").is_empty(),
        "the load waits for the edit's answer"
    );
    gate.fill(());
    harness.console.drive();
    harness.settle();
    assert_eq!(harness.calls_to("load_session").len(), 1);
}

#[test]
fn a_loaded_audio_input_is_opened_by_the_next_start_and_nothing_restarts() {
    let asio = SessionAudio {
        host: "asio".to_owned(),
        device: None,
        buffer_frames: Some(128),
        asio_driver: Some("Demo ASIO driver".to_owned()),
    };
    let desk = with_sessions(
        FakeDesk::running(),
        vec![saved("bare", "Bare"), saved("picked", "Picked")],
        move |_, session| {
            if session.id == "picked" {
                SessionLoadedDto {
                    audio: Some(asio.clone()),
                    audio_loaded: true,
                    ..loaded(session)
                }
            } else {
                SessionLoadedDto {
                    audio_skipped: Some(
                        "ASIO is kept only with the ASIO driver the engineer picked, and none is named"
                            .to_owned(),
                    ),
                    ..loaded(session)
                }
            }
        },
    );
    let (mut harness, model) = network(desk, Options::default());
    let before = harness.console.state().audio.clone();
    let starts = harness.starts().len();

    model.load_session("bare".to_owned());
    harness.settle();
    let words = said(&model).expect("said").message;
    assert!(
        words.ends_with("Its audio input was not taken: ASIO is kept only with the ASIO driver the engineer picked, and none is named. The desk keeps its own."),
        "{words}"
    );
    assert_eq!(
        harness.console.state().audio,
        before,
        "the desk keeps its own"
    );

    model.load_session("picked".to_owned());
    harness.settle();
    assert!(
        said(&model)
            .expect("said")
            .message
            .ends_with("Its audio input is used from the next start.")
    );
    let audio = harness.console.state().audio.clone();
    assert_eq!(audio.host, Host::Asio);
    assert_eq!(audio.asio_driver.as_deref(), Some("Demo ASIO driver"));
    assert_eq!(harness.starts().len(), starts, "nothing restarted");

    let stop = harness.console.stop();
    harness.run(stop);
    let start = harness.console.start();
    harness.run(start);
    assert_eq!(
        harness.starts().last(),
        Some(&("asio".to_owned(), Some("Demo ASIO driver".to_owned()))),
        "the next start opens it"
    );
}

#[test]
fn with_the_engine_stopped_a_load_applies_at_the_next_start_and_pushes_no_mix() {
    let desk = with_sessions(
        FakeDesk::stopped(),
        vec![saved("s9", "Sunday")],
        |_, session| SessionLoadedDto {
            applied: false,
            mixes: Vec::new(),
            skipped: vec![SkippedMixDto {
                key: "client:lead".to_owned(),
                label: None,
                reason: "notRunning",
                error: None,
            }],
            ..loaded(session)
        },
    );
    let (mut harness, model) = network(
        desk,
        Options {
            auto_start: false,
            ..Options::default()
        },
    );
    model.load_session("s9".to_owned());
    harness.settle();
    assert_eq!(
        said(&model).map(|said| said.message),
        Some(
            "Loaded \"Sunday\": it applies at the next start. The saved mix was not pushed: the engine is not running."
                .to_owned()
        )
    );
}

#[test]
fn a_load_whose_strips_cannot_be_read_after_it_says_so() {
    let (mut harness, model) = network(sunday(), Options::default());
    harness.desk.fail_next("processing", "the desk is busy", 1);
    model.load_session("s9".to_owned());
    harness.settle();
    assert!(
        said(&model)
            .expect("said")
            .message
            .ends_with("Its channel processing could not be read: the desk is busy.")
    );
    let state = harness.console.state();
    assert_eq!(state.processing, None, "no longer known");
    assert_eq!(
        state
            .processing_error
            .as_ref()
            .map(|error| error.message.as_str()),
        Some("The loaded session's channel processing could not be read: the desk is busy.")
    );
}

#[test]
fn saved_sessions_are_renamed_and_deleted_and_one_that_cannot_be_read_is_neither_loaded_nor_renamed()
 {
    let (mut harness, model) = network(sunday(), Options::default());
    model.rename_session("  Late show ");
    harness.settle();
    assert_eq!(
        said(&model),
        Some(Said::new(
            true,
            "Renamed the saved session \"Sunday\" to \"Late show\"."
        ))
    );
    assert_eq!(model.screen().cleared.rename, 1);

    model.choose_session("broken");
    let view = model.screen().session;
    assert!(!view.load_ready);
    assert_eq!(
        view.chosen.map(|row| row.label),
        Some("broken (cannot be loaded)".to_owned())
    );
    model.press_load();
    model.rename_session("Fixed");
    harness.settle();
    assert!(harness.calls_to("load_session").is_empty());
    assert!(
        harness.calls_to("rename_session").len() == 1,
        "only the first rename went out"
    );

    model.arm_delete();
    assert_eq!(model.screen().session.delete, (true, false));
    model.confirm_delete();
    harness.settle();
    assert!(
        harness.calls_to("delete_session").is_empty(),
        "a double click keeps the session"
    );
    harness.fire_timers();
    assert_eq!(model.screen().session.delete, (true, true));
    model.confirm_delete();
    harness.settle();
    assert_eq!(
        said(&model),
        Some(Said::new(true, "Deleted the saved session \"broken\"."))
    );
    let names: Vec<_> = model
        .screen()
        .session
        .panel
        .rows
        .into_iter()
        .map(|row| row.name)
        .collect();
    assert_eq!(names, ["Late show"]);

    model.arm_delete();
    harness.settle();
    harness.fire_timers();
    harness.fire_timers();
    assert_eq!(
        model.screen().session.delete,
        (false, false),
        "it disarms itself"
    );
    model.arm_delete();
    model.keep_delete();
    assert_eq!(model.screen().session.delete, (false, false), "Keep");
}

#[test]
fn the_panel_is_answered_after_every_command_also_one_that_fails() {
    let mut desk = sunday();
    desk.fail_next("load_session", "the bridge is gone", 1);
    let (mut harness, model) = network(desk, Options::default());
    edit(&harness, BASS, 0, 70);
    model.press_load();
    model.confirm_load();
    harness.settle();
    assert_eq!(
        model.screen().session.asking,
        None,
        "an answered load is never asked again"
    );
    let before = said(&model);
    model.list_sessions();
    harness.settle();
    assert_eq!(said(&model), before, "listing again says nothing");
}

fn backup(stamp: &str, problem: Option<&str>) -> MixBackupSummary {
    MixBackupSummary {
        stamp: stamp.to_owned(),
        saved_at: problem.is_none().then_some(BACKED_UP),
        musicians: 1,
        names: vec!["Lead vocal".to_owned()],
        problem: problem.map(str::to_owned),
    }
}

/// A desk with two backups, one damaged, that restores the good one onto Lead vocal.
fn with_backups(mut desk: FakeDesk) -> FakeDesk {
    desk.script("list_mix_backups", |_, _| {
        Ok(Answer::MixBackups(vec![
            backup("20261018T190405000Z", None),
            backup("20261018T190300000Z", Some("it is damaged")),
        ]))
    });
    desk.script("restore_mix_backup", |desk, command| {
        let Command::RestoreMixBackup { stamp } = command else {
            return Err("not a restore".to_owned());
        };
        if !desk.running {
            return Err("the engine is not running".to_owned());
        }
        if stamp != "20261018T190405000Z" {
            return Err(format!("`{stamp}` is not a mix backup"));
        }
        Ok(Answer::MixRestored(MixRestoredDto {
            backup: backup(stamp, None),
            restored: vec![LoadedMixDto {
                address: LEAD.to_owned(),
                follows: None,
                mix: mix_dto(),
            }],
            skipped: Vec::new(),
        }))
    });
    desk
}

#[test]
fn the_backups_are_listed_once_connected_and_refresh_says_only_a_failure() {
    let (mut harness, model) = network(with_backups(FakeDesk::running()), Options::default());
    assert_eq!(model.screen().session.backups.rows.len(), 2);
    model.refresh_backups();
    harness.settle();
    assert_eq!(said(&model), None, "a list that worked says nothing");

    harness
        .desk
        .fail_next("list_mix_backups", "the folder is locked", 1);
    model.refresh_backups();
    harness.settle();
    assert_eq!(
        said(&model),
        Some(Said::new(
            false,
            "The mix backups could not be listed: the folder is locked."
        ))
    );
    let view = model.screen().session.backups;
    assert_eq!(view.rows.len(), 2, "the backups known stay");
    assert_eq!(view.unread, None);
}

#[test]
fn a_restore_asks_first_runs_in_turn_and_says_who_got_their_mix_back() {
    let (mut harness, model) = network(with_backups(FakeDesk::running()), Options::default());
    model.ask_restore("20261018T190300000Z");
    assert_eq!(
        model.screen().session.restore_asking,
        None,
        "a damaged backup is not asked about"
    );
    model.ask_restore("20261018T190405000Z");
    let asking = model.screen().session.restore_asking.expect("asking");
    assert_eq!(asking.label, "18 Oct 2026, 19:04:05");
    model.cancel_restore();
    assert_eq!(model.screen().session.restore_asking, None);
    harness.settle();
    assert!(harness.calls_to("restore_mix_backup").is_empty());

    let gate = Slot::new();
    let held = gate.clone();
    let editing = harness
        .console
        .in_turn(true, async move { held.wait().await });
    harness.console.spawn(editing);
    model.ask_restore("20261018T190405000Z");
    model.confirm_restore();
    harness.settle();
    assert!(
        harness.calls_to("restore_mix_backup").is_empty(),
        "in turn with the desk's edits"
    );
    let listings = harness.calls_to("list_mix_backups").len();
    gate.fill(());
    harness.console.drive();
    harness.settle();
    assert_eq!(harness.calls_to("restore_mix_backup").len(), 1);
    assert_eq!(
        harness.calls_to("list_mix_backups").len(),
        listings + 1,
        "listed again"
    );
    assert_eq!(
        said(&model),
        Some(Said::new(
            true,
            "Restored the mixes backed up at 18 Oct 2026, 19:04:05 to Lead vocal."
        ))
    );

    harness.desk.running = false;
    model.ask_restore("20261018T190405000Z");
    model.confirm_restore();
    harness.settle();
    assert_eq!(
        said(&model),
        Some(Said::new(
            false,
            "The mixes were not restored: the engine is not running."
        ))
    );
}
