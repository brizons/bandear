//! What the engine sends the console, as the console reads it: the `status`, `mix-changed` and
//! `meters` events and the `engine_status` answer.
//!
//! The desk's own types ([bandear_desktop_core::dto]) only serialize, so the console reads the same
//! JSON the web console read into these mirrors. Every field has a default, so a payload from an
//! older desk, or a test's partial one, still reads; the defaults are what the web console assumed
//! for a missing field (an idle recorder, the live inputs, no limit from the engineer). Names
//! follow the desk's JSON (camelCase), field for field.

use std::collections::BTreeMap;

use personal_monitoring::{Effect, EffectLevels, EffectSettings, LimiterSettings};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::mix::Mix;

/// The engine's capture format: the device's channels, its rate, its sample format (`I24`,
/// `F32`, ...) and its buffer (`Fixed(256)` or `device default`).
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CaptureFormat {
    pub channels: u16,
    pub sample_rate: u32,
    pub sample_format: String,
    pub buffer_size: String,
}

/// A group as a status lists it: its inputs numbered from 0, and `invalid` when it names an
/// input the device lacks.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Group {
    pub id: String,
    pub name: String,
    pub channels: Vec<usize>,
    pub invalid: bool,
}

/// An input of the running device as a status lists it.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Channel {
    /// 1-based.
    pub number: u8,
    pub name: String,
    pub kind: String,
    pub hidden: bool,
    /// Linked to the next input as a stereo pair.
    pub link: bool,
    pub group: Option<String>,
    pub group_id: Option<String>,
    pub level: u8,
}

/// The newest link report of a phone (`linkLatest`).
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LinkSample {
    pub seq: u64,
    pub mono_ms: u64,
    pub at_ms: u64,
    pub gap: bool,
    pub reconnected: bool,
    pub health: String,
    pub interval_ms: Option<u32>,
    pub packets_received: Option<u64>,
    pub packets_lost: Option<u64>,
    pub loss_percent: Option<f64>,
    pub underruns: Option<u64>,
    pub receive_timeouts: Option<u64>,
    pub rtt_ms: Option<f64>,
    pub jitter_ms: Option<f64>,
    pub cushion_ms: Option<f64>,
    pub backlog_ms: Option<f64>,
    pub recovered: Option<u64>,
}

/// Whose mix a musician's mix follows.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Mirror {
    pub leader: String,
    pub leader_address: Option<String>,
    pub name: String,
}

/// A phone's playback buffer: `auto`, or `fixed` with its length.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Buffer {
    pub mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ms: Option<u16>,
}

impl Default for Buffer {
    fn default() -> Self {
        Self {
            mode: "auto".to_owned(),
            ms: None,
        }
    }
}

/// A musician's packet redundancy.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Redundancy {
    pub on: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay_blocks: Option<u8>,
}

/// A musician as the console keeps them: the status row, with its mix nested as `mix` (a row
/// carries it flat beside the other fields).
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Musician {
    pub address: String,
    pub client_name: Option<String>,
    pub client_id: Option<String>,
    pub name: Option<String>,
    pub name_is_local: bool,
    pub control_connected: bool,
    #[serde(flatten)]
    pub mix: Mix,
    pub packets_sent: u64,
    pub packets_discarded: u64,
    pub last_send_error: Option<String>,
    /// `good`, `warning`, `bad` or `unknown` (see [crate::format::link_state]).
    pub link_health: String,
    pub packets_per_second: Option<f64>,
    pub underruns_per_second: Option<f64>,
    pub loss_percent: Option<f64>,
    pub receive_timeouts_per_second: Option<f64>,
    pub capabilities: Vec<String>,
    pub rtt_ms: Option<f64>,
    pub rtt_last_ms: Option<f64>,
    pub link_latest: Option<LinkSample>,
    pub mirror: Option<Mirror>,
    pub buffer: Buffer,
    pub buffer_supported: bool,
    pub redundancy: Redundancy,
    pub duplicates_sent: u64,
    pub recovered: u64,
    pub redundancy_supported: bool,
    pub redundancy_hold_supported: bool,
    pub limiter: LimiterSettings,
    pub fx_returns: EffectLevels,
}

/// Where the local mix plays.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LocalOutput {
    pub device: Option<String>,
    pub first_channel: Option<u16>,
    pub state: String,
    pub reason: Option<String>,
    pub underrun_frames: u64,
    pub dropped_frames: u64,
    pub notice: Option<String>,
}

/// A marker in a take.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TakeMarker {
    pub frame: u64,
    pub seconds: f64,
    pub name: String,
}

/// The recorder: `idle`, `recording`, `stopping` or `failed`, and the take it writes.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Recording {
    pub state: String,
    pub reason: Option<String>,
    pub take_folder: Option<String>,
    pub frames: u64,
    pub seconds: f64,
    pub dropped_frames: u64,
    pub bytes_written: u64,
    pub tracks: Vec<String>,
    pub markers: Vec<TakeMarker>,
    pub free_bytes: Option<u64>,
    pub minutes_left: Option<u64>,
}

