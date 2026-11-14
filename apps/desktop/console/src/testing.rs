//! What tests drive the console with: the sample engine's status and helpers to build payloads.
//! On with the `testing` feature (and in this crate's own tests), never in a build.

use serde_json::{Value, json};

use crate::mix::Mix;
use crate::wire::StatusReport;

/// `base` with every field of `extra` set on it (both JSON objects).
pub fn with_fields(base: Value, extra: Value) -> Value {
    let mut base = base;
    if let (Some(base), Some(extra)) = (base.as_object_mut(), extra.as_object()) {
        for (key, value) in extra {
            base.insert(key.clone(), value.clone());
        }
    }
    base
}

/// A musician's row as the sample engine lists it: `identity` and `link` on top of the defaults,
/// and its mix the engine's default with `mix` changes.
fn musician_row(address: &str, identity: Value, link: Value, mix: Value) -> Value {
    let capabilities: Vec<String> = identity
        .get("capabilities")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let has = |capability: &str| capabilities.iter().any(|listed| listed == capability);
    let mut row = json!({
        "address": address,
        "clientName": null,
        "clientId": null,
        "name": null,
        "nameIsLocal": false,
        "capabilities": [],
        "rttMs": null,
        "rttLastMs": null,
        "linkLatest": null,
        "buffer": { "mode": "auto" },
        "redundancy": { "on": false },
        "duplicatesSent": 0,
        "limiter": { "enabled": false, "ceilingDb": 0.0, "releaseMs": 50.0 },
        "fxReturns": { "reverb": 0, "delay": 0 },
        "recovered": 0,
    });
    row = with_fields(row, identity);
    row = with_fields(
        row,
        json!({
            "bufferSupported": has("playback.buffer"),
            "redundancySupported": has("redundancy.recovered"),
            "redundancyHoldSupported": has("redundancy.hold"),
        }),
    );
    let default = serde_json::to_value(Mix::engine_default()).expect("the default mix");
    let mut mixed = with_fields(default, json!({ "revision": 1 }));
    // A table change replaces the table from its first entry and keeps the rest, as the engine
    // merges a mix update.
    if let Some(changes) = mix.as_object() {
        for (field, value) in changes {
            match (mixed.get_mut(field), value.as_array()) {
                (Some(Value::Array(table)), Some(entries)) => {
                    for (index, entry) in entries.iter().enumerate() {
                        table[index] = entry.clone();
                    }
                }
                _ => {
                    mixed[field] = value.clone();
                }
            }
        }
    }
    row = with_fields(row, mixed);
    row = with_fields(row, json!({ "packetsSent": 0, "packetsDiscarded": 0 }));
    row = with_fields(row, link);
    with_fields(row, json!({ "mirror": null }))
}

fn good() -> Value {
    json!({
        "controlConnected": true,
        "linkHealth": "good",
        "packetsPerSecond": 200,
        "underrunsPerSecond": 0,
        "lossPercent": 0.01,
        "receiveTimeoutsPerSecond": 0,
    })
}

fn warning() -> Value {
    json!({
        "controlConnected": true,
        "linkHealth": "warning",
        "packetsPerSecond": 196,
        "underrunsPerSecond": 2,
        "lossPercent": 0.82,
        "receiveTimeoutsPerSecond": 0.5,
    })
}

fn lost() -> Value {
    json!({
        "controlConnected": true,
        "linkHealth": "bad",
        "packetsPerSecond": null,
        "underrunsPerSecond": null,
        "lossPercent": null,
        "receiveTimeoutsPerSecond": null,
    })
}

