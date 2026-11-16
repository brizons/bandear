//! `DeskReportsTests.swift`: both of its suites. `DeskReportsParityTests` ports the Android
//! client's tests of what its control channel hands on from the desk's mix reports
//! (`ControlChannelTest.kt`), with Android's own JSON: each message is decoded and mapped to the
//! event it means for the mix; what the channel also does with an ack (counting it, marking the
//! mix held by the desk) is the connection's. `DeskReportsTests` pins what each message means for
//! the mix, and the engineer's controls a report carries.

use super::controls;
use crate::control::server_message::{DeskConfiguration, DeskError, MixReport, ServerMessage};
use crate::cushion::PlaybackBufferSetting;
use crate::mix::{EngineerControls, MixEvent};
use serde_json::{json, Value};

/// A mix report as the desk writes it: its full 32-channel and 16-group tables, and `seq` if any.
/// Android's `mixReport` helper in `ControlChannelTest.kt:47-59`.
fn mix_report(message_type: &str, first_level: i64, seq: Option<i64>) -> Value {
    let mut object = json!({
        "type": message_type,
        "volume_percent": 80,
        "max_level_percent": 90,
        "muted": true,
        "channels": (0..32).map(|c| first_level + c).collect::<Vec<i64>>(),
        "pans": (0..32).map(|c| if c == 1 { 25 } else if c % 2 == 0 { 0 } else { 100 }).collect::<Vec<i64>>(),
        "mutes": (0..32).map(|c| c == 2).collect::<Vec<bool>>(),
        "solos": (0..32).map(|c| c == 3).collect::<Vec<bool>>(),
        "group_levels": (0..16).map(|g| 50 + g).collect::<Vec<i64>>(),
    });
    if let Some(seq) = seq {
        object["seq"] = json!(seq);
    }
    object
}

/// `object` with `key` set to `value`, or without `key` when `value` is `None`, which is what
/// org.json's `put` does with a Kotlin null. `Value::Null` stands for Android's `JSONObject.NULL`.
fn putting(key: &str, value: Option<Value>, mut object: Value) -> Value {
    let fields = object.as_object_mut().expect("a JSON object");
    match value {
        Some(value) => fields.insert(key.to_owned(), value),
        None => fields.remove(key),
    };
    object
}

/// Android's `withEngineer` helper: `report` carrying the engineer's `limit` and `locked`.
fn with_engineer(report: Value, limit: Option<Value>, locked: Option<Value>) -> Value {
    putting(
        "locked",
        locked,
        putting("engineer_limit_percent", limit, report),
    )
}

/// What the desk's `messages` mean for the mix, in order: what Android's control channel hands
/// its listener as `onMixPushed` and `onMixAnswered`.
fn mix_events(messages: &[Value]) -> Vec<MixEvent> {
    messages
        .iter()
        .filter_map(|object| MixEvent::reported_by(&message(&object.to_string())))
        .collect()
}

fn message(json: &str) -> ServerMessage {
    ServerMessage::parse(json).expect("the desk's message reads")
}

/// The engineer's controls `events` carry, in order: Android's `RecordingListener.engineer`, which
/// records a push's and a readable answer's, never a refusal's.
fn engineer(events: &[MixEvent]) -> Vec<EngineerControls> {
    events
        .iter()
        .filter_map(|event| match event {
            MixEvent::MixPushed(report) => Some(report.engineer_controls()),
            MixEvent::MixAnswered { report, .. } => {
                report.as_ref().map(MixReport::engineer_controls)
            }
            _ => None,
        })
        .collect()
}

/// A one-channel mix as the desk reports it, numbered `sequence`.
fn one_channel(sequence: Option<u64>) -> MixReport {
    MixReport {
        sequence,
        volume_percent: 80,
        max_level_percent: 90,
        muted: false,
        channel_gains: vec![70],
        pans: vec![50],
        channel_mutes: vec![false],
        channel_solos: vec![false],
        group_levels: Vec::new(),
        engineer_limit_percent: 60,
        locked: vec![false, true, false, true],
        following: Some("Ana".to_owned()),
    }
}

// DeskReportsParityTests

#[test]
fn a_push_and_an_ack_carry_the_engineers_limit_and_locks() {
    let events = mix_events(&[
        with_engineer(
            mix_report("mix_state", 10, None),
            Some(json!(60)),
            Some(json!((0..32).map(|c| c == 0 || c == 3).collect::<Vec<_>>())),
        ),
        with_engineer(
            mix_report("mix_ack", 10, Some(12)),
            Some(json!(75)),
            Some(json!((0..32).map(|c| c == 1).collect::<Vec<_>>())),
        ),
    ]);

    assert_eq!(
        engineer(&events),
        [controls(60, &[0, 3], None), controls(75, &[1], None)]
    );
    assert_eq!(events.len(), 2);
    let MixEvent::MixPushed(pushed) = &events[0] else {
        panic!("expected a push first, got {:?}", events[0]);
    };
    assert_eq!(pushed.channel_gains.first(), Some(&10));
}

#[test]
fn a_report_from_an_older_server_has_no_limit_and_nothing_locked() {
    let events = mix_events(&[mix_report("mix_state", 10, None)]);

    assert_eq!(engineer(&events), [EngineerControls::default()]);
    assert!(!EngineerControls::default().is_limited());
}

