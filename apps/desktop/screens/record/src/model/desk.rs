//! A recorder and a soundcheck scripted onto the console's simulated desk, for the model's tests:
//! the commands the fake desk does not know, answered as the desk answers them (its words, its
//! refusals), with no audio and no clock. A stop takes two seconds of audio.

use std::cell::RefCell;
use std::rc::Rc;

use bandear_desktop_console::command::{Answer, Command};
use bandear_desktop_console::testing::FakeDesk;
use bandear_desktop_console::types::dto::{
    DiskEstimateDto, RecordingDto, RecordingSettingsDto, RecordingStartedDto, RecordingStoppedDto,
    SoundcheckDto, TakeMarkerDto, TakeSummaryDto, TakeTrackDto, TakesDto,
};
use bandear_desktop_console::wire::{Recording, Soundcheck, TakeMarker, TakeSummary};

use crate::record_view::take_name_of;

/// The recordings folder of the scripted desk.
pub const RECORDINGS: &str = "D:\\Recordings";

/// The seconds every take holds once stopped.
pub const TAKE_SECONDS: f64 = 2.0;

/// What the scripted desk keeps.
pub struct Recorder {
    pub settings: RecordingSettingsDto,
    /// Newest first.
    pub takes: Vec<TakeSummaryDto>,
    pub unreadable: usize,
    pub opened: Vec<String>,
    /// How many more stops answer that the files are still being finished.
    pub finishing: usize,
    pub free_bytes: u64,
}

fn settings() -> RecordingSettingsDto {
    RecordingSettingsDto {
        folder: RECORDINGS.to_owned(),
        default_folder: RECORDINGS.to_owned(),
        folder_is_default: true,
        format: "int24",
        include_local: false,
        armed: Vec::new(),
        notice: None,
    }
}

fn state_name(state: &str) -> &'static str {
    match state {
        "playing" => "playing",
        "paused" => "paused",
        "failed" => "failed",
        "recording" => "recording",
        "stopping" => "stopping",
        _ => "off",
    }
}

/// The soundcheck as a command answers it.
pub fn soundcheck_dto(soundcheck: &Soundcheck) -> SoundcheckDto {
    SoundcheckDto {
        state: state_name(&soundcheck.state),
        reason: soundcheck.reason.clone(),
        take_folder: soundcheck.take_folder.clone(),
        take_name: soundcheck.take_name.clone(),
        position_frames: soundcheck.position_frames,
        position_seconds: soundcheck.position_seconds,
        length_frames: soundcheck.length_frames,
        length_seconds: soundcheck.length_seconds,
        looping: soundcheck.looping,
        underrun_frames: soundcheck.underrun_frames,
        clock: if soundcheck.clock == "output" {
            "output"
        } else {
            "timer"
        },
    }
}

fn recording_dto(recording: &Recording) -> RecordingDto {
    RecordingDto {
        state: match recording.state.as_str() {
            "recording" => "recording",
            "stopping" => "stopping",
            "failed" => "failed",
            _ => "idle",
        },
        reason: recording.reason.clone(),
        take_folder: recording.take_folder.clone(),
        frames: recording.frames,
        seconds: recording.seconds,
        dropped_frames: recording.dropped_frames,
        bytes_written: recording.bytes_written,
        tracks: recording.tracks.clone(),
        markers: recording
            .markers
            .iter()
            .map(|marker| TakeMarkerDto {
                frame: marker.frame,
                seconds: marker.seconds,
                name: marker.name.clone(),
            })
            .collect(),
        free_bytes: recording.free_bytes,
        minutes_left: recording.minutes_left,
    }
}

/// `seconds` as the desk says a time: "4.2 s".
fn seconds_words(frames: u64) -> String {
    format!("{:.1} s", frames as f64 / 48_000.0)
}