/// The sample engine's status as JSON (the web console's `sampleStatus`): six inputs on "Demo
/// interface", two groups, four phones (Lead vocal, Bass, Keys, and Drums with its link lost) and
/// the local mix.
pub fn sample_status_json() -> Value {
    let names = [
        ("Kick", "drums"),
        ("Snare", "drums"),
        ("Overheads", "drums"),
        ("Bass", "bass"),
        ("Lead vox", "voice"),
        ("Backing vox", "voice"),
    ];
    let group_of = |index: usize| match index {
        0..=2 => (json!("Drums"), json!("drums")),
        4 | 5 => (json!("Vocals"), json!("vocals")),
        _ => (Value::Null, Value::Null),
    };
    let channels: Vec<Value> = names
        .iter()
        .enumerate()
        .map(|(index, (name, kind))| {
            let (group, group_id) = group_of(index);
            json!({
                "number": index + 1,
                "name": name,
                "kind": kind,
                "hidden": false,
                "link": false,
                "group": group,
                "groupId": group_id,
                "level": 0,
            })
        })
        .collect();
    let no_reports = with_fields(lost(), json!({ "linkHealth": "unknown" }));
    json!({
        "run": 1,
        "deviceName": "Demo interface",
        "captureFormat": { "channels": 6, "sampleRate": 48000, "sampleFormat": "I24", "bufferSize": "Fixed(256)" },
        "controlPort": 50001,
        "sampleRate": 48000,
        "sourceChannels": 6,
        "groups": [
            { "id": "drums", "name": "Drums", "channels": [0, 1, 2], "invalid": false },
            { "id": "vocals", "name": "Vocals", "channels": [4, 5], "invalid": false },
        ],
        "channels": channels,
        "stopped": false,
        "samplesReceived": 0,
        "packetsSent": 0,
        "packetsDiscarded": 0,
        "musicians": [
            musician_row(
                "192.168.1.3:50000",
                json!({
                    "clientName": "Lead vocal",
                    "clientId": "5f0c2a8e-1b7d-4c3a-9e6f-2d8b4a7c1e90",
                    "name": "Lead vocal",
                    "capabilities": ["stats.v2", "playback.buffer"],
                }),
                good(),
                json!({
                    "channelGains": [62, 55, 40, 58, 88, 52],
                    "pans": [50, 50, 50, 50, 50, 40],
                    "volumePercent": 85,
                    "maxLevelPercent": 90,
                }),
            ),
            musician_row(
                "192.168.1.12:50000",
                json!({
                    "clientName": "Bass",
                    "clientId": "a3d9e7b1-6c2f-4e8a-b5d0-7f1c9e3a2b64",
                    "name": "Bass",
                    "capabilities": ["stats.v2", "redundancy.recovered", "redundancy.hold"],
                }),
                warning(),
                json!({}),
            ),
            musician_row(
                "192.168.1.17:50000",
                json!({ "clientName": "Keys", "clientId": "c81e4f2a-9b3d-4a7e-8c6f-0d2b5e9a1f37", "name": "Keys" }),
                no_reports,
                json!({ "pans": [50, 50, 50, 50, 30, 70] }),
            ),
            musician_row(
                "192.168.1.20:50000",
                json!({ "clientId": "e4b7c1d9-2a6f-4d3e-9b8c-5f0a7e2d6c18", "name": "Drums", "nameIsLocal": true }),
                lost(),
                json!({ "channelMuted": [false, false, false, true] }),
            ),
        ],
        "localMix": with_fields(serde_json::to_value(Mix::engine_default()).expect("a mix"), json!({ "revision": 1 })),
        "localOutput": {
            "device": null,
            "firstChannel": null,
            "state": "off",
            "reason": null,
            "underrunFrames": 0,
            "droppedFrames": 0,
            "notice": null,
        },
        "lastTake": null,
        "demo": false,
        "load": { "percent": 9.4, "averagePercent": 8.2, "peakPercent": 14.6, "lateBlocks": 0 },
        "localLimiter": { "enabled": false, "ceilingDb": 0.0, "releaseMs": 50.0 },
        "localFxReturns": { "reverb": 0, "delay": 0 },
    })
}

/// The sample engine's status (see [sample_status_json]).
pub fn sample_status() -> StatusReport {
    serde_json::from_value(sample_status_json()).expect("the sample status reads")
}

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::rc::Rc;
use std::time::Duration;

use bandear_desktop_core::dto::{
    CaptureFormatDto, DiskEstimateDto, InputDeviceDto, RecordingSettingsDto, StartSummary,
};
use bandear_desktop_core::session::SessionAudio;

use crate::command::{Answer, Command, Reply, Ticket};
use crate::console::{Console, Effect, Options};
use crate::wire::{EngineEvent, EngineState, Meters, MixChanged};

