//! `network-view.test.js` (rows, addresses), `playback-buffer.test.js` and `redundancy.test.js`
//! (the cells and their words), ported.

use bandear_desktop_console::format::LinkLevel;
use bandear_desktop_console::state::{Action, ConsoleState};
use bandear_desktop_console::testing::sample_status;
use bandear_desktop_console::types::dto::{BufferDto, RedundancyDto};
use bandear_desktop_console::wire::{Buffer, LinkSample, Redundancy, StatusReport};

use super::*;

const LEAD: &str = "client:5f0c2a8e-1b7d-4c3a-9e6f-2d8b4a7c1e90";
const BASS: &str = "client:a3d9e7b1-6c2f-4e8a-b5d0-7f1c9e3a2b64";

fn with(report: StatusReport) -> ConsoleState {
    let mut state = ConsoleState::new();
    state.reduce(&Action::Status(Box::new(report)));
    state
}

fn delays(hold: bool, block_ms: f64) -> Vec<DelayChoice> {
    (0..=4)
        .map(|value| {
            let label = format!("Copy: {}", delay_label(value, block_ms));
            let disabled = value > 0 && !hold;
            DelayChoice {
                value,
                label: if disabled {
                    format!("{label}, needs a newer app")
                } else {
                    label
                },
                disabled,
            }
        })
        .collect()
}

#[test]
fn a_row_per_musician_with_the_name_who_owns_it_the_link_and_the_counts() {
    let mut report = sample_status();
    report.musicians[0].packets_sent = 1200;
    report.musicians[0].packets_discarded = 30;
    let client_id = report.musicians[0].client_id.clone();
    let rows = musician_rows(&with(report));

    assert_eq!(rows.len(), 4);
    assert_eq!(
        rows[0],
        MusicianRow {
            address: "192.168.1.3:50000".to_owned(),
            client_id,
            name: "Lead vocal".to_owned(),
            editable: false,
            name_is_local: false,
            label_follows: LabelFollows::Phone,
            connected: true,
            link: LinkState {
                level: LinkLevel::Good,
                label: "Good".to_owned(),
                detail: "Loss 0.01% \u{b7} Glitches 0.0/s \u{b7} Timeouts 0.0/s".to_owned(),
            },
            mix: "85% \u{b7} live".to_owned(),
            sent: "1,200".to_owned(),
            discarded: "30".to_owned(),
            loss: "2.44%".to_owned(),
            loss_high: true,
            buffer: BufferView {
                key: LEAD.to_owned(),
                supported: true,
                mode: BufferMode::Auto,
                ms: None,
                playing: None,
                note: None,
            },
            redundancy: RedundancyView {
                key: LEAD.to_owned(),
                on: false,
                delay_blocks: None,
                hold_supported: false,
                delays: delays(false, DEFAULT_BLOCK_MS),
                hold_note: Some("Delayed copies: needs a newer app".to_owned()),
                bandwidth: "Sends once; on doubles this musician's bandwidth".to_owned(),
                recovered: None,
                duplicates: None,
            },
        }
    );
    assert_eq!(
        rows[0].redundancy.delays[1].label,
        "Copy: 1 block later (+5.3 ms), needs a newer app"
    );
    assert!(
        rows[1].redundancy.hold_supported,
        "Bass's phone holds packets back for copies"
    );
    assert_eq!(
        rows[1].buffer.note,
        Some("needs a newer app"),
        "Bass's phone takes no buffer"
    );
    assert_eq!(rows[3].name, "Drums");
    assert!(rows[3].editable, "no announced name: the desk may label it");
    assert!(rows[3].name_is_local);
    assert_eq!(rows[1].loss, "0.00%");
    assert!(!rows[1].loss_high);
    assert_eq!(rows[0].name_title(), CLIENT_NAME_TITLE);
    assert_eq!(rows[3].name_title(), LOCAL_LABEL_FOR_PHONE);
}

