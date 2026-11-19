//! What the Mix screen shows, worked out from the console state (`mix-view.js`): the top bar, the
//! tabs, the channel and group strips, the master, why the mix area is empty and the sentences
//! above the strips. Pure functions, no window: the view only draws these plain values.

use bandear_desktop_console::format::{
    LinkLevel, SEPARATOR, format_capture_format, format_engineer_controls_set, format_percent,
    link_state, musician_label,
};
use bandear_desktop_console::mix::{EditKey, EditValue, Edits, MixField};
use bandear_desktop_console::router::LAST_CHANNEL;
use bandear_desktop_console::state::{
    CommandAction, ConnectionStatus, ConsoleState, LOCAL_MIX, Phase, pair_units,
};
use bandear_desktop_console::values::{GroupTone, clamp_level, group_tone};
use bandear_desktop_console::wire::{Channel, Meters};
use bandear_desktop_kit::top_bar::EngineAction;

/// The top bar (`topBarView`): the phase, the device and its format (none while no status), how
/// many phones need attention and the link's words, and what the engine button does (none while a
/// start or a stop is on).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TopBarView {
    pub phase: Phase,
    pub device: Option<String>,
    pub format: Option<String>,
    pub attention: usize,
    pub attention_label: Option<String>,
    pub action: Option<EngineAction>,
}

pub fn top_bar_view(state: &ConsoleState) -> TopBarView {
    let attention = state.musicians_needing_attention().len();
    let attention_label = (attention > 0).then(|| {
        let need = if attention == 1 {
            "phone needs"
        } else {
            "phones need"
        };
        format!("{attention} {need} attention")
    });
    let status = state.engine.status.as_ref();
    TopBarView {
        phase: state.engine.phase,
        device: status.map(|status| status.device_name.clone()),
        format: status.map(|status| format_capture_format(&status.capture_format)),
        attention,
        attention_label,
        action: match state.engine.phase {
            Phase::Idle | Phase::Error => Some(EngineAction::Start),
            Phase::Running | Phase::Stopped => Some(EngineAction::Stop),
            Phase::Starting | Phase::Stopping => None,
        },
    }
}

/// How a tab's link reads: a phone's link level, or the local mix's own word about where it plays
/// (`local`, drawn as the plain ring).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabLevel {
    Good,
    Warning,
    Bad,
    Unknown,
    Local,
}

impl From<LinkLevel> for TabLevel {
    fn from(level: LinkLevel) -> Self {
        match level {
            LinkLevel::Good => Self::Good,
            LinkLevel::Warning => Self::Warning,
            LinkLevel::Bad => Self::Bad,
            LinkLevel::Unknown => Self::Unknown,
        }
    }
}

/// A tab's link: its level, its word and the detail its tooltip says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabLink {
    pub level: TabLevel,
    pub label: String,
    pub detail: String,
}

/// One tab: whose mix it opens, its name, its link, and whether it is open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MixTab {
    pub address: String,
    pub label: String,
    pub link: TabLink,
    pub selected: bool,
}

/// One tab per musician, in the engine's order.
pub fn musician_tabs(state: &ConsoleState) -> Vec<MixTab> {
    state
        .musicians
        .iter()
        .map(|musician| {
            let link = link_state(musician);
            MixTab {
                address: musician.address.clone(),
                label: musician_label(musician.name.as_deref(), &musician.address),
                link: TabLink {
                    level: link.level.into(),
                    label: link.label,
                    detail: link.detail,
                },
                selected: state.selected.as_deref() == Some(musician.address.as_str()),
            }
        })
        .collect()
}

/// An output in words: "Volt 4 · outputs 3/4".
pub fn output_label(device: &str, first_channel: u16) -> String {
    format!(
        "{device}{SEPARATOR}outputs {}/{}",
        first_channel + 1,
        first_channel + 2
    )
}