/// The sample engine's interface.
pub const INTERFACE: &str = "Demo interface";
/// An input the fake's machine has besides the interface.
pub const USB: &str = "Microphone (USB audio)";
/// The fake's ASIO driver.
pub const ASIO_DRIVER: &str = "Demo ASIO driver";

/// A command the fake refuses, how many more times, and whether it did its work anyway.
struct Failure {
    command: &'static str,
    message: String,
    times: usize,
    after_effect: bool,
}

/// A simulated desk for tests: an engine that starts, stops and switches devices on the inputs
/// the fake's machine has, keeps the audio choices, and refuses what a test tells it to. Its
/// events wait in [FakeDesk::events] until the harness delivers them.
pub struct FakeDesk {
    pub running: bool,
    /// The last run begun.
    pub run: u64,
    /// The running engine's status, as the next read gives it.
    pub report: StatusReport,
    pub host: String,
    /// The audio choices the desk's session keeps.
    pub saved_audio: Option<SessionAudio>,
    /// True when a start sends a status at once, as the desk's pump does.
    pub status_on_start: bool,
    /// Why every start fails, until cleared.
    pub fail_start: Option<String>,
    /// Why listening to the engine fails, until cleared.
    pub listen_failure: Option<String>,
    pub events: VecDeque<EngineEvent>,
    failures: Vec<Failure>,
    scripts: HashMap<&'static str, Script>,
}

/// How a test answers one command in place of the fake desk: it gets the desk, so it can change
/// the status or send events as the real desk would while it answers.
pub type Script = Box<dyn FnMut(&mut FakeDesk, &Command) -> Reply>;

impl Default for FakeDesk {
    fn default() -> Self {
        Self::stopped()
    }
}

impl FakeDesk {
    /// A desk with no engine running yet.
    pub fn stopped() -> Self {
        let mut report = sample_status();
        report.status.run = Some(0);
        Self {
            running: false,
            run: 0,
            report,
            host: "auto".to_owned(),
            saved_audio: None,
            status_on_start: true,
            fail_start: None,
            listen_failure: None,
            events: VecDeque::new(),
            failures: Vec::new(),
            scripts: HashMap::new(),
        }
    }

    /// A desk whose engine runs, as run 1, on the sample interface.
    pub fn running() -> Self {
        let mut desk = Self::stopped();
        desk.running = true;
        desk.run = 1;
        desk.report.status.run = Some(1);
        desk
    }

    /// Refuses the next `times` calls of `command` with `message`.
    pub fn fail_next(&mut self, command: &'static str, message: &str, times: usize) {
        self.failures.push(Failure {
            command,
            message: message.to_owned(),
            times,
            after_effect: false,
        });
    }

    /// Answers the next `command` with `message` after doing its work anyway.
    pub fn fail_after_effect(&mut self, command: &'static str, message: &str) {
        self.failures.push(Failure {
            command,
            message: message.to_owned(),
            times: 1,
            after_effect: true,
        });
    }

    /// Answers every call of `command` with `script` from now on, in place of the desk's own
    /// answer, so a screen's tests can simulate the commands the fake desk does not know. A
    /// refusal from [FakeDesk::fail_next] still comes first.
    pub fn script(
        &mut self,
        command: &'static str,
        script: impl FnMut(&mut FakeDesk, &Command) -> Reply + 'static,
    ) {
        self.scripts.insert(command, Box::new(script));
    }

    /// The inputs the fake's machine offers under `host`, with their channels.
    pub fn devices(host: &str) -> Vec<(&'static str, u16)> {
        match host {
            "asio" => vec![(ASIO_DRIVER, 0), ("Volt ASIO", 0)],
            "demo" => vec![(
                crate::demo_source::DEMO_DEVICE,
                crate::demo_source::DEMO_CHANNELS,
            )],
            _ => vec![(INTERFACE, 6), (USB, 2), ("Digital input", 2)],
        }
    }

    /// The running engine's status now.
    pub fn status(&self) -> StatusReport {
        self.report.clone()
    }

    /// Sends the running engine's status, as its pump does.
    pub fn emit_status(&mut self) {
        self.events
            .push_back(EngineEvent::Status(Box::new(self.status())));
    }

    /// Sends a meters pass of the running engine: a level for each input.
    pub fn emit_meters(&mut self) {
        let channels = self.report.status.source_channels;
        self.events.push_back(EngineEvent::Meters(Box::new(Meters {
            run: Some(self.run),
            channels: vec![50; channels],
            ..Meters::default()
        })));
    }

