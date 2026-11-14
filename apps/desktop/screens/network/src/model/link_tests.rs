//! The model's flows for each phone's link panel, Buffer and Redundancy, and for the local output,
//! over the console's simulated desk (the parts of `playback-buffer.test.js`, `redundancy.test.js`,
//! `link-health-controller.test.js` and `local-mix.test.js` that run through the console).

use std::cell::RefCell;
use std::future::Future;

use bandear_desktop_console::command::{Answer, Command, Owner, Reply};
use bandear_desktop_console::console::Options;
use bandear_desktop_console::exec::Slot;
use bandear_desktop_console::router::Section;
use bandear_desktop_console::testing::{FakeDesk, Harness};
use bandear_desktop_console::types::dto::{
    BufferDto, BufferSetDto, LinkHistoryDto, LocalOutputSetDto, OutputDeviceDto, RedundancyDto,
    RedundancySetDto,
};
use bandear_desktop_console::wire::{Buffer, LocalOutput, Musician, Redundancy};

use super::audio::output_value;
use super::flows::OutputOutcome;
use super::*;

const LEAD: &str = "192.168.1.3:50000";
const BASS: &str = "192.168.1.12:50000";
const KEYS: &str = "192.168.1.17:50000";
const DEMO: &str = "Demo interface";
const SPEAKERS: &str = "Speakers (Realtek Audio)";

/// A console connected to `desk`, and the screen's model over it.
fn network(desk: FakeDesk) -> (Harness, Model) {
    let harness = Harness::connected(desk, Options::default());
    let model = Model::new(harness.console.for_owner(Owner::Screen(Section::Network)));
    model.set_zone(Zone::Fixed(0));
    (harness, model)
}

fn names(harness: &Harness, from: usize) -> Vec<&'static str> {
    harness.calls[from..].iter().map(Command::name).collect()
}

/// Runs `flow` on the console and keeps its outcome, even when it ends later.
fn spawn<T: 'static>(
    harness: &Harness,
    flow: impl Future<Output = T> + 'static,
) -> Rc<RefCell<Option<T>>> {
    let outcome = Rc::new(RefCell::new(None));
    let kept = Rc::clone(&outcome);
    harness.console.spawn(async move {
        *kept.borrow_mut() = Some(flow.await);
    });
    outcome
}

fn row<'a>(view: &'a ScreenView, address: &str) -> &'a ShownRow {
    view.table
        .rows
        .iter()
        .find(|row| row.row.address == address)
        .expect("the row")
}

// ---- The link panel ---------------------------------------------------------------------------

fn with_history(mut desk: FakeDesk) -> FakeDesk {
    desk.script("link_history", |desk, command| {
        let Command::LinkHistory { musician_key, .. } = command else {
            return Err("not a history read".to_owned());
        };
        if !desk
            .report
            .musicians
            .iter()
            .any(|musician| &musicians::musician_key(musician) == musician_key)
        {
            return Err(format!("{musician_key} names no musician"));
        }
        Ok(Answer::LinkHistory(LinkHistoryDto {
            musician_key: musician_key.clone(),
            history_id: 1,
            samples: Vec::new(),
            max_samples: 600,
            period_ms: 1000,
        }))
    });
    desk
}

#[test]
fn picking_a_musician_opens_their_link_panel_and_a_status_without_them_closes_it() {
    let (mut harness, model) = network(with_history(FakeDesk::running()));
    model.select_musician(BASS);
    harness.settle();
    let view = model.screen();
    assert!(row(&view, BASS).selected);
    let panel = view.health.expect("the panel");
    assert_eq!(panel.title, "Bass \u{b7} last 10 minutes");
    assert_eq!(
        panel.empty,
        Some("No link reports from this phone yet: the chart starts with its first report.")
    );
    model.set_range(60_000);
    model.show_table(true);
    let panel = model.screen().health.expect("the panel");
    assert_eq!(panel.range_ms, 60_000);
    assert_eq!(panel.table, Some(Vec::new()));

    harness
        .desk
        .report
        .musicians
        .retain(|musician| musician.address != BASS);
    harness.desk.emit_status();
    harness.emit();
    let reads = harness.calls_to("link_history").len();
    harness.run_timers();
    assert_eq!(
        harness.calls_to("link_history").len(),
        reads,
        "no more reads"
    );
    assert_eq!(model.screen().health, None);

    model.select_musician(KEYS);
    harness.settle();
    model.unmount();
    assert_eq!(
        model.screen().health,
        None,
        "leaving the screen closes the panel"
    );
    model.select_musician(KEYS);
    model.close_health();
    assert_eq!(model.screen().health, None, "Close");
}

