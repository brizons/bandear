//! The model's flows over the console's simulated desk (`console-app.test.js`'s channel and group
//! tests, `channels-screen.test.js`).

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::rc::Rc;

use bandear_desktop_console::command::{Command, Owner};
use bandear_desktop_console::console::Options;
use bandear_desktop_console::router::Section;
use bandear_desktop_console::state::Action;
use bandear_desktop_console::testing::{FakeDesk, Harness};
use bandear_desktop_console::types::SnapshotSummary;
use bandear_desktop_console::types::dto::SessionLoadedDto;

use super::*;
use crate::testdesk::{Shared, channel_desk, show, use_device};

struct Screen {
    harness: Harness,
    kept: Shared,
    model: Model,
}

fn screen_over(mut desk: FakeDesk, options: Options) -> Screen {
    let kept = channel_desk(&mut desk);
    let harness = Harness::connected(desk, options);
    let model = Model::new(harness.console.for_owner(Owner::Screen(Section::Channels)));
    Screen {
        harness,
        kept,
        model,
    }
}

fn running() -> Screen {
    screen_over(FakeDesk::running(), Options::default())
}

/// Runs `flow` and hands back where its outcome lands once it ends (it may wait on held answers).
fn spawned<T: 'static>(
    harness: &mut Harness,
    flow: impl Future<Output = T> + 'static,
) -> Rc<RefCell<Option<T>>> {
    let outcome = Rc::new(RefCell::new(None));
    let kept = Rc::clone(&outcome);
    harness.run(async move {
        let value = flow.await;
        *kept.borrow_mut() = Some(value);
    });
    outcome
}