/// The Local mix tab (`localTab`), or `None` while there is no local mix (no engine runs). Its link
/// says where the local mix plays, a mark and a word, never a colour alone.
pub fn local_tab(state: &ConsoleState) -> Option<MixTab> {
    state.local_mix.as_ref()?;
    let output = state
        .engine
        .status
        .as_ref()
        .and_then(|status| status.local_output.as_ref());
    let link = match output {
        Some(output) if output.state == "running" => TabLink {
            level: TabLevel::Local,
            label: format!(
                "This computer{SEPARATOR}{}",
                output_label(
                    output.device.as_deref().unwrap_or_default(),
                    output.first_channel.unwrap_or(0)
                )
            ),
            detail: "The local mix plays on this computer".to_owned(),
        },
        Some(output) if output.state == "failed" => TabLink {
            level: TabLevel::Warning,
            label: format!("This computer{SEPARATOR}output stopped"),
            detail: output
                .reason
                .clone()
                .unwrap_or_else(|| "The output stopped".to_owned()),
        },
        _ => TabLink {
            level: TabLevel::Local,
            label: format!("This computer{SEPARATOR}no output"),
            detail: "Choose where it plays in Network and system".to_owned(),
        },
    };
    Some(MixTab {
        address: LOCAL_MIX.to_owned(),
        label: "Local mix".to_owned(),
        link,
        selected: state.selected.as_deref() == Some(LOCAL_MIX),
    })
}

/// The tabs: one per musician, then the Local mix while an engine runs.
pub fn mix_tabs(state: &ConsoleState) -> Vec<MixTab> {
    let mut tabs = musician_tabs(state);
    tabs.extend(local_tab(state));
    tabs
}

/// The name of the musician whose mix `address`'s follows (a mirror), or `None`.
pub fn following_of(state: &ConsoleState, address: &str) -> Option<String> {
    state
        .musician_by_address(address)
        .and_then(|musician| musician.mirror.as_ref())
        .map(|mirror| mirror.name.clone())
}

/// How a strip of a mix that follows another shows it (`followingState`): its moves are off, the
/// words under its name, and its accessible name from `name`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Following {
    pub disabled: bool,
    pub text: String,
    pub label: String,
}

pub fn following_state(following: Option<&str>, name: &str) -> Following {
    match following {
        Some(leader) => Following {
            disabled: true,
            text: format!("Follows {leader}"),
            label: format!("{name}, follows {leader}'s mix"),
        },
        None => Following {
            disabled: false,
            text: String::new(),
            label: name.to_owned(),
        },
    }
}

/// A strip's group, found by id (two groups may share a name), in its chip's tone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StripGroup {
    pub id: String,
    pub name: String,
    pub tone: GroupTone,
    pub invalid: bool,
}

/// A channel fader's fill: muted dims it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StripTone {
    Default,
    Muted,
}

/// One strip of a mix (`channelStrips`): an input, or a stereo pair as one strip (`sides` both
/// inputs, `index` the first). A pair's level, mute and solo are its first side's, its pan the
/// balance its two pans make, and it is locked when either side is. `processing` says whether the
/// first side's processing is on (`None` while the strips were never read).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelStrip {
    pub index: usize,
    pub sides: Vec<usize>,
    pub stereo: bool,
    pub label: String,
    pub kind: String,
    pub group: Option<StripGroup>,
    pub gain: u8,
    pub pan: u8,
    pub muted: bool,
    pub solo: bool,
    pub tone: StripTone,
    pub readout: String,
    pub lockable: bool,
    pub locked: bool,
    pub lock_text: &'static str,
    pub following: Option<String>,
    pub processing: Option<bool>,
}

/// An input's name, or its number when it has none ("CH 5").
pub fn channel_label(channel: &Channel) -> String {
    let name = channel.name.trim();
    if name.is_empty() {
        format!("CH {}", channel.number)
    } else {
        name.to_owned()
    }
}

/// The two pans a pair's balance writes: 0 hard left, 50 the full width, 100 hard right.
pub fn balance_to_pans(balance: u8) -> (u8, u8) {
    let level = i32::from(clamp_level(f64::from(balance)));
    ((2 * level - 100).max(0) as u8, (2 * level).min(100) as u8)
}

/// The balance a pair's two pans show: their mean, which reads back what [balance_to_pans] wrote.
pub fn pans_to_balance(first: u8, second: u8) -> u8 {
    clamp_level((f64::from(first) + f64::from(second)) / 2.0)
}

fn entry<T: Copy + Default>(table: &[T], index: usize) -> T {
    table.get(index).copied().unwrap_or_default()
}

/// The groups of the status, with each one's position and tone.
fn groups_with_tones(state: &ConsoleState) -> Vec<(usize, StripGroup)> {
    state
        .engine
        .status
        .iter()
        .flat_map(|status| status.groups.iter())
        .enumerate()
        .map(|(index, group)| {
            (
                index,
                StripGroup {
                    id: group.id.clone(),
                    name: group.name.clone(),
                    tone: group_tone(Some(index as i64)),
                    invalid: group.invalid,
                },
            )
        })
        .collect()
}