#[test]
fn a_label_follows_a_phone_that_has_a_client_id_and_stays_with_the_address_otherwise() {
    let mut report = sample_status();
    report.musicians[1].client_id = None;
    report.musicians[2].client_id = Some(String::new());
    let rows = musician_rows(&with(report));

    assert_eq!(rows[3].label_follows, LabelFollows::Phone);
    assert_eq!(
        rows[1].client_id, None,
        "a configured target or an older phone"
    );
    assert_eq!(rows[1].label_follows, LabelFollows::Address);
    assert_eq!(rows[2].client_id, None, "an empty id reads as none");
    assert_eq!(rows[2].label_follows, LabelFollows::Address);

    let mut report = sample_status();
    report.musicians[3].client_id = None;
    assert_eq!(
        musician_rows(&with(report))[3].name_title(),
        LOCAL_LABEL_FOR_ADDRESS,
        "a local label kept for the address says so"
    );
}

#[test]
fn a_muted_musicians_mix_reads_muted() {
    let mut report = sample_status();
    report.musicians[1].mix.muted = true;
    assert_eq!(musician_rows(&with(report))[1].mix, "100% \u{b7} muted");
}

#[test]
fn an_address_is_checked_before_it_is_sent_with_a_sentence_that_says_what_is_wrong() {
    assert_eq!(
        validate_address(" 192.168.1.50 ").as_deref(),
        Ok("192.168.1.50")
    );
    assert_eq!(
        validate_address("192.168.1.50:50000").as_deref(),
        Ok("192.168.1.50:50000")
    );
    assert_eq!(
        validate_address("[fe80::1]:50000").as_deref(),
        Ok("[fe80::1]:50000")
    );
    assert_eq!(
        validate_address("stage-rack.local").as_deref(),
        Ok("stage-rack.local")
    );
    assert_eq!(
        validate_address("fe80::1").as_deref(),
        Ok("fe80::1"),
        "no port to check"
    );

    assert_eq!(
        validate_address("").unwrap_err(),
        "Type the phone's address, for example 192.168.1.50."
    );
    assert_eq!(
        validate_address("192.168.1 50").unwrap_err(),
        "An address has no spaces: 192.168.1 50"
    );
    assert_eq!(
        validate_address("192.168.1.300").unwrap_err(),
        "192.168.1.300 is not an IP address: each part goes up to 255."
    );
    assert_eq!(
        validate_address("192.168.1").unwrap_err(),
        "192.168.1 is not an IP address such as 192.168.1.50."
    );
    assert_eq!(
        validate_address("192.168.1.5:0").unwrap_err(),
        "The port must be from 1 to 65535, not 0."
    );
    assert_eq!(
        validate_address("192.168.1.5:70000").unwrap_err(),
        "The port must be from 1 to 65535, not 70000."
    );
}

fn lead_row(buffer: Buffer, cushion: Option<f64>) -> Musician {
    let mut musician = sample_status().musicians[0].clone();
    musician.buffer = buffer;
    musician.link_latest = cushion.map(|cushion_ms| LinkSample {
        cushion_ms: Some(cushion_ms),
        ..LinkSample::default()
    });
    musician
}

fn fixed(ms: u16) -> Buffer {
    Buffer {
        mode: "fixed".to_owned(),
        ms: Some(ms),
    }
}

#[test]
fn the_buffer_cell_shows_the_mode_a_fixed_length_and_the_cushion_the_phone_plays() {
    assert_eq!(
        buffer_view(&lead_row(fixed(40), Some(42.6667))),
        BufferView {
            key: LEAD.to_owned(),
            supported: true,
            mode: BufferMode::Fixed,
            ms: Some(40),
            playing: Some("plays 42.7 ms".to_owned()),
            note: None,
        }
    );
    assert_eq!(
        buffer_view(&lead_row(Buffer::default(), None)),
        BufferView {
            key: LEAD.to_owned(),
            supported: true,
            mode: BufferMode::Auto,
            ms: None,
            playing: None,
            note: None,
        }
    );
    let mut older = lead_row(Buffer::default(), None);
    older.client_id = None;
    older.buffer_supported = false;
    let older = buffer_view(&older);
    assert_eq!(older.key, "address:192.168.1.3:50000");
    assert!(!older.supported);
    assert_eq!(older.note, Some(NEEDS_NEWER_APP));
    assert_eq!(older.mode, BufferMode::Auto);
}

