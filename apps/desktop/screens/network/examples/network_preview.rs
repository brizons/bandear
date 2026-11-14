//! Opens the screen alone over the simulated desk, with its engine running on the sample
//! interface and musicians: no engine starts, no input opens and nothing goes on the network. The
//! desk answers what this screen asks as the desktop would: a minute of link history for every
//! phone, two outputs for the local mix, two saved sessions and two mix backups.
//! `NETWORK_PREVIEW=stopped` opens it with no engine running.

use bandear_desktop_console::command::{Answer, Command};
use bandear_desktop_console::testing::FakeDesk;
use bandear_desktop_console::types::dto::{
    LinkHistoryDto, LinkSampleDto, LocalOutputSetDto, OutputDeviceDto,
};
use bandear_desktop_console::types::{MixBackupSummary, SnapshotSummary};
use bandear_desktop_console::wire::LocalOutput;

/// 2026-11-13 21:30:00 UTC.
const NOW: u64 = 1_794_605_400_000;

fn sample(seq: u64) -> LinkSampleDto {
    // A link that is mostly clean, loses a little every few seconds and went quiet once.
    let gap = (40..44).contains(&seq);
    let loss = if seq % 7 == 0 { 1.5 } else { 0.2 };
    LinkSampleDto {
        seq,
        mono_ms: seq * 1000,
        at_ms: NOW - 120_000 + seq * 1000,
        gap,
        reconnected: seq == 44,
        health: if gap {
            "unknown"
        } else if loss > 1.0 {
            "warning"
        } else {
            "good"
        },
        interval_ms: (!gap).then_some(1000),
        packets_received: (!gap).then_some(198),
        packets_lost: (!gap).then_some(if seq % 7 == 0 { 3 } else { 0 }),
        loss_percent: (!gap).then_some(loss),
        underruns: (!gap).then_some(u64::from(seq % 23 == 0)),
        receive_timeouts: (!gap).then_some(0),
        rtt_ms: Some(8.0 + (seq % 5) as f64),
        jitter_ms: (!gap).then_some(1.5 + (seq % 3) as f64),
        cushion_ms: (!gap).then_some(40.0),
        backlog_ms: None,
        recovered: None,
    }
}

fn main() {
    // `NETWORK_PREVIEW=stopped` shows the screen with no engine running.
    let stopped = std::env::var("NETWORK_PREVIEW").is_ok_and(|look| look == "stopped");
    let mut desk = if stopped {
        FakeDesk::stopped()
    } else {
        FakeDesk::running()
    };
    desk.script("link_history", |_, command| {
        let Command::LinkHistory {
            musician_key,
            since_seq,
        } = command
        else {
            return Err("not a history read".to_owned());
        };
        let from = since_seq.map_or(1, |since| since + 1);
        Ok(Answer::LinkHistory(LinkHistoryDto {
            musician_key: musician_key.clone(),
            history_id: 1,
            samples: (from..=120).map(sample).collect(),
            max_samples: 600,
            period_ms: 1000,
        }))
    });
    let outputs = || {
        vec![
            OutputDeviceDto {
                name: "Demo interface".to_owned(),
                channels: 4,
            },
            OutputDeviceDto {
                name: "Speakers (Realtek Audio)".to_owned(),
                channels: 2,
            },
        ]
    };
    desk.script("output_devices", move |_, _| {
        Ok(Answer::OutputDevices(outputs()))
    });
    desk.script("set_local_output", |desk, command| {
        let Command::SetLocalOutput { output } = command else {
            return Err("not an output".to_owned());
        };
        desk.report.status.local_output = Some(LocalOutput {
            device: output.as_ref().map(|output| output.device.clone()),
            first_channel: output.as_ref().map(|output| output.first_channel),
            state: if output.is_some() { "running" } else { "off" }.to_owned(),
            ..LocalOutput::default()
        });
        desk.emit_status();
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
    desk.script("sessions", |_, _| {
        Ok(Answer::Sessions(vec![
            SnapshotSummary {
                id: "sunday".to_owned(),
                name: "Sunday service".to_owned(),
                saved_at: Some(NOW - 86_400_000),
                mixes: 4,
                problem: None,
            },
            SnapshotSummary {
                id: "rehearsal".to_owned(),
                name: "Rehearsal".to_owned(),
                saved_at: Some(NOW - 3 * 86_400_000),
                mixes: 3,
                problem: None,
            },
        ]))
    });
    desk.script("list_mix_backups", |_, _| {
        Ok(Answer::MixBackups(vec![
            MixBackupSummary {
                stamp: "20261113T212700000Z".to_owned(),
                saved_at: Some(NOW - 180_000),
                musicians: 3,
                names: vec![
                    "Lead vocal".to_owned(),
                    "Bass".to_owned(),
                    "Keys".to_owned(),
                ],
                problem: None,
            },
            MixBackupSummary {
                stamp: "20261113T201500000Z".to_owned(),
                saved_at: None,
                musicians: 0,
                names: Vec::new(),
                problem: Some("it is damaged and cannot be read".to_owned()),
            },
        ]))
    });
    bandear_desktop_preview::run::<bandear_desktop_network::view::NetworkScreen>(desk);
}