impl Default for Recording {
    /// The idle recorder (`IDLE_RECORDING`): what a status without one means.
    fn default() -> Self {
        Self {
            state: "idle".to_owned(),
            reason: None,
            take_folder: None,
            frames: 0,
            seconds: 0.0,
            dropped_frames: 0,
            bytes_written: 0,
            tracks: Vec::new(),
            markers: Vec::new(),
            free_bytes: None,
            minutes_left: None,
        }
    }
}

impl Recording {
    /// True while a take records or its files are being finished: the input cannot change.
    pub fn is_recording(&self) -> bool {
        self.state == "recording" || self.state == "stopping"
    }
}

/// A track of a finished take.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TakeTrack {
    pub file: String,
    pub kind: String,
    pub channels: Vec<u16>,
    pub name: String,
    pub instrument: Option<String>,
    pub frames: u64,
}

/// A take that ended while the engine ran.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TakeSummary {
    pub folder: String,
    pub name: String,
    pub started: String,
    pub sample_rate: u32,
    pub format: String,
    pub device: String,
    pub tracks: Vec<TakeTrack>,
    pub frames: u64,
    pub seconds: f64,
    pub markers: Vec<TakeMarker>,
    pub dropped_frames: u64,
    pub ended: String,
    pub reason: Option<String>,
}

/// The virtual soundcheck: `off` (the live inputs), or a take playing in their place.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Soundcheck {
    pub state: String,
    pub reason: Option<String>,
    pub take_folder: Option<String>,
    pub take_name: Option<String>,
    pub position_frames: u64,
    pub position_seconds: f64,
    pub length_frames: u64,
    pub length_seconds: f64,
    pub looping: bool,
    pub underrun_frames: u64,
    pub clock: String,
}

impl Default for Soundcheck {
    /// The live inputs (`IDLE_SOUNDCHECK`): what a status without a soundcheck means.
    fn default() -> Self {
        Self {
            state: "off".to_owned(),
            reason: None,
            take_folder: None,
            take_name: None,
            position_frames: 0,
            position_seconds: 0.0,
            length_frames: 0,
            length_seconds: 0.0,
            looping: false,
            underrun_frames: 0,
            clock: "timer".to_owned(),
        }
    }
}

impl Soundcheck {
    /// True while a take plays in place of the inputs (playing, paused or failed).
    pub fn is_soundchecking(&self) -> bool {
        self.state != "off"
    }

    /// The soundcheck the desk answered a command with, as a status carries it.
    pub fn from_core(dto: &bandear_desktop_core::dto::SoundcheckDto) -> Self {
        Self {
            state: dto.state.to_owned(),
            reason: dto.reason.clone(),
            take_folder: dto.take_folder.clone(),
            take_name: dto.take_name.clone(),
            position_frames: dto.position_frames,
            position_seconds: dto.position_seconds,
            length_frames: dto.length_frames,
            length_seconds: dto.length_seconds,
            looping: dto.looping,
            underrun_frames: dto.underrun_frames,
            clock: dto.clock.to_owned(),
        }
    }
}

/// How hard the engine thread works.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct EngineLoad {
    pub percent: f32,
    pub average_percent: f32,
    pub peak_percent: f32,
    pub late_blocks: u64,
}

/// The engine as a status reports it, without its musicians (the console keeps those beside it,
/// see [crate::state::ConsoleState::musicians]).
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Status {
    /// The engine run the status came from; a payload without one is never trusted.
    pub run: Option<u64>,
    pub device_name: String,
    pub capture_format: CaptureFormat,
    pub control_port: u16,
    pub sample_rate: u32,
    pub source_channels: usize,
    pub groups: Vec<Group>,
    pub channels: Vec<Channel>,
    pub stopped: bool,
    pub samples_received: u64,
    pub packets_sent: u64,
    pub packets_discarded: u64,
    /// The engineer's local mix; `None` from a desk before the local mix.
    pub local_mix: Option<Mix>,
    pub local_output: Option<LocalOutput>,
    pub recording: Recording,
    pub last_take: Option<TakeSummary>,
    pub soundcheck: Soundcheck,
    /// True while the engine plays the demo source in place of an interface.
    pub demo: bool,
    pub load: EngineLoad,
    pub local_limiter: LimiterSettings,
    pub local_fx_returns: EffectLevels,
    pub effects: EffectSettings,
}

/// A `status` event or a running `engine_status` answer: the engine and its musicians.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct StatusReport {
    #[serde(flatten)]
    pub status: Status,
    pub musicians: Vec<Musician>,
}

/// The `engine_status` answer: no engine runs (an answer, not an error), or its status.
#[derive(Clone, Debug, PartialEq)]
pub enum EngineState {
    /// No engine runs; `run` is the last run begun (0 before the first start).
    NotRunning {
        run: u64,
    },
    Running(Box<StatusReport>),
}

impl EngineState {
    /// The answer in the desk's JSON (`{ "state": "notRunning", "run": 3 }` or a status with
    /// `"state": "running"`).
    pub fn from_json(value: serde_json::Value) -> Result<Self, String> {
        let state = value
            .get("state")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        match state.as_deref() {
            Some("notRunning") => Ok(Self::NotRunning {
                run: value
                    .get("run")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0),
            }),
            Some("running") => {
                read::<StatusReport>(value).map(|report| Self::Running(Box::new(report)))
            }
            other => Err(format!(
                "an engine state the console does not know: {other:?}"
            )),
        }
    }
}