// ---- Buffer and redundancy ---------------------------------------------------------------------

const LEAD_KEY: &str = "client:5f0c2a8e-1b7d-4c3a-9e6f-2d8b4a7c1e90";
const BASS_KEY: &str = "client:a3d9e7b1-6c2f-4e8a-b5d0-7f1c9e3a2b64";

fn musician_mut<'a>(desk: &'a mut FakeDesk, key: &str) -> Result<&'a mut Musician, String> {
    desk.report
        .musicians
        .iter_mut()
        .find(|musician| musicians::musician_key(musician) == key)
        .ok_or_else(|| format!("{key} is not here"))
}

/// A desk that keeps each phone's buffer and redundancy as the desktop does.
fn with_links(mut desk: FakeDesk) -> FakeDesk {
    desk.script("set_musician_buffer", |desk, command| {
        let Command::SetMusicianBuffer {
            musician_key,
            mode,
            ms,
        } = command
        else {
            return Err("not a buffer".to_owned());
        };
        let musician = musician_mut(desk, musician_key)?;
        let answer = |outcome, buffer| BufferSetDto {
            musician_key: musician_key.clone(),
            outcome,
            buffer,
            saved: true,
            save_error: None,
            message: String::new(),
            min_ms: 5,
            max_ms: 500,
        };
        if !musician.buffer_supported {
            return Ok(Answer::BufferSet(answer("unsupported", None)));
        }
        let fixed = mode == "fixed";
        musician.buffer = Buffer {
            mode: mode.clone(),
            ms: ms.map(|ms| ms as u16),
        };
        Ok(Answer::BufferSet(answer(
            "sent",
            Some(BufferDto {
                mode: if fixed { "fixed" } else { "auto" },
                ms: ms.map(|ms| ms as u16),
            }),
        )))
    });
    desk.script("set_musician_redundancy", |desk, command| {
        let Command::SetMusicianRedundancy {
            musician_key,
            on,
            delay_blocks,
        } = command
        else {
            return Err("not redundancy".to_owned());
        };
        let musician = musician_mut(desk, musician_key)?;
        let delay = delay_blocks.unwrap_or(0) as u8;
        let answer = |outcome, redundancy: Option<RedundancyDto>| RedundancySetDto {
            musician_key: musician_key.clone(),
            outcome,
            bandwidth_factor: if redundancy.is_some_and(|redundancy| redundancy.on) {
                2
            } else {
                1
            },
            redundancy,
            saved: true,
            save_error: None,
            message: String::new(),
            min_delay_blocks: 0,
            max_delay_blocks: 4,
            default_delay_blocks: 0,
        };
        if *on && delay > 0 && !musician.redundancy_hold_supported {
            return Ok(Answer::RedundancySet(answer("unsupported", None)));
        }
        musician.redundancy = Redundancy {
            on: *on,
            delay_blocks: on.then_some(delay),
        };
        Ok(Answer::RedundancySet(answer(
            "set",
            Some(RedundancyDto {
                on: *on,
                delay_blocks: on.then_some(delay),
            }),
        )))
    });
    desk
}

#[test]
fn a_buffer_is_set_in_turn_the_status_read_after_it_and_the_outcome_said_on_the_row() {
    let (mut harness, model) = network(with_links(FakeDesk::running()));
    model.ask_buffer(LEAD, "fixed", "40");
    let pending = model.screen();
    assert_eq!(
        row(&pending, LEAD).buffer_mode,
        musicians::BufferMode::Fixed
    );
    assert!(
        row(&pending, LEAD).buffer_pending,
        "shown until the row reports it"
    );
    let from = harness.calls.len();
    harness.settle();

    assert!(matches!(
        &harness.calls[from],
        Command::SetMusicianBuffer { musician_key, mode, ms: Some(40) } if musician_key == LEAD_KEY && mode == "fixed"
    ));
    assert_eq!(
        names(&harness, from),
        ["set_musician_buffer", "engine_status"]
    );
    let view = model.screen();
    let lead = row(&view, LEAD);
    assert_eq!(
        lead.buffer_said,
        Some(Said::new(
            true,
            "Sent Lead vocal's phone a fixed 40 ms buffer."
        ))
    );
    assert!(!lead.buffer_pending);
    assert_eq!(lead.row.buffer.ms, Some(40));
    assert_eq!(row(&view, BASS).buffer_said, None, "said on its own row");

    model.ask_buffer(BASS, "fixed", "40");
    harness.settle();
    assert_eq!(
        row(&model.screen(), BASS).buffer_said,
        Some(Said::new(
            false,
            "Bass's phone needs a newer app to take its buffer from the desk."
        ))
    );
    assert_eq!(
        row(&model.screen(), LEAD).buffer_said,
        None,
        "the last outcome only"
    );

    let sent = harness.calls_to("set_musician_buffer").len();
    model.ask_buffer(LEAD, "fixed", "4");
    harness.settle();
    assert_eq!(
        harness.calls_to("set_musician_buffer").len(),
        sent,
        "checked first"
    );
    let lead = row(&model.screen(), LEAD).clone();
    assert_eq!(
        lead.buffer_said,
        Some(Said::new(
            false,
            "A fixed buffer is from 5 to 500 ms, not 4 ms."
        ))
    );
    assert!(!lead.buffer_pending, "a refused length settles the choice");

    harness
        .desk
        .fail_next("set_musician_buffer", "the engine is not running", 1);
    model.ask_buffer(LEAD, "auto", "");
    harness.settle();
    assert_eq!(
        row(&model.screen(), LEAD).buffer_said,
        Some(Said::new(
            false,
            "The buffer was not set: the engine is not running."
        ))
    );
}