    /// A phone changed its master volume: its mix is stored with a newer revision and announced.
    pub fn change_from_phone(&mut self, address: &str, volume_percent: u8) {
        let Some(musician) = self
            .report
            .musicians
            .iter_mut()
            .find(|musician| musician.address == address)
        else {
            return;
        };
        musician.mix.revision += 1;
        musician.mix.volume_percent = volume_percent;
        let mix = musician.mix.clone();
        self.events
            .push_back(EngineEvent::MixChanged(Box::new(MixChanged {
                run: Some(self.run),
                address: Some(address.to_owned()),
                id: None,
                origin: "phone".to_owned(),
                mix,
            })));
    }

    fn failure(&mut self, command: &str) -> Option<(String, bool)> {
        let index = self
            .failures
            .iter()
            .position(|failure| failure.command == command && failure.times > 0)?;
        let failure = &mut self.failures[index];
        failure.times -= 1;
        Some((failure.message.clone(), failure.after_effect))
    }

    fn start(&mut self, host: &str, device: Option<&str>, buffer: Option<u32>) -> Reply {
        if let Some(message) = &self.fail_start {
            return Err(message.clone());
        }
        let devices = Self::devices(host);
        let found = match device {
            None => devices.first().copied(),
            Some(sought) => devices
                .iter()
                .find(|(name, _)| name.to_lowercase().contains(&sought.to_lowercase()))
                .copied(),
        };
        let Some((name, channels)) = found else {
            return Err(format!(
                "No input device matches \"{}\".",
                device.unwrap_or("")
            ));
        };
        self.running = true;
        self.run += 1;
        self.host = host.to_owned();
        let status = &mut self.report.status;
        status.run = Some(self.run);
        status.stopped = false;
        status.device_name = name.to_owned();
        status.demo = host == "demo";
        if channels > 0 {
            status.capture_format.channels = channels;
        }
        status.capture_format.buffer_size = match buffer {
            Some(frames) => format!("Fixed({frames})"),
            None => "device default".to_owned(),
        };
        if self.status_on_start {
            self.emit_status();
        }
        Ok(Answer::Started(self.summary()))
    }

    fn summary(&self) -> StartSummary {
        let format = &self.report.status.capture_format;
        StartSummary {
            run: self.run,
            device_name: self.report.status.device_name.clone(),
            capture_format: CaptureFormatDto {
                channels: format.channels,
                sample_rate: format.sample_rate,
                sample_format: format.sample_format.clone(),
                buffer_size: format.buffer_size.clone(),
            },
        }
    }