/// One strip per shown input of the device, left to right, for `address`'s mix as it shows.
pub fn channel_strips(state: &ConsoleState, address: &str) -> Vec<ChannelStrip> {
    let (Some(mix), Some(status)) = (state.view_mix(address), state.engine.status.as_ref()) else {
        return Vec::new();
    };
    let groups = groups_with_tones(state);
    let channels = &status.channels[..status.channels.len().min(status.source_channels)];
    let following = following_of(state, address);
    let lockable = address != LOCAL_MIX;
    pair_units(channels)
        .into_iter()
        .filter(|sides| !channels[sides[0]].hidden)
        .map(|sides| {
            let index = sides[0];
            let channel = &channels[index];
            let stereo = sides.len() > 1;
            let group = channel.group_id.as_ref().and_then(|id| {
                groups
                    .iter()
                    .find(|(_, group)| &group.id == id)
                    .map(|(_, group)| group.clone())
            });
            let muted = entry(&mix.channel_muted, index);
            let gain = entry(&mix.channel_gains, index);
            let locked = lockable && sides.iter().any(|side| entry(&mix.locked_channels, *side));
            let pan = if stereo {
                pans_to_balance(entry(&mix.pans, index), entry(&mix.pans, sides[1]))
            } else {
                entry(&mix.pans, index)
            };
            ChannelStrip {
                index,
                stereo,
                label: channel_label(channel),
                kind: if channel.kind.is_empty() {
                    "other".to_owned()
                } else {
                    channel.kind.clone()
                },
                group,
                gain,
                pan,
                muted,
                solo: entry(&mix.channel_solo, index),
                tone: if muted {
                    StripTone::Muted
                } else {
                    StripTone::Default
                },
                readout: if muted {
                    "muted".to_owned()
                } else {
                    format_percent(f64::from(gain))
                },
                lockable,
                locked,
                lock_text: if locked { "Locked" } else { "Unlocked" },
                following: following.clone(),
                processing: state.processing_active(index + 1),
                sides,
            }
        })
        .collect()
}

/// The edits a move on a channel strip makes (`channelEdits`): a pair's level, mute, solo or lock
/// goes to both its inputs and its balance becomes both pans; a strip of one input edits that one.
pub fn channel_edits(sides: &[usize], stereo: bool, field: MixField, value: EditValue) -> Edits {
    let key = |index: usize| EditKey {
        field,
        index: Some(index),
    };
    if field == MixField::Pans && stereo && sides.len() > 1 {
        let (left, right) = balance_to_pans(value.level());
        return Edits::from([
            (key(sides[0]), EditValue::Level(left)),
            (key(sides[1]), EditValue::Level(right)),
        ]);
    }
    sides.iter().map(|side| (key(*side), value)).collect()
}

/// The master meter's left and right levels from `meters`, for the mix open: the local mix's peak
/// on the Local mix tab, a musician's output on theirs, silence otherwise.
pub fn master_levels(meters: Option<&Meters>, selected: Option<&str>) -> (u8, u8) {
    let output = meters.and_then(|meters| match selected {
        Some(LOCAL_MIX) => Some(&meters.local),
        Some(address) => meters.musicians.get(address),
        None => None,
    });
    output.map_or((0, 0), |output| (output.left, output.right))
}

/// The level a strip's meter shows: its input's peak, or the louder side of a pair.
pub fn strip_level(sides: &[usize], levels: &[u8]) -> u8 {
    sides
        .iter()
        .map(|side| entry(levels, *side))
        .max()
        .unwrap_or(0)
}

/// One strip per configured group (`groupStrips`): `index` is the group's position, where its level
/// is in the mix. Groups have no mute of their own in the engine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupStrip {
    pub index: usize,
    pub id: String,
    pub name: String,
    pub tone: GroupTone,
    pub invalid: bool,
    pub level: u8,
    pub readout: String,
    pub following: Option<String>,
}

pub fn group_strips(state: &ConsoleState, address: &str) -> Vec<GroupStrip> {
    let Some(mix) = state.view_mix(address) else {
        return Vec::new();
    };
    let following = following_of(state, address);
    groups_with_tones(state)
        .into_iter()
        .map(|(index, group)| {
            let level = entry(&mix.group_levels, index);
            GroupStrip {
                index,
                id: group.id,
                name: group.name,
                tone: group.tone,
                invalid: group.invalid,
                level,
                readout: format_percent(f64::from(level)),
                following: following.clone(),
            }
        })
        .collect()
}

