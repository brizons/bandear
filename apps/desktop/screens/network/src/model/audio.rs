//! The audio setup (`audioSetupView` and friends in `network-view.js`) and its Local mix out
//! (`local-mix.js`), worked out from the console's state and the screen's own draft. Pure.
//!
//! The first window's rules, kept on purpose (N43-N56):
//!
//! - ASIO loads a kernel-mode driver, so it is never chosen by default and never applied by a
//!   driver change alone: the engineer picks an ASIO driver before anything starts.
//! - Only WASAPI switches the input device live; every other driver restarts the engine on it.
//! - A switch that would leave configured groups without their inputs asks first.
//! - Devices listed for another driver are never offered under the one shown.
//! - While a take records or a soundcheck plays, the driver, the device and the buffer stay.
//! - The local mix plays at 48 kHz on a whole stereo pair the running driver offers, or nowhere.

use bandear_desktop_console::console::{RECORDING_BLOCKS_SWITCH, SOUNDCHECK_BLOCKS_SWITCH};
use bandear_desktop_console::demo_source::DEMO_DEVICE;
use bandear_desktop_console::engine_config::{Audio, Host, remembered_device};
use bandear_desktop_console::format::{format_capture_format, sentence_with_reason};
use bandear_desktop_console::state::{CommandAction, ConsoleState};
use bandear_desktop_console::types::dto::{
    InputDeviceDto, LocalOutputRequestDto, LocalOutputSetDto, OutputDeviceDto,
};
use bandear_desktop_console::values::js_round;
use bandear_desktop_console::wire::{LocalOutput, Status};

use super::musicians::count;

/// What a readout shows with no engine.
pub const DASH: &str = "\u{2014}";

/// One choice of a select: the value it stands for and the words it shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub value: String,
    pub label: String,
}

impl Choice {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
        }
    }
}

/// The drivers, the safe one first and the demo source last, which only the engineer chooses.
pub fn host_options() -> Vec<(Host, &'static str)> {
    vec![
        (Host::Auto, "Auto \u{2014} the system's own (safe)"),
        (Host::Wasapi, "WASAPI (shared)"),
        (Host::Asio, "ASIO \u{2014} kernel driver (risky)"),
        (Host::Demo, DEMO_DEVICE),
    ]
}

/// The capture buffer sizes, in frames.
pub const BUFFER_OPTIONS: [(u32, &str); 4] = [
    (128, "128 frames \u{2014} lowest"),
    (256, "256 frames \u{2014} balanced"),
    (512, "512 frames"),
    (1024, "1024 frames \u{2014} safest"),
];

/// The device choices: "Volt 4 — 4 channels", or "channels unknown" when it could not be read.
pub fn device_options(devices: &[InputDeviceDto]) -> Vec<Choice> {
    devices
        .iter()
        .map(|device| {
            let channels = match device.channels {
                0 => "channels unknown".to_owned(),
                1 => "1 channel".to_owned(),
                channels => format!("{channels} channels"),
            };
            Choice::new(&device.name, format!("{} {DASH} {channels}", device.name))
        })
        .collect()
}

/// What picking an input device does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SwitchPlan {
    /// The device in use, or one not listed: nothing.
    None,
    Switch {
        name: String,
    },
    /// Groups would lose inputs: the engineer is asked first.
    Confirm {
        name: String,
        message: String,
    },
}

/// What picking device `name` among `devices` does: a device whose channel count is unknown cannot
/// tell whether groups lose inputs, so it never asks (N46).
pub fn device_switch_plan(
    state: &ConsoleState,
    devices: &[InputDeviceDto],
    name: &str,
) -> SwitchPlan {
    let status = state.engine.status.as_ref();
    let device = devices.iter().find(|device| device.name == name);
    let Some(device) = device.filter(|_| !name.is_empty()) else {
        return SwitchPlan::None;
    };
    if status.is_some_and(|status| status.device_name == name) {
        return SwitchPlan::None;
    }
    let channels = usize::from(device.channels);
    let missing: Vec<&str> = match status {
        Some(status) if channels > 0 => status
            .groups
            .iter()
            .filter(|group| group.channels.iter().any(|channel| *channel >= channels))
            .map(|group| group.name.as_str())
            .collect(),
        _ => Vec::new(),
    };
    if missing.is_empty() {
        return SwitchPlan::Switch {
            name: name.to_owned(),
        };
    }
    let noun = if missing.len() == 1 {
        "group"
    } else {
        "groups"
    };
    SwitchPlan::Confirm {
        name: name.to_owned(),
        message: format!(
            "Switching to {} ({channels} ch) makes {} {noun} unavailable: {}. They are kept, not deleted, and become available again if you switch back.",
            device.name,
            missing.len(),
            missing.join(", ")
        ),
    }
}