#[test]
fn a_buffer_waits_for_a_desk_edit_asked_for_before_it() {
    let (mut harness, model) = network(with_links(FakeDesk::running()));
    let gate = Slot::new();
    let held = gate.clone();
    let edit = harness
        .console
        .in_turn(true, async move { held.wait().await });
    harness.console.spawn(edit);
    model.ask_buffer(LEAD, "fixed", "60");
    harness.settle();
    assert!(
        harness.calls_to("set_musician_buffer").is_empty(),
        "in turn"
    );

    gate.fill(());
    harness.console.drive();
    harness.settle();
    assert_eq!(harness.calls_to("set_musician_buffer").len(), 1);
}

#[test]
fn a_pending_buffer_choice_settles_by_value_once_the_row_reports_it() {
    let (mut harness, model) = network(with_links(FakeDesk::running()));
    harness.hold("set_musician_buffer");
    model.ask_buffer(LEAD, "fixed", "040");
    harness.settle();
    harness.desk.report.musicians[0].buffer = Buffer {
        mode: "fixed".to_owned(),
        ms: Some(40),
    };
    harness.desk.emit_status();
    harness.emit();
    assert!(
        !row(&model.screen(), LEAD).buffer_pending,
        "\"040\" typed is the 40 reported"
    );
}

#[test]
fn redundancy_is_set_in_turn_the_status_read_after_it_and_the_outcome_said_on_the_row() {
    let (mut harness, model) = network(with_links(FakeDesk::running()));
    model.ask_redundancy(BASS, true, None);
    assert!(
        row(&model.screen(), BASS).redundancy_on,
        "shown as switched"
    );
    harness.settle();
    assert!(matches!(
        harness.calls_to("set_musician_redundancy").last(),
        Some(Command::SetMusicianRedundancy { musician_key, on: true, delay_blocks: None }) if musician_key == BASS_KEY
    ));
    let view = model.screen();
    assert_eq!(
        row(&view, BASS).redundancy_said,
        Some(Said::new(
            true,
            "Bass's audio is now sent twice, each copy right after its original: their bandwidth doubles (\u{d7}2), with no added latency."
        ))
    );
    assert_eq!(
        harness.console.state().musicians[1].redundancy,
        Redundancy {
            on: true,
            delay_blocks: Some(0)
        }
    );

    model.ask_redundancy(BASS, true, Some(2));
    harness.settle();
    let view = model.screen();
    assert_eq!(
        row(&view, BASS)
            .redundancy_said
            .as_ref()
            .map(|said| said.message.as_str()),
        Some(
            "Bass's audio is now sent twice, each copy 2 blocks later: their bandwidth doubles (\u{d7}2) and their phone plays 10.7 ms later."
        )
    );
    assert_eq!(row(&view, BASS).redundancy_delay, 2);

    model.ask_redundancy(KEYS, true, Some(2));
    harness.settle();
    let keys = row(&model.screen(), KEYS).clone();
    assert_eq!(
        keys.redundancy_said,
        Some(Said::new(
            false,
            "Keys's phone needs a newer app for a delayed copy; an immediate copy works with any app."
        ))
    );
    assert!(
        !keys.redundancy_on,
        "the row shows what the engine reports again"
    );

    harness
        .desk
        .fail_next("set_musician_redundancy", "the engine is stopping", 1);
    model.ask_redundancy(BASS, false, None);
    harness.settle();
    assert_eq!(
        row(&model.screen(), BASS).redundancy_said,
        Some(Said::new(
            false,
            "Redundancy was not set: the engine is stopping."
        ))
    );
}