    /// The desk's answer to `command`.
    pub fn answer(&mut self, command: &Command) -> Reply {
        let failure = self.failure(command.name());
        if let Some((message, false)) = &failure {
            return Err(message.clone());
        }
        // Out of the map while it runs, so it may use the desk; a script that set a new one for
        // its own command keeps the new one.
        if let Some(mut script) = self.scripts.remove(command.name()) {
            let reply = script(self, command);
            self.scripts.entry(command.name()).or_insert(script);
            return match failure {
                Some((message, true)) => Err(message),
                _ => reply,
            };
        }
        let answer = match command {
            Command::EngineStatus if self.running => Ok(Answer::EngineState(EngineState::Running(
                Box::new(self.status()),
            ))),
            Command::EngineStatus => Ok(Answer::EngineState(EngineState::NotRunning {
                run: self.run,
            })),
            Command::StartEngine(request) => self.start(
                request.host.as_deref().unwrap_or("auto"),
                request.device_filter.as_deref(),
                request.buffer_frames,
            ),
            Command::StopEngine => {
                self.running = false;
                Ok(Answer::Done)
            }
            Command::ListInputDevices { host } => Ok(Answer::InputDevices(
                Self::devices(host.as_deref().unwrap_or("auto"))
                    .into_iter()
                    .map(|(name, channels)| InputDeviceDto {
                        name: name.to_owned(),
                        channels,
                    })
                    .collect(),
            )),
            Command::SwitchDevice { device } => {
                if !self.running {
                    return Err("no engine is running".to_owned());
                }
                let found = Self::devices(&self.host)
                    .into_iter()
                    .find(|(name, _)| name == device);
                let Some((name, channels)) = found else {
                    return Err(format!("No input device matches \"{device}\"."));
                };
                self.report.status.device_name = name.to_owned();
                self.report.status.capture_format.channels = channels;
                self.emit_status();
                Ok(Answer::Started(self.summary()))
            }
            Command::AudioSettings => Ok(Answer::AudioSettings(self.saved_audio.clone())),
            Command::SetAudioSettings { settings } => {
                let kept = SessionAudio {
                    host: settings.host.clone(),
                    device: settings.device.clone(),
                    buffer_frames: settings.buffer_frames,
                    asio_driver: settings.asio_driver.clone(),
                };
                self.saved_audio = Some(kept.clone());
                Ok(Answer::AudioSaved(kept))
            }
            Command::Presets => Ok(Answer::Presets(Vec::new())),
            Command::Processing => Ok(Answer::Processing(Vec::new())),
            Command::StripPresets => Ok(Answer::StripPresets(Vec::new())),
            Command::Sessions => Ok(Answer::Sessions(Vec::new())),
            Command::SessionNotice => Ok(Answer::SessionNotice(None)),
            Command::ListMixBackups => Ok(Answer::MixBackups(Vec::new())),
            Command::RecordingSettings => Ok(Answer::RecordingSettings(RecordingSettingsDto {
                folder: "Recordings".to_owned(),
                default_folder: "Recordings".to_owned(),
                folder_is_default: true,
                format: "wav24",
                include_local: false,
                armed: Vec::new(),
                notice: None,
            })),
            Command::RecordEstimate => Ok(Answer::Estimate(DiskEstimateDto {
                folder: "Recordings".to_owned(),
                bytes_per_second: Some(864_000),
                free_bytes: Some(100_000_000_000),
                minutes_left: Some(1929),
                problem: None,
            })),
            other => Err(format!("the fake desk does not answer `{}`", other.name())),
        };
        match failure {
            Some((message, true)) => Err(message),
            _ => answer,
        }
    }
}

/// A console driven by a [FakeDesk]: every command it sends is answered at once (unless held),
/// timers wait until fired, and the desk's events are delivered after each answer.
pub struct Harness {
    pub console: Console,
    pub desk: FakeDesk,
    /// Every command the console sent, in order.
    pub calls: Vec<Command>,
    /// Every window title the console set, in order.
    pub titles: Vec<String>,
    /// How many times the console listened to the engine.
    pub listens: usize,
    timers: Vec<Ticket>,
    held: Vec<&'static str>,
    holding: Vec<(Ticket, Command)>,
}

impl Harness {
    /// A console with `options` over `desk`; nothing is sent until a flow runs.
    pub fn new(desk: FakeDesk, options: Options) -> Self {
        Self {
            console: Console::new(options),
            desk,
            calls: Vec::new(),
            titles: Vec::new(),
            listens: 0,
            timers: Vec::new(),
            held: Vec::new(),
            holding: Vec::new(),
        }
    }

    /// The console connected to `desk` (see [Console::connect]), every answer in.
    pub fn connected(desk: FakeDesk, options: Options) -> Self {
        let mut harness = Self::new(desk, options);
        let connect = harness.console.connect();
        harness.run(connect);
        harness
    }