/// True when the applied driver switches devices without a restart: WASAPI only.
pub fn switches_live(audio: &Audio) -> bool {
    audio.host == Host::Wasapi
}

/// Above this share of a block's time, on average or at the peak, the engine's load warns.
pub const LOAD_WARNING_PERCENT: f64 = 70.0;

/// The engine's load line and whether it warns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadView {
    pub text: String,
    pub warning: bool,
}

/// "12% (peak 30%)", then the late blocks when there are any; a dash with no engine. The warning is
/// decided on the rounded numbers, so 70.4 does not warn and 70.5 does (N55).
pub fn load_view(status: Option<&Status>) -> LoadView {
    let Some(load) = status.map(|status| &status.load) else {
        return LoadView {
            text: DASH.to_owned(),
            warning: false,
        };
    };
    let average = js_round(f64::from(load.average_percent));
    let peak = js_round(f64::from(load.peak_percent));
    let late = match load.late_blocks {
        0 => String::new(),
        1 => " \u{b7} 1 late block".to_owned(),
        blocks => format!(" \u{b7} {} late blocks", count(blocks)),
    };
    LoadView {
        text: format!("{average}% (peak {peak}%){late}"),
        warning: average > LOAD_WARNING_PERCENT || peak > LOAD_WARNING_PERCENT,
    }
}

/// The screen's own audio choices: an ASIO driver being picked, and a switch waiting for the
/// engineer's word.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Draft {
    /// The driver shown while it waits (only ASIO waits).
    pub host: Option<Host>,
    /// The ASIO driver picked and not started yet.
    pub device: Option<String>,
    /// A switch that asks first: the device and what it would do.
    pub pending_switch: Option<(String, String)>,
}

/// The engine's numbers, each a dash with no engine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Readouts {
    pub format: String,
    pub sample_rate: String,
    pub control_port: String,
    pub samples_received: String,
    pub packets: String,
}

/// What the audio setup shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioSetupView {
    /// The driver shown: an ASIO choice still waiting for its driver, or the applied one.
    pub host: Host,
    pub asio_pending: bool,
    /// True until the shown driver's devices arrive: nothing is offered meanwhile (N47).
    pub listing: bool,
    pub devices: Vec<Choice>,
    /// The device select's choices as drawn: "Choose an ASIO driver…" first while ASIO waits, or
    /// one line that says why there is none.
    pub device_choices: Vec<Choice>,
    /// The device shown (N48).
    pub device: String,
    pub host_disabled: bool,
    pub device_disabled: bool,
    pub can_apply_asio: bool,
    pub buffer: Option<u32>,
    pub buffer_disabled: bool,
    pub recording_lock: Option<&'static str>,
    /// The question a switch asks first.
    pub warning: Option<String>,
    pub readouts: Readouts,
    pub load: LoadView,
}

/// The audio setup for `state` and the screen's `draft`.
pub fn audio_setup_view(state: &ConsoleState, draft: &Draft) -> AudioSetupView {
    let status = state.engine.status.as_ref();
    let soundcheck = state.is_soundcheck_now();
    let locked = state.is_recording_now() || soundcheck;
    let host = draft.host.unwrap_or(state.audio.host);
    let listing = state.devices.host != Some(host);
    let devices = if listing {
        Vec::new()
    } else {
        device_options(&state.devices.list)
    };
    let asio_pending = draft.host == Some(Host::Asio);
    let device = if asio_pending {
        draft.device.clone().unwrap_or_default()
    } else {
        status
            .map(|status| status.device_name.clone())
            .or_else(|| remembered_device(&state.audio))
            .unwrap_or_default()
    };
    let can_switch = status.is_some_and(|status| !status.stopped);
    let device_choices = if devices.is_empty() {
        let empty = if listing {
            "Listing the devices\u{2026}"
        } else {
            "no input devices"
        };
        vec![Choice::new("", empty)]
    } else if asio_pending {
        std::iter::once(Choice::new("", "Choose an ASIO driver\u{2026}"))
            .chain(devices.iter().cloned())
            .collect()
    } else {
        devices.clone()
    };
    let readouts = match status {
        Some(status) => Readouts {
            format: format!(
                "{} ch \u{b7} {}",
                status.capture_format.channels,
                format_capture_format(&status.capture_format)
            ),
            sample_rate: format!("{} Hz", count(u64::from(status.sample_rate))),
            control_port: status.control_port.to_string(),
            samples_received: count(status.samples_received),
            packets: format!(
                "{} / {}",
                count(status.packets_sent),
                count(status.packets_discarded)
            ),
        },
        None => Readouts {
            format: DASH.to_owned(),
            sample_rate: DASH.to_owned(),
            control_port: DASH.to_owned(),
            samples_received: DASH.to_owned(),
            packets: DASH.to_owned(),
        },
    };
    AudioSetupView {
        host,
        asio_pending,
        listing,
        device_disabled: locked || devices.is_empty() || (!asio_pending && !can_switch),
        devices,
        device_choices,
        device,
        host_disabled: locked,
        can_apply_asio: !locked && asio_pending && draft.device.is_some(),
        buffer: state.audio.buffer_frames,
        buffer_disabled: locked,
        recording_lock: if soundcheck {
            Some(SOUNDCHECK_BLOCKS_SWITCH)
        } else if locked {
            Some(RECORDING_BLOCKS_SWITCH)
        } else {
            None
        },
        warning: draft
            .pending_switch
            .as_ref()
            .map(|(_, message)| message.clone()),
        readouts,
        load: load_view(status),
    }
}