#[test]
fn a_report_says_whose_mix_the_phone_follows() {
    let events = mix_events(&[
        putting(
            "following",
            Some(json!("Ana")),
            mix_report("mix_state", 10, None),
        ),
        putting(
            "following",
            Some(Value::Null),
            mix_report("mix_ack", 10, Some(12)),
        ),
    ]);

    assert_eq!(
        engineer(&events)
            .into_iter()
            .map(|controls| controls.following)
            .collect::<Vec<_>>(),
        [Some("Ana".to_owned()), None]
    );
}

/// Android also counts the ack as acknowledged and marks the mix held by the desk; that is the
/// connection's.
#[test]
fn a_numbered_ack_answers_its_mix_with_the_mix_the_server_plays() {
    let events = mix_events(&[mix_report("mix_ack", 40, Some(12))]);

    assert_eq!(events.len(), 1);
    let MixEvent::MixAnswered { sequence, report } = &events[0] else {
        panic!("expected an answer, got {:?}", events[0]);
    };
    assert_eq!(*sequence, 12);
    assert_eq!(
        report
            .as_ref()
            .and_then(|mix| mix.channel_gains.first().copied()),
        Some(40)
    );
}

/// Android also counts it as acknowledged; that is the connection's. Here the ack still decodes,
/// so there is something to count.
#[test]
fn an_ack_without_a_number_only_acknowledges() {
    let ack = message(&mix_report("mix_ack", 40, None).to_string());

    assert_eq!(MixEvent::reported_by(&ack), None);
    let ServerMessage::MixAck(report) = ack else {
        panic!("expected a mix ack, got {ack:?}");
    };
    assert_eq!(report.sequence, None);
}

#[test]
fn a_numbered_error_answers_its_mix_with_nothing_to_show() {
    let error = message(r#"{"type":"error","message":"too many channels","seq":12}"#);

    assert_eq!(
        MixEvent::reported_by(&error),
        Some(MixEvent::MixAnswered {
            sequence: 12,
            report: None
        })
    );
    assert_eq!(
        error,
        ServerMessage::Error(DeskError {
            message: "too many channels".to_owned(),
            sequence: Some(12)
        })
    );
}

#[test]
fn an_error_without_a_number_never_touches_the_mixes() {
    let error = message(r#"{"type":"error","message":"no UDP target configured for client IP"}"#);

    assert_eq!(MixEvent::reported_by(&error), None);
    assert_eq!(
        error,
        ServerMessage::Error(DeskError {
            message: "no UDP target configured for client IP".to_owned(),
            sequence: None
        })
    );
}

/// Android also counts it as acknowledged; no error is the decode not failing.
#[test]
fn a_numbered_ack_whose_mix_cannot_be_read_still_answers_its_number() {
    let ack = message(r#"{"type":"mix_ack","seq":12,"channels":[100]}"#);

    assert_eq!(
        MixEvent::reported_by(&ack),
        Some(MixEvent::MixAnswered {
            sequence: 12,
            report: None
        })
    );
}

// DeskReportsTests

#[test]
fn hands_a_pushed_mix_on_as_a_push_whatever_it_carries() {
    assert_eq!(
        MixEvent::reported_by(&ServerMessage::MixState(one_channel(None))),
        Some(MixEvent::MixPushed(one_channel(None)))
    );
}

#[test]
fn hands_a_numbered_ack_on_as_the_answer_to_its_number_with_its_mix() {
    assert_eq!(
        MixEvent::reported_by(&ServerMessage::MixAck(one_channel(Some(3)))),
        Some(MixEvent::MixAnswered {
            sequence: 3,
            report: Some(one_channel(Some(3)))
        })
    );
}

#[test]
fn answers_the_number_of_an_ack_it_could_not_read_and_of_an_error_with_nothing_to_show() {
    assert_eq!(
        MixEvent::reported_by(&ServerMessage::UnreadableMixAck { sequence: 4 }),
        Some(MixEvent::MixAnswered {
            sequence: 4,
            report: None
        })
    );
    assert_eq!(
        MixEvent::reported_by(&ServerMessage::Error(DeskError {
            message: "locked".to_owned(),
            sequence: Some(5)
        })),
        Some(MixEvent::MixAnswered {
            sequence: 5,
            report: None
        })
    );
}

/// Swift's parameterized test, its six arguments in one loop.
#[test]
fn means_nothing_for_the_mix_from_an_unnumbered_ack_or_error_or_any_other_message() {
    let messages = [
        ServerMessage::MixAck(one_channel(None)),
        ServerMessage::Error(DeskError {
            message: "no UDP target".to_owned(),
            sequence: None,
        }),
        ServerMessage::Configuration(DeskConfiguration {
            source_channels: 2,
            sample_rate: None,
            groups: Vec::new(),
            channels: None,
        }),
        ServerMessage::PlaybackBuffer(PlaybackBufferSetting::Auto),
        ServerMessage::RedundancyHold { blocks: 2 },
        ServerMessage::Unrecognized {
            message_type: "mix_preview".to_owned(),
        },
    ];

    for message in messages {
        assert_eq!(MixEvent::reported_by(&message), None, "{message:?}");
    }
}

#[test]
fn reads_the_engineers_controls_off_a_report_its_limit_the_channels_it_locks_whom_it_follows() {
    assert_eq!(
        one_channel(None).engineer_controls(),
        controls(60, &[1, 3], Some("Ana"))
    );
}

#[test]
fn says_the_engineer_limited_the_musician_only_below_full_scale() {
    assert!(controls(60, &[], None).is_limited());
    assert!(controls(99, &[], None).is_limited());
    assert!(!controls(100, &[], None).is_limited());
}