/// A mix's master (`masterStrip`): its level, the phone's own level cap and the engineer's limit
/// (the engine plays at most the lower, which `ceiling_note` says), and its mute. The local mix has
/// neither cap nor limit: its readout is the level alone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MasterStrip {
    pub volume: u8,
    pub cap: Option<u8>,
    pub limit: Option<u8>,
    pub muted: bool,
    pub readout: String,
    pub ceiling_note: Option<String>,
}

pub fn master_strip(state: &ConsoleState, address: &str) -> Option<MasterStrip> {
    let mix = state.view_mix(address)?;
    let level = if mix.muted {
        "muted".to_owned()
    } else {
        format_percent(f64::from(mix.volume_percent))
    };
    if address == LOCAL_MIX {
        return Some(MasterStrip {
            volume: mix.volume_percent,
            cap: None,
            limit: None,
            muted: mix.muted,
            readout: level,
            ceiling_note: None,
        });
    }
    let limit = mix.engineer_limit_percent;
    let ceiling = mix.max_level_percent.min(limit);
    Some(MasterStrip {
        volume: mix.volume_percent,
        cap: Some(mix.max_level_percent),
        limit: Some(limit),
        muted: mix.muted,
        readout: [
            level,
            format!("cap {}", format_percent(f64::from(mix.max_level_percent))),
            format!("limit {}", format_percent(f64::from(limit))),
        ]
        .join(SEPARATOR),
        ceiling_note: Some(format!(
            "The lower of the phone's cap and your limit wins: at most {}.",
            format_percent(f64::from(ceiling))
        )),
    })
}

/// Why the mix area has no strips (`mixNotice`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MixNotice {
    /// The console cannot hear the engine, for this reason.
    Disconnected(Option<String>),
    Connecting,
    Stopped,
    Starting,
    Stopping,
    /// The engine is not running, for this error.
    Error(Option<String>),
    /// No phone is here and the local mix is not open (it plays with no phone).
    NoMusicians,
}

/// What a notice's button does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoticeAction {
    /// Connects again ("Try again" after a lost connection).
    Reconnect,
    /// Starts the engine ("Start engine", "Try again" after a failed start).
    Start,
}

/// The words that send the engineer to Network and system from the no-phones notice, before its
/// link.
pub const NO_MUSICIANS_TEXT: &str = "Musicians appear here when they open BandEar on their phone and join this computer. You can also add a phone by address in ";

impl MixNotice {
    pub fn title(&self) -> &'static str {
        match self {
            Self::Disconnected(_) => "The console cannot reach the engine",
            Self::Connecting => "Connecting to the engine\u{2026}",
            Self::Stopped => "The engine is stopped",
            Self::Starting => "Starting the engine\u{2026}",
            Self::Stopping => "Stopping the engine\u{2026}",
            Self::Error(_) => "The engine is not running",
            Self::NoMusicians => "No phones yet",
        }
    }

    /// The text under the title; for the no-phones notice, the words before its link to Network
    /// and system (and the full stop after it).
    pub fn text(&self) -> Option<String> {
        match self {
            Self::Disconnected(message) | Self::Error(message) => message.clone(),
            Self::Connecting => None,
            Self::Stopped => Some("Start it to send each musician their mix.".to_owned()),
            Self::Starting => Some("Opening the audio input.".to_owned()),
            Self::Stopping => Some("Closing the audio input.".to_owned()),
            Self::NoMusicians => Some(NO_MUSICIANS_TEXT.to_owned()),
        }
    }

    /// The button's words and what it does.
    pub fn action(&self) -> Option<(&'static str, NoticeAction)> {
        match self {
            Self::Disconnected(_) => Some(("Try again", NoticeAction::Reconnect)),
            Self::Stopped => Some(("Start engine", NoticeAction::Start)),
            Self::Error(_) => Some(("Try again", NoticeAction::Start)),
            _ => None,
        }
    }

    /// True for the notice whose text ends in a link to Network and system.
    pub fn links_network(&self) -> bool {
        matches!(self, Self::NoMusicians)
    }
}