#[test]
fn a_fixed_buffer_is_whole_milliseconds_within_its_range_and_auto_needs_none() {
    assert_eq!(
        (MIN_BUFFER_MS, MAX_BUFFER_MS, DEFAULT_FIXED_MS),
        (5, 500, 40)
    );
    let asked = |mode: BufferMode, ms: Option<u32>| Ok(BufferRequest { mode, ms });
    assert_eq!(
        buffer_request("auto", "garbage"),
        asked(BufferMode::Auto, None)
    );
    assert_eq!(
        buffer_request("fixed", " 40 "),
        asked(BufferMode::Fixed, Some(40))
    );
    assert_eq!(
        buffer_request("fixed", "5"),
        asked(BufferMode::Fixed, Some(5))
    );
    assert_eq!(
        buffer_request("fixed", "500"),
        asked(BufferMode::Fixed, Some(500))
    );
    assert_eq!(
        buffer_request("fixed", "040"),
        asked(BufferMode::Fixed, Some(40))
    );
    for (text, says) in [
        ("", "whole milliseconds"),
        ("12.5", "whole milliseconds"),
        ("-3", "whole milliseconds"),
        ("4", "not 4 ms"),
        ("501", "not 501 ms"),
    ] {
        let refused = buffer_request("fixed", text).unwrap_err();
        assert!(refused.contains(says), "{text}: {refused}");
    }
    assert_eq!(
        buffer_request("fixed", "").unwrap_err(),
        "Type the buffer in whole milliseconds, from 5 to 500."
    );
    assert_eq!(
        buffer_request("fixed", "501").unwrap_err(),
        "A fixed buffer is from 5 to 500 ms, not 501 ms."
    );
    assert_eq!(
        buffer_request("elastic", "40").unwrap_err(),
        "The buffer is Auto or Fixed, not elastic."
    );
}

fn buffer_set(outcome: &'static str, buffer: Option<BufferDto>) -> BufferSetDto {
    BufferSetDto {
        musician_key: LEAD.to_owned(),
        outcome,
        buffer,
        saved: true,
        save_error: None,
        message: String::new(),
        min_ms: 5,
        max_ms: 500,
    }
}

#[test]
fn the_desks_answer_is_said_for_the_musician_and_a_failed_save_after_it() {
    let fixed = Some(BufferDto {
        mode: "fixed",
        ms: Some(40),
    });
    let auto = Some(BufferDto {
        mode: "auto",
        ms: None,
    });
    assert_eq!(
        format_buffer_set(&buffer_set("sent", fixed), "Ana"),
        Said::new(true, "Sent Ana's phone a fixed 40 ms buffer.")
    );
    assert_eq!(
        format_buffer_set(&buffer_set("sent", auto), "Ana"),
        Said::new(true, "Ana's phone is back on Auto.")
    );
    assert_eq!(
        format_buffer_set(&buffer_set("notConnected", fixed), "Ana"),
        Said::new(
            true,
            "Kept for Ana: their phone gets a fixed 40 ms buffer when it is connected again."
        )
    );
    assert_eq!(
        format_buffer_set(&buffer_set("notConnected", auto), "Ana").message,
        "Kept for Ana: their phone gets Auto when it is connected again."
    );
    assert_eq!(
        format_buffer_set(&buffer_set("unsupported", None), "Bass"),
        Said::new(
            false,
            "Bass's phone needs a newer app to take its buffer from the desk."
        )
    );
    let invalid = BufferSetDto {
        message: "Not set: too long.".to_owned(),
        ..buffer_set("invalid", None)
    };
    assert_eq!(
        format_buffer_set(&invalid, "Ana"),
        Said::new(false, "Not set: too long.")
    );
    assert_eq!(
        format_buffer_set(&buffer_set("invalid", None), "Ana").message,
        "The buffer was not set."
    );
    let unsaved = BufferSetDto {
        saved: false,
        save_error: Some("disk full".to_owned()),
        ..buffer_set("sent", fixed)
    };
    assert_eq!(
        format_buffer_set(&unsaved, "Ana").message,
        "Sent Ana's phone a fixed 40 ms buffer. It was not saved for the next start: disk full."
    );
    let musicians = vec![lead_row(Buffer::default(), None)];
    assert_eq!(name_for_key(&musicians, LEAD), "Lead vocal");
    assert_eq!(name_for_key(&musicians, BASS), BASS);
}