/// The refusal of a stop, a start or a switch while the engine went on, in words (`mixMessages`).
pub fn command_message(state: &ConsoleState) -> Option<String> {
    let error = state.command_error.as_ref()?;
    let lead = match error.action {
        CommandAction::Stop => "The engine could not be stopped",
        CommandAction::Start => "The engine could not be started",
        CommandAction::Switch => "The input device could not be switched",
    };
    Some(format!("{lead}: {}.", error.message))
}

/// The word about the audio input (the fallback, or an unsaved choice).
pub fn audio_message(state: &ConsoleState) -> Option<String> {
    state
        .audio_notice
        .as_ref()
        .map(|notice| notice.message.clone())
}

// ---- The local mix's output ---------------------------------------------------------------------

/// The select's value for no local output.
pub const NO_OUTPUT: &str = "";

/// What the select says about the rules, under it.
pub const LOCAL_OUTPUT_HINT: &str = "The local mix plays at 48 kHz on the pair you choose. Under ASIO only the input driver's own outputs are offered; WASAPI offers every output device.";

fn none_choice() -> Choice {
    Choice::new(NO_OUTPUT, "None (lowest load)")
}

/// An output in words: "Volt 4 · outputs 3/4".
pub fn output_label(device: &str, first_channel: u16) -> String {
    format!(
        "{device} \u{b7} outputs {}/{}",
        u32::from(first_channel) + 1,
        u32::from(first_channel) + 2
    )
}

/// The select's value for the pair of `device` that starts at `first_channel` (0-based).
pub fn output_value(device: &str, first_channel: u16) -> String {
    format!("{first_channel}|{device}")
}

/// The choice a select's value names, or `None` for None (or a value that names nothing).
pub fn parse_output_value(value: &str) -> Option<LocalOutputRequestDto> {
    let (first, device) = value.split_once('|')?;
    if first.is_empty() || !first.bytes().all(|byte| byte.is_ascii_digit()) || device.is_empty() {
        return None;
    }
    Some(LocalOutputRequestDto {
        device: device.to_owned(),
        first_channel: first.parse().ok()?,
    })
}

/// None first, then each whole stereo pair of every device listed.
pub fn output_options(devices: &[OutputDeviceDto]) -> Vec<Choice> {
    let mut options = vec![none_choice()];
    for device in devices {
        let mut first = 0;
        while first + 2 <= device.channels {
            options.push(Choice::new(
                output_value(&device.name, first),
                output_label(&device.name, first),
            ));
            first += 2;
        }
    }
    options
}

/// What the local output is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputKind {
    /// No engine runs.
    Idle,
    Running,
    Failed,
    Off,
}

/// The local output's state in words; `alert` when it stopped on its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputState {
    pub kind: OutputKind,
    pub text: String,
    pub alert: Option<String>,
}

fn frames(count_of: u64, noun: &str) -> String {
    format!(
        "{} {noun}{}",
        count(count_of),
        if count_of == 1 { "" } else { "s" }
    )
}