/// One output's levels: left, right (0-100, linear) and the limiter's gain reduction in dB.
#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct OutputLevels {
    pub left: u8,
    pub right: u8,
    #[serde(rename = "limiterReductionDb")]
    pub limiter_reduction_db: f32,
}

/// A `meters` event: every input's level before and after its processing, the dynamics'
/// reductions, each musician's output and the local mix's.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Meters {
    pub run: Option<u64>,
    pub channels: Vec<u8>,
    pub processed: Vec<u8>,
    pub gate_reduction_db: Vec<f32>,
    pub compressor_reduction_db: Vec<f32>,
    /// By musician address.
    pub musicians: BTreeMap<String, OutputLevels>,
    pub local: OutputLevels,
}

/// A `mix-changed` event: a musician's mix (`address`) or the local mix (`id` "local") changed,
/// from a phone or from this desk.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MixChanged {
    pub run: Option<u64>,
    pub address: Option<String>,
    pub id: Option<String>,
    /// `phone` or `desktop`.
    pub origin: String,
    pub mix: Mix,
}

/// An event of the engine's pump, read from its JSON.
#[derive(Clone, Debug, PartialEq)]
pub enum EngineEvent {
    Status(Box<StatusReport>),
    MixChanged(Box<MixChanged>),
    Meters(Box<Meters>),
}

impl EngineEvent {
    /// The event `name` (`status`, `mix-changed` or `meters`) with its payload; `Ok(None)` for an
    /// event the console does not listen to.
    pub fn from_json(name: &str, payload: serde_json::Value) -> Result<Option<Self>, String> {
        Ok(Some(match name {
            "status" => Self::Status(Box::new(read(payload)?)),
            "mix-changed" => Self::MixChanged(Box::new(read(payload)?)),
            "meters" => Self::Meters(Box::new(read(payload)?)),
            _ => return Ok(None),
        }))
    }
}

/// Reads `value` as `T`, saying why when it cannot.
pub fn read<T: DeserializeOwned>(value: serde_json::Value) -> Result<T, String> {
    serde_json::from_value(value)
        .map_err(|error| format!("the desk's answer could not be read: {error}"))
}

/// One of the desk's own values (`dto`) read as the console reads it: the same JSON, the console's
/// type.
pub fn mirror<T: Serialize, U: DeserializeOwned>(value: &T) -> Result<U, String> {
    serde_json::to_value(value)
        .map_err(|error| format!("the desk's answer could not be read: {error}"))
        .and_then(read)
}

/// The name the desk gives `effect` (`reverb` or `delay`).
pub fn effect_name(effect: Effect) -> &'static str {
    match effect {
        Effect::Reverb => "reverb",
        Effect::Delay => "delay",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_status_row_carries_its_mix_flat_and_the_console_keeps_it_nested() {
        let report: StatusReport = read(json!({
            "run": 2,
            "deviceName": "Volt 4",
            "musicians": [{ "address": "192.168.1.3:50000", "revision": 4, "volumePercent": 70 }],
        }))
        .expect("status");
        assert_eq!(report.status.run, Some(2));
        assert_eq!(report.musicians[0].mix.revision, 4);
        assert_eq!(report.musicians[0].mix.volume_percent, 70);
        assert_eq!(
            report.status.recording.state, "idle",
            "no recorder is the idle one"
        );
        assert_eq!(
            report.status.soundcheck.state, "off",
            "no soundcheck is the live inputs"
        );
    }

    #[test]
    fn the_engine_state_names_whether_an_engine_runs() {
        assert_eq!(
            EngineState::from_json(json!({ "state": "notRunning", "run": 3 })),
            Ok(EngineState::NotRunning { run: 3 })
        );
        let running =
            EngineState::from_json(json!({ "state": "running", "run": 4, "musicians": [] }));
        assert!(
            matches!(running, Ok(EngineState::Running(report)) if report.status.run == Some(4))
        );
        assert!(EngineState::from_json(json!({ "state": "dancing" })).is_err());
    }

    #[test]
    fn events_read_their_payloads_and_others_are_not_heard() {
        let meters = EngineEvent::from_json(
            "meters",
            json!({ "run": 1, "channels": [50], "musicians": { "a": { "left": 1, "right": 2, "limiterReductionDb": -3.0 } } }),
        );
        assert!(
            matches!(meters, Ok(Some(EngineEvent::Meters(meters))) if meters.musicians["a"].limiter_reduction_db == -3.0)
        );
        let local = EngineEvent::from_json(
            "mix-changed",
            json!({ "run": 1, "id": "local", "origin": "desktop", "mix": { "revision": 2 } }),
        );
        assert!(
            matches!(local, Ok(Some(EngineEvent::MixChanged(change))) if change.id.as_deref() == Some("local"))
        );
        assert_eq!(EngineEvent::from_json("other", json!({})), Ok(None));
    }
}