    /// Runs `flow` until nothing more can happen without a timer or a held answer; its outcome,
    /// if it ended.
    pub fn run<T: 'static>(&mut self, flow: impl Future<Output = T> + 'static) -> Option<T> {
        let outcome = Rc::new(RefCell::new(None));
        let kept = Rc::clone(&outcome);
        self.console.spawn(async move {
            let value = flow.await;
            *kept.borrow_mut() = Some(value);
        });
        self.settle();
        outcome.borrow_mut().take()
    }

    /// Answers every command the console sent, delivers the desk's events, and goes on until the
    /// console waits for nothing but timers and held answers.
    pub fn settle(&mut self) {
        loop {
            let effects = self.console.take_effects();
            if effects.is_empty() {
                break;
            }
            for effect in effects {
                match effect {
                    Effect::Run {
                        ticket, command, ..
                    } => {
                        self.calls.push(command.clone());
                        if self.held.contains(&command.name()) {
                            self.holding.push((ticket, command));
                            continue;
                        }
                        let reply = self.desk.answer(&command);
                        self.console.answer(ticket, reply);
                        self.deliver();
                    }
                    Effect::Timer { ticket, .. } => self.timers.push(ticket),
                    Effect::Listen { ticket } => {
                        self.listens += 1;
                        let result = match &self.desk.listen_failure {
                            Some(message) => Err(message.clone()),
                            None => Ok(()),
                        };
                        self.console.listened(ticket, result);
                    }
                    Effect::SetTitle(title) => self.titles.push(title),
                }
            }
        }
    }

    /// Delivers the desk's events waiting.
    pub fn deliver(&mut self) {
        while let Some(event) = self.desk.events.pop_front() {
            self.console.event(event);
        }
    }

    /// Delivers the desk's events and settles.
    pub fn emit(&mut self) {
        self.deliver();
        self.settle();
    }

    /// Fires every timer started so far, then settles.
    pub fn fire_timers(&mut self) {
        for ticket in std::mem::take(&mut self.timers) {
            self.console.fire(ticket);
        }
        self.settle();
    }

    /// Fires timers until none is left (at most 100 rounds).
    pub fn run_timers(&mut self) {
        for _ in 0..100 {
            if self.timers.is_empty() {
                return;
            }
            self.fire_timers();
        }
    }

    /// True while a timer waits to be fired.
    pub fn timers_waiting(&self) -> bool {
        !self.timers.is_empty()
    }

    /// Holds the answers to `command` until [Harness::release].
    pub fn hold(&mut self, command: &'static str) {
        self.held.push(command);
    }

    /// Answers the commands held so far, in order, and settles.
    pub fn release(&mut self) {
        for (ticket, command) in std::mem::take(&mut self.holding) {
            let reply = self.desk.answer(&command);
            self.console.answer(ticket, reply);
            self.deliver();
        }
        self.settle();
    }

    /// The commands named `name` the console sent.
    pub fn calls_to(&self, name: &str) -> Vec<&Command> {
        self.calls
            .iter()
            .filter(|command| command.name() == name)
            .collect()
    }

    /// The driver and the device of every start the console asked for.
    pub fn starts(&self) -> Vec<(String, Option<String>)> {
        self.calls_to("start_engine")
            .into_iter()
            .filter_map(|command| match command {
                Command::StartEngine(request) => Some((
                    request.host.clone().unwrap_or_default(),
                    request.device_filter.clone(),
                )),
                _ => None,
            })
            .collect()
    }

    /// How long the console waits between status reads after a start, for tests that time it.
    pub fn status_retry(&self) -> Duration {
        self.console.options().status_retry
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    #[test]
    fn a_scripted_command_is_answered_by_its_script_with_the_desk_at_hand() {
        let mut desk = FakeDesk::running();
        assert!(
            desk.answer(&Command::OutputDevices).is_err(),
            "a command nobody scripted is refused"
        );
        let asked = Rc::new(Cell::new(0));
        let seen = Rc::clone(&asked);
        desk.script("output_devices", move |desk, command| {
            seen.set(seen.get() + 1);
            assert!(matches!(command, Command::OutputDevices));
            desk.emit_status();
            Ok(Answer::OutputDevices(Vec::new()))
        });

        for _ in 0..2 {
            assert!(matches!(
                desk.answer(&Command::OutputDevices),
                Ok(Answer::OutputDevices(list)) if list.is_empty()
            ));
        }
        assert_eq!(asked.get(), 2, "the script answers every call");
        assert_eq!(desk.events.len(), 2, "the script acts on the desk");
    }

    #[test]
    fn a_script_replaces_the_desks_own_answer_and_a_refusal_still_comes_first() {
        let mut desk = FakeDesk::running();
        desk.script("presets", |_, _| Err("no presets today".to_owned()));
        assert_eq!(
            desk.answer(&Command::Presets).unwrap_err(),
            "no presets today"
        );

        desk.fail_next("presets", "refused", 1);
        assert_eq!(desk.answer(&Command::Presets).unwrap_err(), "refused");
        assert_eq!(
            desk.answer(&Command::Presets).unwrap_err(),
            "no presets today",
            "the script answers again once the refusal is used up"
        );
    }
}