impl Recorder {
    fn set(&mut self, command: &Command) -> Result<Answer, String> {
        let Command::SetRecordingSettings { settings: request } = command else {
            return Err("not a settings change".to_owned());
        };
        let folder = match request.folder.as_deref().map(str::trim) {
            None | Some("") => None,
            Some(text) if !(text.starts_with('/') || text.get(1..3) == Some(":\\")) => {
                return Err(format!(
                    "the recordings folder must be a whole path, such as D:\\Recordings, and \
                     `{text}` is not"
                ));
            }
            Some(text) if text == self.settings.default_folder => None,
            Some(text) => Some(text.to_owned()),
        };
        let format = match request.format.as_deref().unwrap_or("int24") {
            "int24" => "int24",
            "float32" => "float32",
            other => return Err(format!("the format is int24 or float32, not {other}")),
        };
        if let Some(input) = request
            .armed
            .iter()
            .find(|input| !(1..=255).contains(*input))
        {
            return Err(format!(
                "CH {input} is not an input: inputs go from 1 to 255"
            ));
        }
        let mut armed: Vec<u16> = request.armed.iter().map(|&input| input as u16).collect();
        armed.sort_unstable();
        armed.dedup();
        self.settings = RecordingSettingsDto {
            folder: folder
                .clone()
                .unwrap_or_else(|| self.settings.default_folder.clone()),
            default_folder: self.settings.default_folder.clone(),
            folder_is_default: folder.is_none(),
            format,
            include_local: request.include_local,
            armed,
            notice: None,
        };
        Ok(Answer::RecordingSettings(self.settings.clone()))
    }

    fn estimate(&self, desk: &FakeDesk) -> Answer {
        let tracks = self.settings.armed.len() as u64;
        let local = u64::from(self.settings.include_local) * 2;
        let per_sample = if self.settings.format == "float32" {
            4
        } else {
            3
        };
        let problem = if !desk.running {
            Some("the engine is not running: the tracks depend on the input device".to_owned())
        } else if tracks + local == 0 {
            Some("nothing is armed: arm at least one input, or record the local mix".to_owned())
        } else {
            None
        };
        let bytes_per_second = problem
            .is_none()
            .then_some((tracks + local) * per_sample * 48_000);
        Answer::Estimate(DiskEstimateDto {
            folder: self.settings.folder.clone(),
            bytes_per_second,
            free_bytes: Some(self.free_bytes),
            minutes_left: bytes_per_second.map(|rate| self.free_bytes / rate / 60),
            problem,
        })
    }

    fn start(&mut self, desk: &mut FakeDesk, command: &Command) -> Result<Answer, String> {
        let Command::StartRecording { take_name, .. } = command else {
            return Err("not a start".to_owned());
        };
        let status = &desk.report.status;
        if status.soundcheck.is_soundchecking() {
            return Err(
                "a soundcheck is playing a take: go back to the live inputs before \
                        recording (a take never records playback)"
                    .to_owned(),
            );
        }
        if !desk.running {
            return Err("the engine is not running: start it to record".to_owned());
        }
        if status.recording.is_recording() {
            return Err("a take is already recording: stop it first".to_owned());
        }
        let inputs = status.source_channels as u16;
        let (on_device, skipped): (Vec<u16>, Vec<u16>) = self
            .settings
            .armed
            .iter()
            .partition(|&&input| input <= inputs);
        if on_device.is_empty() && !self.settings.include_local {
            if skipped.is_empty() {
                return Err(
                    "nothing is armed: arm at least one input, or record the local mix".to_owned(),
                );
            }
            let list: Vec<String> = skipped.iter().map(|input| format!("CH {input}")).collect();
            return Err(format!(
                "nothing is armed on this device: {} are past its {inputs} inputs; arm one of its \
                 inputs, or record the local mix",
                list.join(", ")
            ));
        }
        let mut tracks: Vec<String> = on_device
            .iter()
            .map(|&input| status.channels[usize::from(input) - 1].name.clone())
            .collect();
        if self.settings.include_local {
            tracks.push("Local mix".to_owned());
        }
        let name = if take_name.trim().is_empty() {
            "Take"
        } else {
            take_name.trim()
        };
        desk.report.status.recording = Recording {
            state: "recording".to_owned(),
            take_folder: Some(format!(
                "{}\\2026-10-16_210405_{name}",
                self.settings.folder
            )),
            tracks,
            free_bytes: Some(self.free_bytes),
            minutes_left: Some(1000),
            ..Recording::default()
        };
        Ok(Answer::RecordingStarted(RecordingStartedDto {
            recording: recording_dto(&desk.report.status.recording),
            folder: self.settings.folder.clone(),
            armed: on_device,
            skipped,
            include_local: self.settings.include_local,
            format: self.settings.format,
        }))
    }