#[test]
fn each_table_row_carries_its_buffer_cell() {
    let mut report = sample_status();
    report.musicians[0].buffer = fixed(80);
    let rows = musician_rows(&with(report));
    assert_eq!(rows[0].buffer.mode, BufferMode::Fixed);
    assert_eq!(rows[0].buffer.ms, Some(80));
    assert_eq!(rows[0].buffer.key, LEAD);
}

#[test]
fn a_pending_buffer_choice_settles_once_the_row_reports_it_compared_by_value() {
    let auto = buffer_view(&lead_row(Buffer::default(), None));
    let fixed40 = buffer_view(&lead_row(fixed(40), None));
    let asked = |mode: &str, ms: Option<&str>| BufferAsked {
        mode: mode.to_owned(),
        ms: ms.map(str::to_owned),
    };
    // "040" typed is 40 reported: as strings they would never match.
    assert!(buffer_choice_settled(
        &asked("fixed", Some("040")),
        &fixed40
    ));
    assert!(buffer_choice_settled(
        &asked("fixed", Some(" 40 ")),
        &fixed40
    ));
    assert!(!buffer_choice_settled(&asked("fixed", Some("040")), &auto));
    assert!(!buffer_choice_settled(
        &asked("fixed", Some("41")),
        &fixed40
    ));
    assert!(
        buffer_choice_settled(&asked("auto", None), &auto),
        "Auto settles on Auto whatever was typed"
    );
    assert!(!buffer_choice_settled(&asked("auto", None), &fixed40));
}

fn bass_row(redundancy: Redundancy, duplicates: u64, recovered: u64) -> Musician {
    let mut musician = sample_status().musicians[1].clone();
    musician.redundancy = redundancy;
    musician.duplicates_sent = duplicates;
    musician.recovered = recovered;
    musician
}

fn on(delay: Option<u8>) -> Redundancy {
    Redundancy {
        on: true,
        delay_blocks: delay,
    }
}

#[test]
fn one_engine_block_lasts_its_buffer_at_the_devices_rate_else_the_default_block() {
    let format = |buffer: &str, rate: u32| CaptureFormat {
        channels: 2,
        sample_rate: rate,
        sample_format: "I24".to_owned(),
        buffer_size: buffer.to_owned(),
    };
    assert!((DEFAULT_BLOCK_MS - 5.333).abs() < 0.001);
    assert_eq!(
        block_ms_of(Some(&format("Fixed(256)", 48_000))),
        DEFAULT_BLOCK_MS
    );
    assert_eq!(
        block_ms_of(Some(&format("Fixed(512)", 96_000))),
        DEFAULT_BLOCK_MS,
        "decimated to 48 kHz"
    );
    assert!((block_ms_of(Some(&format("Fixed(128)", 48_000))) - 2.667).abs() < 0.001);
    assert_eq!(
        block_ms_of(Some(&format("device default", 48_000))),
        DEFAULT_BLOCK_MS
    );
    assert_eq!(block_ms_of(None), DEFAULT_BLOCK_MS);
    assert_eq!(delay_label(0, DEFAULT_BLOCK_MS), "immediate");
    assert_eq!(delay_label(1, DEFAULT_BLOCK_MS), "1 block later (+5.3 ms)");
    assert_eq!(delay_label(2, 2.5), "2 blocks later (+5.0 ms)");
}