// ---- The local output -------------------------------------------------------------------------

fn sample_outputs() -> Vec<OutputDeviceDto> {
    vec![
        OutputDeviceDto {
            name: DEMO.to_owned(),
            channels: 4,
        },
        OutputDeviceDto {
            name: SPEAKERS.to_owned(),
            channels: 2,
        },
    ]
}

/// A desk that lists its outputs and plays the local mix on one of them.
fn with_outputs(mut desk: FakeDesk) -> FakeDesk {
    desk.script("output_devices", |desk, _| {
        if !desk.running {
            return Err("no engine is running".to_owned());
        }
        Ok(Answer::OutputDevices(sample_outputs()))
    });
    desk.script("set_local_output", |desk, command| {
        let Command::SetLocalOutput { output } = command else {
            return Err("not an output".to_owned());
        };
        if let Some(output) = output
            && !sample_outputs()
                .iter()
                .any(|offered| offered.name == output.device)
        {
            return Err(format!(
                "no output device named \"{}\" is offered",
                output.device
            ));
        }
        desk.report.status.local_output = Some(match output {
            Some(output) => LocalOutput {
                device: Some(output.device.clone()),
                first_channel: Some(output.first_channel),
                state: "running".to_owned(),
                ..LocalOutput::default()
            },
            None => LocalOutput {
                state: "off".to_owned(),
                ..LocalOutput::default()
            },
        });
        Ok(Answer::LocalOutputSet(LocalOutputSetDto {
            output: output.as_ref().map(|output| {
                serde_json::from_value(serde_json::json!({
                    "device": output.device,
                    "firstChannel": output.first_channel,
                }))
                .expect("an output")
            }),
            local_output: None,
            applied: true,
            saved: true,
            save_error: None,
        }))
    });
    desk
}

fn played(harness: &Harness) -> Option<String> {
    harness
        .console
        .state()
        .engine
        .status
        .as_ref()
        .and_then(|status| status.local_output.as_ref())
        .and_then(|output| output.device.clone())
}

#[test]
fn the_outputs_are_listed_once_for_each_engine_run_while_the_screen_is_shown() {
    let (mut harness, model) = network(with_outputs(FakeDesk::running()));
    assert!(
        harness.calls_to("output_devices").is_empty(),
        "not before the screen shows"
    );
    model.mount();
    harness.settle();
    assert_eq!(harness.calls_to("output_devices").len(), 1);
    assert_eq!(
        harness.console.state().outputs.list.as_deref(),
        Some(&sample_outputs()[..])
    );
    harness.desk.emit_status();
    harness.emit();
    assert_eq!(harness.calls_to("output_devices").len(), 1, "the same run");

    let start = harness.console.start();
    harness.run(start);
    assert_eq!(harness.calls_to("output_devices").len(), 2, "a new run");

    model.unmount();
    let start = harness.console.start();
    harness.run(start);
    assert_eq!(
        harness.calls_to("output_devices").len(),
        2,
        "not while hidden"
    );
    model.mount();
    harness.settle();
    assert_eq!(
        harness.calls_to("output_devices").len(),
        3,
        "shown again: listed again"
    );
}

#[test]
fn an_output_list_answered_for_an_engine_run_that_has_ended_is_not_taken() {
    let (mut harness, model) = network(with_outputs(FakeDesk::running()));
    harness.hold("output_devices");
    let listing = model.load_outputs();
    harness.console.spawn(listing);
    harness.settle();
    let start = harness.console.start();
    harness.run(start);
    harness.release();
    assert_eq!(
        harness.console.state().outputs.list,
        None,
        "the ended run's outputs are not shown"
    );
}

#[test]
fn a_choice_of_the_local_output_is_said_and_the_status_read_after_it() {
    let (mut harness, model) = network(with_outputs(FakeDesk::running()));
    model.mount();
    harness.settle();
    model.choose_output(&output_value(DEMO, 2));
    let from = harness.calls.len();
    harness.settle();
    assert_eq!(names(&harness, from), ["set_local_output", "engine_status"]);
    let view = model.screen();
    assert_eq!(
        view.output_said,
        Some(Said::new(
            true,
            "The local mix plays on Demo interface \u{b7} outputs 3/4."
        ))
    );
    assert_eq!(view.local_output.value, output_value(DEMO, 2));
    assert_eq!(played(&harness).as_deref(), Some(DEMO));

    model.choose_output("0|Nowhere");
    harness.settle();
    assert_eq!(
        model.screen().output_said,
        Some(Said::new(
            false,
            "The local mix output was not changed: no output device named \"Nowhere\" is offered."
        ))
    );
    model.choose_output("");
    harness.settle();
    assert!(
        matches!(
            harness.calls_to("set_local_output").last(),
            Some(Command::SetLocalOutput { output: None })
        ),
        "None is no output"
    );
    assert_eq!(
        model.screen().output_said.map(|said| said.message),
        Some("The local mix plays nowhere now.".to_owned())
    );
}