pub fn mix_notice(state: &ConsoleState) -> Option<MixNotice> {
    let phase = state.engine.phase;
    match state.connection.status {
        ConnectionStatus::Failed => {
            return Some(MixNotice::Disconnected(state.connection.error.clone()));
        }
        ConnectionStatus::Connecting if phase == Phase::Idle => {
            return Some(MixNotice::Connecting);
        }
        _ => {}
    }
    match phase {
        Phase::Idle | Phase::Stopped => return Some(MixNotice::Stopped),
        Phase::Starting => return Some(MixNotice::Starting),
        Phase::Stopping => return Some(MixNotice::Stopping),
        Phase::Error => return Some(MixNotice::Error(state.engine.error.clone())),
        Phase::Running => {}
    }
    if state.selected.as_deref() == Some(LOCAL_MIX) && state.local_mix.is_some() {
        return None;
    }
    state.musicians.is_empty().then_some(MixNotice::NoMusicians)
}

/// The sentences above the strips (`mixMessages`), each `None` when there is nothing to say.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MixMessages {
    /// The open musician was removed and another one opened.
    pub departed: Option<String>,
    /// A change to a mix was refused.
    pub error: Option<String>,
    /// The engineer's limit and locks did not both apply and save.
    pub controls: Option<String>,
    /// The engine refused a stop, a start or a device switch while it went on.
    pub command: Option<String>,
    /// The engine runs on another input than the saved one, or the choices were not saved.
    pub audio: Option<String>,
}

fn label_at(state: &ConsoleState, address: &str) -> String {
    match state.musician_by_address(address) {
        Some(musician) => musician_label(musician.name.as_deref(), &musician.address),
        None => address.to_owned(),
    }
}

pub fn mix_messages(state: &ConsoleState) -> MixMessages {
    let shown = state.selected_musician();
    let departed = match (&state.departed, shown) {
        (Some(departed), Some(shown)) => Some(format!(
            "{} was removed; showing {}'s mix.",
            departed.label,
            musician_label(shown.name.as_deref(), &shown.address)
        )),
        _ => None,
    };
    let error = state.mix_error.as_ref().map(|error| {
        let whose = if error.address == LOCAL_MIX {
            "the local mix".to_owned()
        } else {
            format!("{}'s mix", label_at(state, &error.address))
        };
        format!("A change to {whose} was not applied: {}.", error.message)
    });
    let controls = state.controls_notice.as_ref().map(|notice| {
        let outcome = &notice.outcome;
        let name = label_at(state, &notice.address);
        format_engineer_controls_set(
            outcome.applied,
            outcome.saved,
            outcome.save_error.as_deref(),
            Some(&name),
        )
    });
    let command = state.command_error.as_ref().map(|error| {
        let lead = match error.action {
            CommandAction::Stop => "The engine could not be stopped",
            CommandAction::Start => "The engine could not be started",
            CommandAction::Switch => "The input device could not be switched",
        };
        format!("{lead}: {}.", error.message)
    });
    MixMessages {
        departed,
        error,
        controls,
        command,
        audio: state
            .audio_notice
            .as_ref()
            .map(|notice| notice.message.clone()),
    }
}

/// Whether an input's processing does something, in the strip's link (`processingLink`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessingState {
    On,
    Off,
    /// The strips were never read.
    Unknown,
}

/// A strip's link to its input's processing: the channel it opens (a pair's first side, 1-based),
/// its words and its accessible name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessingLink {
    pub channel: u8,
    pub state: ProcessingState,
    pub text: &'static str,
    pub label: String,
}

/// The tooltip of every strip's processing link.
pub const PROCESSING_LINK_TITLE: &str =
    "Open this input's high-pass, equaliser, gate and compressor";

pub fn processing_link(sides: &[usize], label: &str, active: Option<bool>) -> ProcessingLink {
    let first = sides.iter().min().copied().unwrap_or(0) + 1;
    let state = match active {
        Some(true) => ProcessingState::On,
        Some(false) => ProcessingState::Off,
        None => ProcessingState::Unknown,
    };
    let said = match state {
        ProcessingState::On => "on",
        ProcessingState::Off => "off",
        ProcessingState::Unknown => "not read yet",
    };
    ProcessingLink {
        channel: first.min(usize::from(LAST_CHANNEL)) as u8,
        state,
        text: if state == ProcessingState::On {
            "Processing on"
        } else {
            "Processing"
        },
        label: format!("Processing of {label}, {said}"),
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod tests_more;