#[test]
fn the_redundancy_cell_shows_whether_it_is_on_its_copy_what_it_costs_and_what_it_recovered() {
    let delayed = redundancy_view(&bass_row(on(Some(2)), 1200, 7), DEFAULT_BLOCK_MS);
    assert_eq!(delayed.key, BASS);
    assert!(delayed.on);
    assert_eq!(delayed.delay_blocks, Some(2));
    assert!(delayed.hold_supported);
    assert_eq!(delayed.hold_note, None);
    assert_eq!(
        delayed.bandwidth,
        "2\u{d7} bandwidth, +10.7 ms latency on this phone"
    );
    assert_eq!(delayed.recovered.as_deref(), Some("7 packets recovered"));
    assert_eq!(delayed.duplicates.as_deref(), Some("1,200 copies sent"));
    let labels: Vec<_> = delayed
        .delays
        .iter()
        .map(|delay| delay.label.as_str())
        .collect();
    assert_eq!(
        labels,
        [
            "Copy: immediate",
            "Copy: 1 block later (+5.3 ms)",
            "Copy: 2 blocks later (+10.7 ms)",
            "Copy: 3 blocks later (+16.0 ms)",
            "Copy: 4 blocks later (+21.3 ms)",
        ]
    );
    assert!(delayed.delays.iter().all(|delay| !delay.disabled));

    let off = redundancy_view(&bass_row(Redundancy::default(), 0, 0), DEFAULT_BLOCK_MS);
    assert!(!off.on);
    assert_eq!(off.delay_blocks, None);
    assert_eq!(
        off.bandwidth,
        "Sends once; on doubles this musician's bandwidth"
    );
    assert_eq!((off.recovered, off.duplicates), (None, None));

    let immediate = redundancy_view(&bass_row(on(Some(0)), 0, 0), DEFAULT_BLOCK_MS);
    assert_eq!(immediate.bandwidth, "2\u{d7} bandwidth, no added latency");
    assert_eq!(
        immediate.recovered.as_deref(),
        Some("Recovered packets are counted for delayed copies only")
    );
    assert_eq!(
        redundancy_view(&bass_row(on(None), 0, 0), DEFAULT_BLOCK_MS).delay_blocks,
        Some(DEFAULT_DELAY_BLOCKS),
        "a delay not said is the default, an immediate copy"
    );
    assert_eq!(DEFAULT_DELAY_BLOCKS, 0);
    let one = redundancy_view(&bass_row(on(Some(1)), 0, 1), 2.5);
    assert_eq!(
        one.bandwidth,
        "2\u{d7} bandwidth, +2.5 ms latency on this phone"
    );
    assert_eq!(one.recovered.as_deref(), Some("1 packet recovered"));
    assert_eq!(
        redundancy_view(&bass_row(on(Some(1)), 0, 1234), 2.5)
            .recovered
            .as_deref(),
        Some("1,234 packets recovered")
    );

    // An older phone takes immediate copies only and reports nothing it recovered.
    let mut older = bass_row(on(Some(0)), 0, 0);
    older.client_id = None;
    older.redundancy_supported = false;
    older.redundancy_hold_supported = false;
    let older = redundancy_view(&older, DEFAULT_BLOCK_MS);
    assert_eq!(older.key, "address:192.168.1.12:50000");
    assert_eq!(
        older.recovered.as_deref(),
        Some("Recovered packets not reported by this phone")
    );
    assert_eq!(
        older.hold_note.as_deref(),
        Some("Delayed copies: needs a newer app")
    );
    assert_eq!(
        older.delays[0],
        DelayChoice {
            value: 0,
            label: "Copy: immediate".to_owned(),
            disabled: false
        }
    );
    assert_eq!(
        older.delays[2],
        DelayChoice {
            value: 2,
            label: "Copy: 2 blocks later (+10.7 ms), needs a newer app".to_owned(),
            disabled: true
        }
    );
    assert!(older.delays[1..].iter().all(|delay| delay.disabled));
}

fn redundancy_set(redundancy: Option<RedundancyDto>, factor: u8) -> RedundancySetDto {
    RedundancySetDto {
        musician_key: BASS.to_owned(),
        outcome: "set",
        redundancy,
        bandwidth_factor: factor,
        saved: true,
        save_error: None,
        message: "Each packet is sent twice ...".to_owned(),
        min_delay_blocks: 0,
        max_delay_blocks: 4,
        default_delay_blocks: 0,
    }
}

fn copies(delay: Option<u8>) -> Option<RedundancyDto> {
    Some(RedundancyDto {
        on: true,
        delay_blocks: delay,
    })
}