    fn stop(&mut self, desk: &mut FakeDesk) -> Result<Answer, String> {
        if !desk.running {
            return Err("the engine is not running, so nothing is recording".to_owned());
        }
        let recording = desk.report.status.recording.clone();
        if !(recording.is_recording() || recording.state == "failed") {
            return Err("no take is recording".to_owned());
        }
        if self.finishing > 0 {
            self.finishing -= 1;
            desk.report.status.recording.state = "stopping".to_owned();
            return Ok(Answer::RecordingStopped(RecordingStoppedDto {
                finishing: true,
                take: None,
                message: Some("the take's files are still being finished".to_owned()),
            }));
        }
        let folder = recording.take_folder.clone().unwrap_or_default();
        let frames = (TAKE_SECONDS * 48_000.0) as u64;
        let take = TakeSummaryDto {
            name: take_name_of(Some(&folder)),
            folder,
            started: "2026-10-16T19:04:05Z".to_owned(),
            sample_rate: 48_000,
            format: self.settings.format.to_owned(),
            device: desk.report.status.device_name.clone(),
            tracks: recording
                .tracks
                .iter()
                .map(|name| TakeTrackDto {
                    file: format!("{name}.wav"),
                    kind: if name == "Local mix" {
                        "local"
                    } else {
                        "input"
                    }
                    .to_owned(),
                    channels: Vec::new(),
                    name: name.clone(),
                    instrument: None,
                    frames,
                })
                .collect(),
            frames,
            seconds: TAKE_SECONDS,
            markers: recording
                .markers
                .iter()
                .map(|marker| TakeMarkerDto {
                    frame: marker.frame,
                    seconds: marker.seconds,
                    name: marker.name.clone(),
                })
                .collect(),
            dropped_frames: 0,
            ended: "stopped".to_owned(),
            reason: None,
        };
        self.takes.insert(0, take.clone());
        desk.report.status.last_take = Some(TakeSummary {
            folder: take.folder.clone(),
            name: take.name.clone(),
            frames,
            seconds: TAKE_SECONDS,
            ended: "stopped".to_owned(),
            ..TakeSummary::default()
        });
        desk.report.status.recording = Recording::default();
        Ok(Answer::RecordingStopped(RecordingStoppedDto {
            finishing: false,
            take: Some(take),
            message: None,
        }))
    }

    fn mark(desk: &mut FakeDesk, command: &Command) -> Result<Answer, String> {
        let Command::AddMarker { name } = command else {
            return Err("not a marker".to_owned());
        };
        let recording = &mut desk.report.status.recording;
        if recording.state != "recording" {
            return Err("no take is recording: start one to mark it".to_owned());
        }
        let name = if name.trim().is_empty() {
            format!("Marker {}", recording.markers.len() + 1)
        } else {
            name.trim().to_owned()
        };
        let marker = TakeMarker {
            frame: recording.frames,
            seconds: recording.seconds,
            name,
        };
        recording.markers.push(marker.clone());
        Ok(Answer::Marker(TakeMarkerDto {
            frame: marker.frame,
            seconds: marker.seconds,
            name: marker.name,
        }))
    }

    fn not_a_take(&self, folder: &str) -> String {
        format!(
            "{folder} is not a take folder in the recordings folder {}",
            self.settings.folder
        )
    }