fn output_state(output: Option<&LocalOutput>) -> OutputState {
    let where_ = |output: &LocalOutput| {
        output_label(
            output.device.as_deref().unwrap_or(""),
            output.first_channel.unwrap_or(0),
        )
    };
    match output {
        Some(output) if output.state == "running" => {
            let gaps = if output.underrun_frames > 0 {
                format!(
                    "{} filled in",
                    frames(output.underrun_frames, "silent frame")
                )
            } else {
                "no gaps".to_owned()
            };
            let drops = if output.dropped_frames > 0 {
                format!(
                    "{} dropped to hold the delay",
                    frames(output.dropped_frames, "frame")
                )
            } else {
                "nothing dropped".to_owned()
            };
            OutputState {
                kind: OutputKind::Running,
                text: format!("Playing on {}: {gaps}, {drops}.", where_(output)),
                alert: None,
            }
        }
        Some(output) if output.state == "failed" => OutputState {
            kind: OutputKind::Failed,
            text: "Stopped: the local mix plays nowhere.".to_owned(),
            alert: Some(format!(
                "{} Choose it again, or another output.",
                sentence_with_reason(
                    &format!("{} stopped", where_(output)),
                    output.reason.as_deref()
                )
            )),
        },
        _ => OutputState {
            kind: OutputKind::Off,
            text: "Off: the local mix plays nowhere.".to_owned(),
            alert: None,
        },
    }
}

/// What the Local mix out shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalOutputView {
    pub options: Vec<Choice>,
    pub value: String,
    pub disabled: bool,
    pub state: OutputState,
    /// Why the output the session keeps was not opened, verbatim.
    pub notice: Option<String>,
    pub error: Option<String>,
    pub hint: &'static str,
}

/// The Local mix out for `state`: the output in use (or the one that failed) is shown, an output
/// in use that the engine does not offer now marked so; with no engine, or until the outputs are
/// listed, the select waits (N53).
pub fn local_output_view(state: &ConsoleState) -> LocalOutputView {
    let Some(status) = state.engine.status.as_ref() else {
        return LocalOutputView {
            options: vec![none_choice()],
            value: NO_OUTPUT.to_owned(),
            disabled: true,
            state: OutputState {
                kind: OutputKind::Idle,
                text: "Start the engine to choose where the local mix plays.".to_owned(),
                alert: None,
            },
            notice: None,
            error: None,
            hint: LOCAL_OUTPUT_HINT,
        };
    };
    let output = status.local_output.as_ref();
    let (list, error) = (&state.outputs.list, &state.outputs.error);
    let mut options = output_options(list.as_deref().unwrap_or(&[]));
    let used = output.and_then(|output| {
        output
            .device
            .as_deref()
            .map(|device| (device, output.first_channel.unwrap_or(0)))
    });
    let value = used.map_or_else(
        || NO_OUTPUT.to_owned(),
        |(device, first)| output_value(device, first),
    );
    if let Some((device, first)) = used
        && !options.iter().any(|option| option.value == value)
    {
        options.push(Choice::new(
            &value,
            format!("{} (not offered now)", output_label(device, first)),
        ));
    }
    let waiting = list.is_none() && error.is_none();
    let mut shown = output_state(output);
    if waiting {
        shown.text = "Listing the outputs\u{2026}".to_owned();
    }
    LocalOutputView {
        options,
        value,
        disabled: waiting || status.stopped,
        state: shown,
        notice: output.and_then(|output| output.notice.clone()),
        error: error
            .as_deref()
            .map(|error| sentence_with_reason("The outputs could not be listed", Some(error))),
        hint: LOCAL_OUTPUT_HINT,
    }
}

/// What a change of the local output did (`formatLocalOutputSet`): playing there now (or nowhere),
/// kept for the next start with no engine, or playing but not kept.
pub fn format_local_output_set(answer: &LocalOutputSetDto) -> String {
    let place = answer
        .output
        .as_ref()
        .map(|output| output_label(&output.device, output.first_channel));
    if !answer.applied {
        return match place {
            Some(place) => {
                format!("{place} is kept: the local mix plays there from the next start.")
            }
            None => {
                "No output is kept: the local mix plays nowhere from the next start.".to_owned()
            }
        };
    }
    let done = match place {
        Some(place) => format!("The local mix plays on {place}."),
        None => "The local mix plays nowhere now.".to_owned(),
    };
    if answer.saved {
        done
    } else {
        format!(
            "{done} {}",
            sentence_with_reason(
                "It was not saved for the next start",
                answer.save_error.as_deref()
            )
        )
    }
}

#[cfg(test)]
#[path = "audio_tests.rs"]
mod tests;