#[test]
fn the_outcome_of_a_change_is_said_with_what_it_does_to_the_bandwidth_and_the_latency() {
    assert_eq!(
        format_redundancy_set(
            &redundancy_set(copies(Some(0)), 2),
            "Bass",
            DEFAULT_BLOCK_MS
        ),
        Said::new(
            true,
            "Bass's audio is now sent twice, each copy right after its original: their bandwidth doubles (\u{d7}2), with no added latency."
        )
    );
    assert_eq!(
        format_redundancy_set(
            &redundancy_set(copies(Some(2)), 2),
            "Bass",
            DEFAULT_BLOCK_MS
        ),
        Said::new(
            true,
            "Bass's audio is now sent twice, each copy 2 blocks later: their bandwidth doubles (\u{d7}2) and their phone plays 10.7 ms later."
        )
    );
    let off = Some(RedundancyDto {
        on: false,
        delay_blocks: None,
    });
    assert_eq!(
        format_redundancy_set(&redundancy_set(off, 1), "Bass", DEFAULT_BLOCK_MS),
        Said::new(
            true,
            "Bass's audio is sent once again: their bandwidth is back to normal (\u{d7}1)."
        )
    );
    let unsaved = RedundancySetDto {
        saved: false,
        save_error: Some("disk full".to_owned()),
        ..redundancy_set(copies(Some(1)), 2)
    };
    assert_eq!(
        format_redundancy_set(&unsaved, "Bass", 2.5).message,
        "Bass's audio is now sent twice, each copy 1 block later: their bandwidth doubles (\u{d7}2) and their phone plays 2.5 ms later. It was not saved for the next start: disk full."
    );
    let unsupported = RedundancySetDto {
        outcome: "unsupported",
        ..redundancy_set(None, 1)
    };
    assert_eq!(
        format_redundancy_set(&unsupported, "Keys", DEFAULT_BLOCK_MS),
        Said::new(
            false,
            "Keys's phone needs a newer app for a delayed copy; an immediate copy works with any app."
        )
    );
    let invalid = RedundancySetDto {
        outcome: "invalid",
        message: "Not set: a copy must be from 0 to 4 blocks late, got 9.".to_owned(),
        ..redundancy_set(None, 1)
    };
    assert_eq!(
        format_redundancy_set(&invalid, "Bass", DEFAULT_BLOCK_MS),
        Said::new(
            false,
            "Not set: a copy must be from 0 to 4 blocks late, got 9."
        )
    );
}

#[test]
fn a_pending_redundancy_choice_settles_once_the_row_reports_it_and_its_delay() {
    let cell = |redundancy| redundancy_view(&bass_row(redundancy, 0, 0), DEFAULT_BLOCK_MS);
    let (off, immediate, two) = (
        cell(Redundancy::default()),
        cell(on(Some(0))),
        cell(on(Some(2))),
    );
    let asked = |on: bool, delay_blocks| RedundancyAsked { on, delay_blocks };
    assert!(redundancy_choice_settled(&asked(true, None), &immediate));
    assert!(!redundancy_choice_settled(&asked(true, None), &off));
    assert!(
        !redundancy_choice_settled(&asked(true, Some(2)), &immediate),
        "on, but not with the delay asked yet"
    );
    assert!(redundancy_choice_settled(&asked(true, Some(2)), &two));
    assert!(redundancy_choice_settled(&asked(false, None), &off));
}

#[test]
fn every_table_row_carries_its_redundancy_cell_priced_in_the_engines_block() {
    let mut report = sample_status();
    report.status.capture_format.buffer_size = "Fixed(128)".to_owned();
    report.musicians[1].redundancy = on(Some(3));
    report.musicians[1].recovered = 2;
    let rows = musician_rows(&with(report));
    let cell = &rows[1].redundancy;
    assert!(cell.on);
    assert_eq!(cell.delay_blocks, Some(3));
    assert_eq!(cell.key, BASS);
    assert_eq!(cell.delays[2].label, "Copy: 2 blocks later (+5.3 ms)");
    assert_eq!(
        cell.bandwidth,
        "2\u{d7} bandwidth, +8.0 ms latency on this phone"
    );
}

#[test]
fn numbers_are_grouped_by_thousands_as_the_web_console_wrote_them() {
    assert_eq!(count(0), "0");
    assert_eq!(count(999), "999");
    assert_eq!(count(1200), "1,200");
    assert_eq!(count(1_234_567), "1,234,567");
}