    fn soundcheck(&mut self, desk: &mut FakeDesk, command: &Command) -> Result<Answer, String> {
        let status = &mut desk.report.status;
        let current = &mut status.soundcheck;
        if let Command::StartSoundcheck {
            take_folder,
            start_frame,
            looping,
        } = command
        {
            if status.recording.is_recording() {
                return Err(
                    "a take is recording: stop the recording before the soundcheck".to_owned(),
                );
            }
            if current.is_soundchecking() {
                return Err(
                    "a soundcheck is already playing a take: go back to the live inputs \
                            first"
                        .to_owned(),
                );
            }
            let take = self
                .takes
                .iter()
                .find(|take| &take.folder == take_folder)
                .ok_or_else(|| self.not_a_take(take_folder))?;
            let start = start_frame.unwrap_or(0);
            if start > take.frames {
                return Err(format!(
                    "{} is past the end of the take ({})",
                    seconds_words(start),
                    seconds_words(take.frames)
                ));
            }
            *current = Soundcheck {
                state: "playing".to_owned(),
                take_folder: Some(take.folder.clone()),
                take_name: Some(take.name.clone()),
                position_frames: start,
                position_seconds: start as f64 / 48_000.0,
                length_frames: take.frames,
                length_seconds: take.seconds,
                looping: looping.unwrap_or(false),
                ..Soundcheck::default()
            };
            return Ok(Answer::Soundcheck(soundcheck_dto(current)));
        }
        if let Command::StopSoundcheck = command {
            *current = Soundcheck::default();
            return Ok(Answer::Soundcheck(soundcheck_dto(current)));
        }
        if !current.is_soundchecking() {
            return Err("no soundcheck is playing".to_owned());
        }
        if current.state == "failed" {
            return Err(format!(
                "the soundcheck stopped playing: {}; go back to the live inputs to stop it",
                current.reason.clone().unwrap_or_default()
            ));
        }
        let at_end = current.position_frames >= current.length_frames;
        match command {
            Command::PauseSoundcheck => current.state = "paused".to_owned(),
            Command::ResumeSoundcheck => {
                if at_end {
                    current.position_frames = 0;
                }
                current.state = "playing".to_owned();
            }
            Command::SeekSoundcheck { frame } => {
                if *frame > current.length_frames {
                    return Err(format!(
                        "{} is past the end of the take ({})",
                        seconds_words(*frame),
                        seconds_words(current.length_frames)
                    ));
                }
                // A take stopped at its end plays from the new frame; one paused stays paused.
                if current.state == "paused" && at_end && *frame < current.length_frames {
                    current.state = "playing".to_owned();
                }
                current.position_frames = *frame;
            }
            Command::SetSoundcheckLoop { looping } => current.looping = *looping,
            _ => return Err("not a soundcheck command".to_owned()),
        }
        current.position_seconds = current.position_frames as f64 / 48_000.0;
        Ok(Answer::Soundcheck(soundcheck_dto(current)))
    }
}

/// Scripts the recorder and the soundcheck onto `desk`; the recorder is the test's to look at.
pub fn install(desk: &mut FakeDesk) -> Rc<RefCell<Recorder>> {
    let recorder = Rc::new(RefCell::new(Recorder {
        settings: settings(),
        takes: Vec::new(),
        unreadable: 0,
        opened: Vec::new(),
        finishing: 0,
        free_bytes: 100_000_000_000,
    }));
    let kept = Rc::clone(&recorder);
    desk.script("recording_settings", move |_, _| {
        Ok(Answer::RecordingSettings(kept.borrow().settings.clone()))
    });
    let kept = Rc::clone(&recorder);
    desk.script("set_recording_settings", move |_, command| {
        kept.borrow_mut().set(command)
    });
    let kept = Rc::clone(&recorder);
    desk.script("record_estimate", move |desk, _| {
        Ok(kept.borrow().estimate(desk))
    });
    let kept = Rc::clone(&recorder);
    desk.script("start_recording", move |desk, command| {
        kept.borrow_mut().start(desk, command)
    });
    let kept = Rc::clone(&recorder);
    desk.script("stop_recording", move |desk, _| {
        kept.borrow_mut().stop(desk)
    });
    desk.script("add_marker", Recorder::mark);
    let kept = Rc::clone(&recorder);
    desk.script("list_takes", move |_, _| {
        let recorder = kept.borrow();
        Ok(Answer::Takes(TakesDto {
            folder: recorder.settings.folder.clone(),
            takes: recorder.takes.clone(),
            unreadable: recorder.unreadable,
            missing: false,
        }))
    });
    let kept = Rc::clone(&recorder);
    desk.script("open_take_folder", move |_, command| {
        let Command::OpenTakeFolder { folder } = command else {
            return Err("not a folder".to_owned());
        };
        let mut recorder = kept.borrow_mut();
        if !recorder.takes.iter().any(|take| &take.folder == folder) {
            return Err(recorder.not_a_take(folder));
        }
        recorder.opened.push(folder.clone());
        Ok(Answer::Done)
    });
    for command in [
        "start_soundcheck",
        "pause_soundcheck",
        "resume_soundcheck",
        "seek_soundcheck",
        "set_soundcheck_loop",
        "stop_soundcheck",
    ] {
        let kept = Rc::clone(&recorder);
        desk.script(command, move |desk, command| {
            kept.borrow_mut().soundcheck(desk, command)
        });
    }
    recorder
}