#[test]
fn quick_output_choices_go_out_one_at_a_time_and_the_last_one_wins() {
    let (mut harness, model) = network(with_outputs(FakeDesk::running()));
    harness.hold("set_local_output");
    let choose = |device: &str, first| {
        Some(bandear_desktop_console::types::dto::LocalOutputRequestDto {
            device: device.to_owned(),
            first_channel: first,
        })
    };
    let one = spawn(&harness, model.set_local_output(choose(DEMO, 0)));
    harness.settle();
    let two = spawn(&harness, model.set_local_output(choose(DEMO, 2)));
    let three = spawn(&harness, model.set_local_output(choose(SPEAKERS, 0)));
    harness.settle();
    assert_eq!(
        harness.calls_to("set_local_output").len(),
        1,
        "one choice in flight"
    );
    for _ in 0..4 {
        harness.release();
    }

    let sent: Vec<_> = harness
        .calls_to("set_local_output")
        .into_iter()
        .filter_map(|command| match command {
            Command::SetLocalOutput {
                output: Some(output),
            } => Some((output.device.clone(), output.first_channel)),
            _ => None,
        })
        .collect();
    assert_eq!(
        sent,
        [(DEMO.to_owned(), 0), (SPEAKERS.to_owned(), 0)],
        "the choice overtaken while waiting never goes out"
    );
    let outcome =
        |cell: &Rc<RefCell<Option<OutputOutcome>>>| cell.borrow().clone().expect("answered");
    assert!(outcome(&one).superseded);
    assert!(outcome(&two).superseded);
    let last = outcome(&three);
    assert!(!last.superseded);
    assert_eq!(
        last.message.as_deref(),
        Some("The local mix plays on Speakers (Realtek Audio) \u{b7} outputs 1/2.")
    );
    assert_eq!(played(&harness).as_deref(), Some(SPEAKERS));
}

#[test]
fn a_status_that_cannot_be_read_after_a_choice_is_said_and_the_next_choice_still_goes() {
    let (mut harness, model) = network(with_outputs(FakeDesk::running()));
    harness
        .desk
        .fail_next("engine_status", "the desk did not answer", 1);
    model.choose_output(&output_value(DEMO, 2));
    harness.settle();
    assert_eq!(
        model.screen().output_said,
        Some(Said::new(
            true,
            "The local mix plays on Demo interface \u{b7} outputs 3/4. Where it plays now could not be read: the desk did not answer."
        ))
    );
    model.choose_output(&output_value(SPEAKERS, 0));
    harness.settle();
    assert_eq!(
        model.screen().output_said,
        Some(Said::new(
            true,
            "The local mix plays on Speakers (Realtek Audio) \u{b7} outputs 1/2."
        ))
    );
    assert_eq!(played(&harness).as_deref(), Some(SPEAKERS));
}

#[test]
fn an_answer_that_cannot_be_read_is_said_as_such_and_the_next_choice_still_goes() {
    let mut desk = with_outputs(FakeDesk::running());
    let garbled = Rc::new(std::cell::Cell::new(true));
    let once = Rc::clone(&garbled);
    let mut real = with_outputs(FakeDesk::running());
    desk.script("set_local_output", move |desk, command| {
        // The real answer, done on this desk, except the first comes back as something else.
        let reply: Reply = real.answer(command);
        desk.report.status.local_output = real.report.status.local_output.clone();
        if once.replace(false) {
            return Ok(Answer::Done);
        }
        reply
    });
    let (mut harness, model) = network(desk);
    model.choose_output(&output_value(DEMO, 2));
    harness.settle();
    let said = model.screen().output_said.expect("said");
    assert!(said.ok, "the desk took it");
    assert_eq!(
        said.message,
        "The local mix output was changed, but the desk's answer could not be read: the desk answered `set_local_output` with something it does not answer."
    );
    model.choose_output(&output_value(SPEAKERS, 0));
    harness.settle();
    assert_eq!(
        model.screen().output_said.map(|said| said.message),
        Some("The local mix plays on Speakers (Realtek Audio) \u{b7} outputs 1/2.".to_owned())
    );
    assert!(!garbled.get());
}