/// A `load_session` answer that loaded a session with nothing in it.
fn loaded() -> SessionLoadedDto {
    SessionLoadedDto {
        session: SnapshotSummary {
            id: "show".to_owned(),
            name: "Show".to_owned(),
            saved_at: None,
            mixes: 0,
            problem: None,
        },
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

fn ok(message: &str) -> Said {
    Said {
        ok: true,
        message: message.to_owned(),
    }
}

fn refused(message: &str) -> Said {
    Said {
        ok: false,
        message: message.to_owned(),
    }
}

impl Screen {
    fn run<T: 'static>(&mut self, flow: impl Future<Output = T> + 'static) -> T {
        self.harness.run(flow).expect("the flow ends")
    }

    fn status_names(&self) -> Vec<String> {
        let state = self.harness.console.state();
        let status = state.engine.status.as_ref().expect("a status");
        status
            .channels
            .iter()
            .map(|channel| channel.name.clone())
            .collect()
    }

    fn sent_groups(&self) -> Vec<(Option<String>, String, Vec<usize>)> {
        match self.harness.calls_to("set_groups").last() {
            Some(Command::SetGroups { groups }) => groups
                .iter()
                .map(|group| (group.id.clone(), group.name.clone(), group.channels.clone()))
                .collect(),
            _ => Vec::new(),
        }
    }
}

#[test]
fn channel_edits_go_out_one_at_a_time_each_built_on_what_the_last_one_left() {
    let mut screen = running();
    screen.harness.hold("set_channels");

    let first = spawned(
        &mut screen.harness,
        screen
            .model
            .edit_channels(|table, _| rename_channel(table, 0, "Kick in", 6)),
    );
    let second = spawned(
        &mut screen.harness,
        screen
            .model
            .edit_channels(|table, _| set_channel_kind(table, 1, "percussion", 6)),
    );
    assert_eq!(
        screen.harness.calls_to("set_channels").len(),
        1,
        "the second waits for the first's answer"
    );
    assert_eq!(
        screen.harness.console.unsaved_edits(),
        2,
        "both count as unsaved"
    );
    screen.harness.release();
    assert_eq!(screen.harness.calls_to("set_channels").len(), 2);
    screen.harness.release();

    let applied = ok("Channel details applied to every phone.");
    assert_eq!(first.borrow().clone(), Some(applied.clone()));
    assert_eq!(second.borrow().clone(), Some(applied));
    let state = screen.harness.console.state();
    let channels = &state.engine.status.as_ref().unwrap().channels;
    assert_eq!(channels[0].name, "Kick in");
    assert_eq!(
        channels[1].kind, "percussion",
        "the second edit kept the first one's name"
    );
    drop(state);
    assert_eq!(screen.harness.console.unsaved_edits(), 0);
    let reads = screen
        .harness
        .calls
        .iter()
        .rev()
        .position(|command| command.name() == "engine_status");
    assert_eq!(
        reads,
        Some(0),
        "each edit reads the status after its answer"
    );
}

#[test]
fn a_channel_edit_the_desk_refuses_says_why_and_sends_nothing() {
    let mut screen = running();
    let edit = screen
        .model
        .edit_channels(|table, _| rename_channel(table, 0, &"x".repeat(40), 6));
    assert_eq!(
        screen.run(edit),
        refused(
            "The channel details were not changed: the name of CH 1 is longer than 32 characters."
        )
    );
    assert!(screen.harness.calls_to("set_channels").is_empty());

    screen
        .harness
        .desk
        .fail_next("set_channels", "the engine is stopping", 1);
    let edit = screen
        .model
        .edit_channels(|table, _| rename_channel(table, 0, "Kick in", 6));
    assert_eq!(
        screen.run(edit),
        refused("The channel details were not changed: the engine is stopping.")
    );
}

#[test]
fn details_of_inputs_a_smaller_device_lacks_survive_an_edit_made_while_it_is_in_use() {
    let mut screen = running();
    let edit = screen
        .model
        .edit_channels(|table, _| rename_channel(table, 5, "Backing vox 2", 6));
    assert!(screen.run(edit).ok);

    use_device(&mut screen.harness.desk, &screen.kept, 2);
    let read = screen.harness.console.read_status();
    screen.run(read);
    let edit = screen
        .model
        .edit_channels(|table, count| rename_channel(table, 0, "Kick in", count));
    assert!(screen.run(edit).ok);
    use_device(&mut screen.harness.desk, &screen.kept, 6);
    let read = screen.harness.console.read_status();
    screen.run(read);

    assert_eq!(
        screen.status_names(),
        [
            "Kick in",
            "Snare",
            "Overheads",
            "Bass",
            "Lead vox",
            "Backing vox 2"
        ]
    );
}

#[test]
fn a_windows_first_channel_edit_on_a_smaller_device_keeps_the_details_of_the_inputs_it_lacks() {
    let mut screen = running();
    use_device(&mut screen.harness.desk, &screen.kept, 2);
    let read = screen.harness.console.read_status();
    screen.run(read);
    assert_eq!(
        screen.status_names().len(),
        2,
        "the status lists two inputs"
    );

    let edit = screen
        .model
        .edit_channels(|table, count| rename_channel(table, 0, "Kick in", count));
    let result = screen.run(edit);
    assert!(result.ok, "{}", result.message);
    assert_eq!(
        screen.harness.calls_to("channel_table").len(),
        1,
        "the edit is built on the whole table"
    );
    match screen.harness.calls_to("set_channels").last() {
        Some(Command::SetChannels { channels }) => assert_eq!(channels.len(), 6),
        other => panic!("no set_channels: {other:?}"),
    }
    use_device(&mut screen.harness.desk, &screen.kept, 6);
    let read = screen.harness.console.read_status();
    screen.run(read);
    assert_eq!(
        screen.status_names(),
        [
            "Kick in",
            "Snare",
            "Overheads",
            "Bass",
            "Lead vox",
            "Backing vox"
        ]
    );
}

#[test]
fn a_channel_edit_says_why_when_the_whole_table_cannot_be_read_and_sends_nothing() {
    let mut screen = running();
    screen
        .harness
        .desk
        .fail_next("channel_table", "engine state lock was poisoned", 1);
    let edit = screen
        .model
        .edit_channels(|table, _| rename_channel(table, 0, "Kick in", 6));
    assert_eq!(
        screen.run(edit),
        refused("The channel details were not changed: engine state lock was poisoned.")
    );
    assert!(screen.harness.calls_to("set_channels").is_empty());
}

#[test]
fn group_edits_answer_in_words_and_the_new_groups_show_at_once() {
    let mut screen = running();
    let edit = screen.model.edit_groups(add_group);
    assert_eq!(screen.run(edit), ok("Groups applied to every phone."));
    let names: Vec<String> = screen
        .harness
        .console
        .state()
        .engine
        .status
        .as_ref()
        .unwrap()
        .groups
        .iter()
        .map(|group| group.name.clone())
        .collect();
    assert_eq!(names, ["Drums", "Vocals", "Group 3"]);
    let edit = screen
        .model
        .edit_groups(|groups| rename_group(groups, "vocals", " "));
    assert_eq!(
        screen.run(edit),
        refused("The groups were not changed: a group needs a name.")
    );
}

#[test]
fn with_no_engine_running_a_group_edit_is_kept_for_the_next_start_once_the_desk_knows_the_list() {
    let mut screen = screen_over(
        FakeDesk::stopped(),
        Options {
            auto_start: false,
            ..Options::default()
        },
    );
    let edit = screen.model.edit_groups(add_group);
    assert_eq!(
        screen.run(edit),
        refused(
            "The groups were not changed: no engine is running, so this desk does not know them \
             yet; start the engine first."
        )
    );
    assert!(
        screen.harness.calls_to("set_groups").is_empty(),
        "the kept groups are not replaced blindly"
    );

    let start = screen.harness.console.start();
    assert_eq!(screen.run(start), Ok(()));
    let edit = screen
        .model
        .edit_groups(|groups| rename_group(groups, "vocals", "Voices"));
    assert!(screen.run(edit).ok);
    let stop = screen.harness.console.stop();
    screen.run(stop);
    let edit = screen.model.edit_groups(add_group);
    assert_eq!(
        screen.run(edit),
        ok("Groups saved; they apply at the next start.")
    );
    let names: Vec<String> = screen
        .sent_groups()
        .into_iter()
        .map(|(_, name, _)| name)
        .collect();
    assert_eq!(names, ["Drums", "Voices", "Group 3"]);
}

#[test]
fn chained_group_edits_keep_every_groups_id_and_1_based_members_whatever_the_list_came_from() {
    let mut screen = running();
    let edit = screen.model.edit_groups(add_group);
    assert!(screen.run(edit).ok);
    let added = screen
        .harness
        .console
        .state()
        .engine
        .status
        .as_ref()
        .unwrap()
        .groups[2]
        .id
        .clone();
    let stop = screen.harness.console.stop();
    screen.run(stop);

    // No engine now: both edits are built on the last answer's list.
    let member = added.clone();
    let edit = screen
        .model
        .edit_groups(move |groups| set_group_members(groups, &member, &[4], true));
    assert!(screen.run(edit).ok);
    let edit = screen
        .model
        .edit_groups(|groups| rename_group(groups, "vocals", "Voices"));
    assert!(screen.run(edit).ok);
    assert_eq!(
        screen.sent_groups(),
        [
            (Some("drums".to_owned()), "Drums".to_owned(), vec![1, 2, 3]),
            (Some("vocals".to_owned()), "Voices".to_owned(), vec![5, 6]),
            (Some(added.clone()), "Group 3".to_owned(), vec![4]),
        ]
    );

    let start = screen.harness.console.start();
    assert_eq!(screen.run(start), Ok(()));
    let shown: Vec<(String, Vec<usize>)> = screen
        .harness
        .console
        .state()
        .engine
        .status
        .as_ref()
        .unwrap()
        .groups
        .iter()
        .map(|group| (group.id.clone(), group.channels.clone()))
        .collect();
    assert_eq!(
        shown,
        [
            ("drums".to_owned(), vec![0, 1, 2]),
            ("vocals".to_owned(), vec![4, 5]),
            (added.clone(), vec![3]),
        ],
        "the status numbers inputs from 0"
    );
    let edit = screen
        .model
        .edit_groups(|groups| set_group_members(groups, "drums", &[1], false));
    assert!(screen.run(edit).ok);
    let sent: Vec<(Option<String>, Vec<usize>)> = screen
        .sent_groups()
        .into_iter()
        .map(|(id, _, channels)| (id, channels))
        .collect();
    assert_eq!(
        sent,
        [
            (Some("drums".to_owned()), vec![2, 3]),
            (Some("vocals".to_owned()), vec![5, 6]),
            (Some(added), vec![4]),
        ]
    );
}

#[test]
fn a_loaded_session_makes_the_next_group_edit_build_on_the_status_again() {
    let mut screen = running();
    let edit = screen.model.edit_groups(add_group);
    assert!(screen.run(edit).ok);
    // A session loads elsewhere (the Network screen): the desk's groups are the loaded ones.
    screen.kept.borrow_mut().groups.truncate(1);
    show(&mut screen.harness.desk, &screen.kept.borrow());
    screen.harness.emit();
    screen
        .harness
        .console
        .dispatch(Action::SessionLoaded(Box::new(loaded())));

    let edit = screen.model.edit_groups(add_group);
    assert!(screen.run(edit).ok);
    let names: Vec<String> = screen
        .sent_groups()
        .into_iter()
        .map(|(_, name, _)| name)
        .collect();
    assert_eq!(names, ["Drums", "Group 2"], "built on the loaded groups");
}

#[test]
fn a_clicked_shown_box_stays_as_clicked_until_its_edit_is_answered_then_shows_the_desks() {
    let mut screen = running();
    let renders = Rc::new(Cell::new(0));
    let counted = Rc::clone(&renders);
    screen
        .model
        .on_change(move || counted.set(counted.get() + 1));
    screen.harness.hold("set_channels");
    assert!(!screen.model.view().rows[1].hidden);

    let editing = spawned(
        &mut screen.harness,
        screen.model.handle(Edit::Shown {
            index: 1,
            shown: false,
        }),
    );
    assert_eq!(
        screen.harness.calls_to("set_channels").len(),
        1,
        "on its way"
    );
    assert!(!screen.status_hidden(1), "the desk has not changed");
    assert!(
        screen.model.view().rows[1].hidden,
        "the box shows as clicked"
    );
    assert!(
        renders.get() >= 1,
        "the screen was asked to draw the held box"
    );
    screen.harness.desk.emit_status();
    screen.harness.emit();
    assert!(
        screen.model.view().rows[1].hidden,
        "a status with the old value does not flip it back"
    );

    screen.harness.release();
    let outcome = editing.borrow().clone().expect("answered");
    assert!(outcome.ok, "{}", outcome.message);
    assert_eq!(screen.model.view().outcome, Some(outcome));
    assert!(screen.status_hidden(1));
    assert!(
        screen.model.view().rows[1].hidden,
        "now the desk's own value"
    );
}

#[test]
fn a_refused_shown_edit_lets_the_box_show_the_desks_value_again() {
    let mut screen = running();
    screen.harness.hold("set_channels");
    screen
        .harness
        .desk
        .fail_next("set_channels", "the disk is full", 1);
    let editing = spawned(
        &mut screen.harness,
        screen.model.handle(Edit::Shown {
            index: 2,
            shown: false,
        }),
    );
    assert!(screen.model.view().rows[2].hidden);
    screen.harness.release();
    let outcome = editing.borrow().clone().expect("answered");
    assert!(!outcome.ok);
    assert!(
        outcome.message.contains("the disk is full"),
        "{}",
        outcome.message
    );
    assert!(
        !screen.model.view().rows[2].hidden,
        "the box goes back to what the desk has"
    );
    assert_eq!(
        screen.model.view().outcome,
        Some(outcome),
        "the refusal is the alert"
    );
}

#[test]
fn a_clicked_group_box_stays_as_clicked_until_its_edit_is_answered() {
    let mut screen = running();
    screen.harness.hold("set_groups");
    let checked = |model: &Model| {
        model
            .view()
            .cards
            .iter()
            .find(|card| card.id == "drums")
            .and_then(|card| card.choices.iter().find(|choice| choice.key == "4"))
            .map(|choice| choice.checked)
    };
    assert_eq!(
        checked(&screen.model),
        Some(false),
        "CH 4 is in no group yet"
    );

    let editing = spawned(
        &mut screen.harness,
        screen.model.handle(Edit::GroupMember {
            id: "drums".to_owned(),
            key: "4".to_owned(),
            numbers: vec![4],
            member: true,
        }),
    );
    assert_eq!(screen.harness.calls_to("set_groups").len(), 1);
    assert_eq!(
        checked(&screen.model),
        Some(true),
        "held while the desk has it out"
    );
    screen.harness.release();
    assert!(editing.borrow().as_ref().is_some_and(|said| said.ok));
    assert_eq!(checked(&screen.model), Some(true));
    let drums = screen
        .harness
        .console
        .state()
        .engine
        .status
        .as_ref()
        .unwrap()
        .groups
        .iter()
        .find(|group| group.id == "drums")
        .map(|group| group.channels.clone());
    assert_eq!(drums, Some(vec![0, 1, 2, 3]));
}

#[test]
fn every_control_of_the_screen_makes_its_edit() {
    let mut screen = running();
    let said = screen.model.handle(Edit::Rename {
        index: 3,
        name: " Bass DI ".to_owned(),
    });
    assert!(screen.run(said).ok);
    let said = screen.model.handle(Edit::Kind {
        index: 3,
        kind: "keys".to_owned(),
    });
    assert!(screen.run(said).ok);
    let said = screen.model.handle(Edit::Link {
        index: 2,
        linked: true,
    });
    assert!(screen.run(said).ok);
    let view = screen.model.view();
    assert_eq!(view.rows[3].name, "Bass DI");
    assert_eq!(view.rows[3].kind, "keys");
    assert!(view.rows[2].stereo.linked);
    let said = screen.model.handle(Edit::Link {
        index: 3,
        linked: false,
    });
    assert!(screen.run(said).ok);
    assert!(
        !screen.model.view().rows[2].stereo.linked,
        "undone from the second side"
    );

    let said = screen.model.handle(Edit::GroupAdd);
    assert!(screen.run(said).ok);
    let id = screen.model.view().cards[2].id.clone();
    let said = screen.model.handle(Edit::GroupRename {
        id: id.clone(),
        name: "Keys".to_owned(),
    });
    assert!(screen.run(said).ok);
    assert_eq!(screen.model.view().cards[2].name, "Keys");
    let said = screen.model.handle(Edit::GroupDelete { id });
    assert_eq!(screen.run(said), ok("Groups applied to every phone."));
    assert_eq!(screen.model.view().cards.len(), 2);
    assert_eq!(
        screen.model.view().outcome,
        Some(ok("Groups applied to every phone."))
    );
}

#[test]
fn with_no_engine_the_screen_knows_no_inputs_and_no_groups() {
    let screen = screen_over(
        FakeDesk::stopped(),
        Options {
            auto_start: false,
            ..Options::default()
        },
    );
    let view = screen.model.view();
    assert!(!view.known);
    assert!(view.rows.is_empty() && view.cards.is_empty());
    assert_eq!(view.outcome, None);
}

impl Screen {
    fn status_hidden(&self, index: usize) -> bool {
        self.harness
            .console
            .state()
            .engine
            .status
            .as_ref()
            .is_some_and(|status| status.channels[index].hidden)
    }
}
