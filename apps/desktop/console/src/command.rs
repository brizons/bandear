//! Every command the window asks of the desk (the 67 the Tauri shell registered), with typed
//! arguments, and every answer, typed by what it carries.
//!
//! A [Command] goes out as an [crate::console::Effect::Run] with its [Ticket] and its [Owner]; the
//! app runs it on a background thread through [crate::desk::Desk] and hands the [Reply] back with
//! [crate::console::Console::answer], which wakes exactly the flow that asked. The arguments are the
//! desk's own request types, and the answers its own DTOs (re-exported in [crate::types]), except
//! `engine_status`, whose answer the console reads into [EngineState] like the `status` event.

use bandear_desktop_core::backups::MixBackupSummary;
use bandear_desktop_core::commands::processing::{
    ChannelProcessingDto, EqPointDto, ProcessingSetDto, StripPresetDto, StripPresetsSetDto,
};
use bandear_desktop_core::dto::{
    AudioSettingsRequest, BufferSetDto, ChannelRequest, ChannelsSetDto, DiskEstimateDto,
    EffectSetDto, EngineerControlsRequest, EngineerControlsSetDto, FxReturnSetDto, GroupRequest,
    GroupsSetDto, InputDeviceDto, LimiterSetDto, LinkHistoryDto, LocalOutputRequestDto,
    LocalOutputSetDto, MirrorRequest, MirrorSetDto, MixDto, MixRestoredDto, MixUpdateRequest,
    MixesSetDto, OutputDeviceDto, PresetsDto, RecordingSettingsDto, RecordingSettingsRequest,
    RecordingStartedDto, RecordingStoppedDto, RedundancySetDto, SessionLoadedDto, SessionSavedDto,
    SoundcheckDto, StartRequest, StartSummary, TakeMarkerDto, TakesDto,
};
use bandear_desktop_core::logfile::LogLocation;
use bandear_desktop_core::session::{SessionAudio, SessionChannel, SessionPreset};
use bandear_desktop_core::sessions::SnapshotSummary;
use personal_monitoring::{Effect, EqSettings, LimiterSettings, StripSettings};

use crate::router::Section;
use crate::wire::EngineState;

/// Names one command in flight; the desk's answer comes back with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ticket(pub u64);

/// Who asked: the console's own flows, or a screen's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Owner {
    Console,
    Screen(Section),
}

/// A command of the desk, with its arguments.
#[derive(Clone, Debug)]
pub enum Command {
    StartEngine(StartRequest),
    EngineStatus,
    StopEngine,
    ListInputDevices {
        host: Option<String>,
    },
    SwitchDevice {
        device: String,
    },
    AddTarget {
        address: String,
    },
    RemoveTarget {
        address: String,
    },
    SetMix {
        address: String,
        mix: MixUpdateRequest,
    },
    SetLocalMix {
        mix: MixUpdateRequest,
    },
    OutputDevices,
    SetLocalOutput {
        output: Option<LocalOutputRequestDto>,
    },
    SetMusicianName {
        address: String,
        name: String,
    },
    SetGroups {
        groups: Vec<GroupRequest>,
    },
    SetChannels {
        channels: Vec<ChannelRequest>,
    },
    ChannelTable,
    SetEngineerControls {
        address: String,
        client_id: Option<String>,
        controls: EngineerControlsRequest,
    },
    AudioSettings,
    SetAudioSettings {
        settings: AudioSettingsRequest,
    },
    Presets,
    SavePreset {
        address: String,
        name: String,
    },
    RenamePreset {
        id: String,
        name: String,
    },
    DeletePreset {
        id: String,
    },
    ApplyPreset {
        id: String,
        musicians: Vec<String>,
    },
    CopyMix {
        from: String,
        to: Vec<String>,
    },
    SetMirror {
        address: String,
        client_id: Option<String>,
        leader: Option<MirrorRequest>,
    },
    SessionNotice,
    Sessions,
    SaveSession {
        name: String,
    },
    RenameSession {
        id: String,
        name: String,
    },
    DeleteSession {
        id: String,
    },
    LoadSession {
        id: String,
    },
    RecordingSettings,
    SetRecordingSettings {
        settings: RecordingSettingsRequest,
    },
    StartRecording {
        take_name: String,
        utc_offset_minutes: Option<i32>,
    },
    StopRecording,
    AddMarker {
        name: String,
    },
    RecordEstimate,
    ListTakes,
    OpenTakeFolder {
        folder: String,
    },
    StartSoundcheck {
        take_folder: String,
        start_frame: Option<u64>,
        looping: Option<bool>,
    },
    PauseSoundcheck,
    ResumeSoundcheck,
    SeekSoundcheck {
        frame: u64,
    },
    SetSoundcheckLoop {
        looping: bool,
    },
    StopSoundcheck,
    LogLocation,
    ListMixBackups,
    RestoreMixBackup {
        stamp: String,
    },
    LinkHistory {
        musician_key: String,
        since_seq: Option<u64>,
    },
    SetMusicianBuffer {
        musician_key: String,
        mode: String,
        ms: Option<u32>,
    },
    SetMusicianRedundancy {
        musician_key: String,
        on: bool,
        delay_blocks: Option<u32>,
    },
    SetMusicianLimiter {
        musician_key: String,
        settings: LimiterSettings,
    },
    SetLocalLimiter {
        settings: LimiterSettings,
    },
    SetChannelSend {
        channel: usize,
        effect: Effect,
        level: u8,
    },
    /// An effect's settings, in the shape of that effect's (the desk reads them as JSON).
    SetEffect {
        effect: Effect,
        settings: serde_json::Value,
    },
    SetMusicianFxReturn {
        musician_key: String,
        effect: Effect,
        level: u8,
    },
    SetLocalFxReturn {
        effect: Effect,
        level: u8,
    },
    Processing,
    ChannelProcessing {
        channel: usize,
    },
    SetChannelProcessing {
        channel: usize,
        settings: StripSettings,
    },
    ResetChannelProcessing {
        channel: usize,
    },
    CopyChannelProcessing {
        from: usize,
        to: Vec<usize>,
    },
    StripPresets,
    SaveStripPreset {
        name: String,
        settings: StripSettings,
    },
    DeleteStripPreset {
        name: String,
    },
    ApplyStripPreset {
        name: String,
        channel: usize,
    },
    EqResponse {
        settings: EqSettings,
        points: Option<usize>,
    },
}

impl Command {
    /// The command's name, as the Tauri shell registered it.
    pub fn name(&self) -> &'static str {
        match self {
            Self::StartEngine(_) => "start_engine",
            Self::EngineStatus => "engine_status",
            Self::StopEngine => "stop_engine",
            Self::ListInputDevices { .. } => "list_input_devices",
            Self::SwitchDevice { .. } => "switch_device",
            Self::AddTarget { .. } => "add_target",
            Self::RemoveTarget { .. } => "remove_target",
            Self::SetMix { .. } => "set_mix",
            Self::SetLocalMix { .. } => "set_local_mix",
            Self::OutputDevices => "output_devices",
            Self::SetLocalOutput { .. } => "set_local_output",
            Self::SetMusicianName { .. } => "set_musician_name",
            Self::SetGroups { .. } => "set_groups",
            Self::SetChannels { .. } => "set_channels",
            Self::ChannelTable => "channel_table",
            Self::SetEngineerControls { .. } => "set_engineer_controls",
            Self::AudioSettings => "audio_settings",
            Self::SetAudioSettings { .. } => "set_audio_settings",
            Self::Presets => "presets",
            Self::SavePreset { .. } => "save_preset",
            Self::RenamePreset { .. } => "rename_preset",
            Self::DeletePreset { .. } => "delete_preset",
            Self::ApplyPreset { .. } => "apply_preset",
            Self::CopyMix { .. } => "copy_mix",
            Self::SetMirror { .. } => "set_mirror",
            Self::SessionNotice => "session_notice",
            Self::Sessions => "sessions",
            Self::SaveSession { .. } => "save_session",
            Self::RenameSession { .. } => "rename_session",
            Self::DeleteSession { .. } => "delete_session",
            Self::LoadSession { .. } => "load_session",
            Self::RecordingSettings => "recording_settings",
            Self::SetRecordingSettings { .. } => "set_recording_settings",
            Self::StartRecording { .. } => "start_recording",
            Self::StopRecording => "stop_recording",
            Self::AddMarker { .. } => "add_marker",
            Self::RecordEstimate => "record_estimate",
            Self::ListTakes => "list_takes",
            Self::OpenTakeFolder { .. } => "open_take_folder",
            Self::StartSoundcheck { .. } => "start_soundcheck",
            Self::PauseSoundcheck => "pause_soundcheck",
            Self::ResumeSoundcheck => "resume_soundcheck",
            Self::SeekSoundcheck { .. } => "seek_soundcheck",
            Self::SetSoundcheckLoop { .. } => "set_soundcheck_loop",
            Self::StopSoundcheck => "stop_soundcheck",
            Self::LogLocation => "log_location",
            Self::ListMixBackups => "list_mix_backups",
            Self::RestoreMixBackup { .. } => "restore_mix_backup",
            Self::LinkHistory { .. } => "link_history",
            Self::SetMusicianBuffer { .. } => "set_musician_buffer",
            Self::SetMusicianRedundancy { .. } => "set_musician_redundancy",
            Self::SetMusicianLimiter { .. } => "set_musician_limiter",
            Self::SetLocalLimiter { .. } => "set_local_limiter",
            Self::SetChannelSend { .. } => "set_channel_send",
            Self::SetEffect { .. } => "set_effect",
            Self::SetMusicianFxReturn { .. } => "set_musician_fx_return",
            Self::SetLocalFxReturn { .. } => "set_local_fx_return",
            Self::Processing => "processing",
            Self::ChannelProcessing { .. } => "channel_processing",
            Self::SetChannelProcessing { .. } => "set_channel_processing",
            Self::ResetChannelProcessing { .. } => "reset_channel_processing",
            Self::CopyChannelProcessing { .. } => "copy_channel_processing",
            Self::StripPresets => "strip_presets",
            Self::SaveStripPreset { .. } => "save_strip_preset",
            Self::DeleteStripPreset { .. } => "delete_strip_preset",
            Self::ApplyStripPreset { .. } => "apply_strip_preset",
            Self::EqResponse { .. } => "eq_response",
        }
    }
}

/// The desk's answer to a command, by what it carries.
#[derive(Clone, Debug)]
pub enum Answer {
    /// A command that answers nothing (`stop_engine`, `set_musician_name`, `delete_session`,
    /// `open_take_folder`).
    Done,
    /// `start_engine`, `switch_device`.
    Started(StartSummary),
    EngineState(EngineState),
    InputDevices(Vec<InputDeviceDto>),
    /// `add_target`, `remove_target`: the address as the desk keeps it.
    Address(String),
    /// `set_mix`, `set_local_mix`.
    Mix(MixDto),
    OutputDevices(Vec<OutputDeviceDto>),
    LocalOutputSet(LocalOutputSetDto),
    GroupsSet(GroupsSetDto),
    ChannelsSet(ChannelsSetDto),
    ChannelTable(Vec<SessionChannel>),
    EngineerControlsSet(EngineerControlsSetDto),
    /// `audio_settings`: the choices the session keeps, if any.
    AudioSettings(Option<SessionAudio>),
    /// `set_audio_settings`: the choices now kept.
    AudioSaved(SessionAudio),
    Presets(Vec<SessionPreset>),
    /// `save_preset`, `rename_preset`, `delete_preset`.
    PresetsSet(PresetsDto),
    /// `apply_preset`, `copy_mix`.
    Mixes(MixesSetDto),
    MirrorSet(MirrorSetDto),
    SessionNotice(Option<String>),
    Sessions(Vec<SnapshotSummary>),
    SessionSaved(SessionSavedDto),
    /// `rename_session`.
    SessionRenamed(SnapshotSummary),
    SessionLoaded(SessionLoadedDto),
    /// `recording_settings`, `set_recording_settings`.
    RecordingSettings(RecordingSettingsDto),
    RecordingStarted(RecordingStartedDto),
    RecordingStopped(RecordingStoppedDto),
    Marker(TakeMarkerDto),
    Estimate(DiskEstimateDto),
    Takes(TakesDto),
    /// Every soundcheck command.
    Soundcheck(SoundcheckDto),
    LogLocation(LogLocation),
    MixBackups(Vec<MixBackupSummary>),
    MixRestored(MixRestoredDto),
    LinkHistory(LinkHistoryDto),
    BufferSet(BufferSetDto),
    RedundancySet(RedundancySetDto),
    /// `set_musician_limiter`, `set_local_limiter`.
    LimiterSet(LimiterSetDto),
    /// `set_channel_send`, `set_channel_processing`, `reset_channel_processing`,
    /// `copy_channel_processing`, `apply_strip_preset`.
    ProcessingSet(ProcessingSetDto),
    EffectSet(EffectSetDto),
    /// `set_musician_fx_return`, `set_local_fx_return`.
    FxReturnSet(FxReturnSetDto),
    Processing(Vec<ChannelProcessingDto>),
    ChannelProcessing(ChannelProcessingDto),
    StripPresets(Vec<StripPresetDto>),
    /// `save_strip_preset`, `delete_strip_preset`.
    StripPresetsSet(StripPresetsSetDto),
    EqResponse(Vec<EqPointDto>),
}

/// The desk's reply: its answer, or the reason it refused, in its own words.
pub type Reply = Result<Answer, String>;

/// A type an answer carries, so a flow can ask for it by type
/// ([crate::console::Console::call]).
pub trait FromAnswer: Sized {
    /// The value `answer` carries, or the answer back when it carries something else.
    fn from_answer(answer: Answer) -> Result<Self, Answer>;
}

impl FromAnswer for () {
    fn from_answer(answer: Answer) -> Result<Self, Answer> {
        match answer {
            Answer::Done => Ok(()),
            other => Err(other),
        }
    }
}

impl FromAnswer for Answer {
    fn from_answer(answer: Answer) -> Result<Self, Answer> {
        Ok(answer)
    }
}

macro_rules! carried {
    ($($variant:ident($carried:ty)),* $(,)?) => {
        $(
            impl FromAnswer for $carried {
                fn from_answer(answer: Answer) -> Result<Self, Answer> {
                    match answer {
                        Answer::$variant(value) => Ok(value),
                        other => Err(other),
                    }
                }
            }
        )*
    };
}

carried! {
    Started(StartSummary),
    EngineState(EngineState),
    InputDevices(Vec<InputDeviceDto>),
    Address(String),
    Mix(MixDto),
    OutputDevices(Vec<OutputDeviceDto>),
    LocalOutputSet(LocalOutputSetDto),
    GroupsSet(GroupsSetDto),
    ChannelsSet(ChannelsSetDto),
    ChannelTable(Vec<SessionChannel>),
    EngineerControlsSet(EngineerControlsSetDto),
    AudioSettings(Option<SessionAudio>),
    AudioSaved(SessionAudio),
    Presets(Vec<SessionPreset>),
    PresetsSet(PresetsDto),
    Mixes(MixesSetDto),
    MirrorSet(MirrorSetDto),
    SessionNotice(Option<String>),
    Sessions(Vec<SnapshotSummary>),
    SessionSaved(SessionSavedDto),
    SessionRenamed(SnapshotSummary),
    SessionLoaded(SessionLoadedDto),
    RecordingSettings(RecordingSettingsDto),
    RecordingStarted(RecordingStartedDto),
    RecordingStopped(RecordingStoppedDto),
    Marker(TakeMarkerDto),
    Estimate(DiskEstimateDto),
    Takes(TakesDto),
    Soundcheck(SoundcheckDto),
    LogLocation(LogLocation),
    MixBackups(Vec<MixBackupSummary>),
    MixRestored(MixRestoredDto),
    LinkHistory(LinkHistoryDto),
    BufferSet(BufferSetDto),
    RedundancySet(RedundancySetDto),
    LimiterSet(LimiterSetDto),
    ProcessingSet(ProcessingSetDto),
    EffectSet(EffectSetDto),
    FxReturnSet(FxReturnSetDto),
    Processing(Vec<ChannelProcessingDto>),
    ChannelProcessing(ChannelProcessingDto),
    StripPresets(Vec<StripPresetDto>),
    StripPresetsSet(StripPresetsSetDto),
    EqResponse(Vec<EqPointDto>),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every command the Tauri shell registered (`src-tauri/src/lib.rs`, `generate_handler!`), in
    /// its order.
    const TAURI_COMMANDS: [&str; 67] = [
        "start_engine",
        "engine_status",
        "stop_engine",
        "list_input_devices",
        "switch_device",
        "add_target",
        "remove_target",
        "set_mix",
        "set_local_mix",
        "output_devices",
        "set_local_output",
        "set_musician_name",
        "set_groups",
        "set_channels",
        "channel_table",
        "set_engineer_controls",
        "audio_settings",
        "set_audio_settings",
        "presets",
        "save_preset",
        "rename_preset",
        "delete_preset",
        "apply_preset",
        "copy_mix",
        "set_mirror",
        "session_notice",
        "sessions",
        "save_session",
        "rename_session",
        "delete_session",
        "load_session",
        "recording_settings",
        "set_recording_settings",
        "start_recording",
        "stop_recording",
        "add_marker",
        "record_estimate",
        "list_takes",
        "open_take_folder",
        "start_soundcheck",
        "pause_soundcheck",
        "resume_soundcheck",
        "seek_soundcheck",
        "set_soundcheck_loop",
        "stop_soundcheck",
        "log_location",
        "list_mix_backups",
        "restore_mix_backup",
        "link_history",
        "set_musician_buffer",
        "set_musician_redundancy",
        "set_musician_limiter",
        "set_local_limiter",
        "set_channel_send",
        "set_effect",
        "set_musician_fx_return",
        "set_local_fx_return",
        "processing",
        "channel_processing",
        "set_channel_processing",
        "reset_channel_processing",
        "copy_channel_processing",
        "strip_presets",
        "save_strip_preset",
        "delete_strip_preset",
        "apply_strip_preset",
        "eq_response",
    ];

    /// One command of every kind, in the shell's order.
    pub(crate) fn every_command() -> Vec<Command> {
        let text = String::new;
        vec![
            Command::StartEngine(StartRequest::default()),
            Command::EngineStatus,
            Command::StopEngine,
            Command::ListInputDevices { host: None },
            Command::SwitchDevice { device: text() },
            Command::AddTarget { address: text() },
            Command::RemoveTarget { address: text() },
            Command::SetMix {
                address: text(),
                mix: MixUpdateRequest::default(),
            },
            Command::SetLocalMix {
                mix: MixUpdateRequest::default(),
            },
            Command::OutputDevices,
            Command::SetLocalOutput { output: None },
            Command::SetMusicianName {
                address: text(),
                name: text(),
            },
            Command::SetGroups { groups: Vec::new() },
            Command::SetChannels {
                channels: Vec::new(),
            },
            Command::ChannelTable,
            Command::SetEngineerControls {
                address: text(),
                client_id: None,
                controls: EngineerControlsRequest::default(),
            },
            Command::AudioSettings,
            Command::SetAudioSettings {
                settings: AudioSettingsRequest {
                    host: "auto".to_owned(),
                    device: None,
                    buffer_frames: None,
                    asio_driver: None,
                },
            },
            Command::Presets,
            Command::SavePreset {
                address: text(),
                name: text(),
            },
            Command::RenamePreset {
                id: text(),
                name: text(),
            },
            Command::DeletePreset { id: text() },
            Command::ApplyPreset {
                id: text(),
                musicians: Vec::new(),
            },
            Command::CopyMix {
                from: text(),
                to: Vec::new(),
            },
            Command::SetMirror {
                address: text(),
                client_id: None,
                leader: None,
            },
            Command::SessionNotice,
            Command::Sessions,
            Command::SaveSession { name: text() },
            Command::RenameSession {
                id: text(),
                name: text(),
            },
            Command::DeleteSession { id: text() },
            Command::LoadSession { id: text() },
            Command::RecordingSettings,
            Command::SetRecordingSettings {
                settings: RecordingSettingsRequest::default(),
            },
            Command::StartRecording {
                take_name: text(),
                utc_offset_minutes: None,
            },
            Command::StopRecording,
            Command::AddMarker { name: text() },
            Command::RecordEstimate,
            Command::ListTakes,
            Command::OpenTakeFolder { folder: text() },
            Command::StartSoundcheck {
                take_folder: text(),
                start_frame: None,
                looping: None,
            },
            Command::PauseSoundcheck,
            Command::ResumeSoundcheck,
            Command::SeekSoundcheck { frame: 0 },
            Command::SetSoundcheckLoop { looping: false },
            Command::StopSoundcheck,
            Command::LogLocation,
            Command::ListMixBackups,
            Command::RestoreMixBackup { stamp: text() },
            Command::LinkHistory {
                musician_key: text(),
                since_seq: None,
            },
            Command::SetMusicianBuffer {
                musician_key: text(),
                mode: "auto".to_owned(),
                ms: None,
            },
            Command::SetMusicianRedundancy {
                musician_key: text(),
                on: false,
                delay_blocks: None,
            },
            Command::SetMusicianLimiter {
                musician_key: text(),
                settings: LimiterSettings::default(),
            },
            Command::SetLocalLimiter {
                settings: LimiterSettings::default(),
            },
            Command::SetChannelSend {
                channel: 1,
                effect: Effect::Reverb,
                level: 0,
            },
            Command::SetEffect {
                effect: Effect::Reverb,
                settings: serde_json::json!({}),
            },
            Command::SetMusicianFxReturn {
                musician_key: text(),
                effect: Effect::Reverb,
                level: 0,
            },
            Command::SetLocalFxReturn {
                effect: Effect::Delay,
                level: 0,
            },
            Command::Processing,
            Command::ChannelProcessing { channel: 1 },
            Command::SetChannelProcessing {
                channel: 1,
                settings: StripSettings::default(),
            },
            Command::ResetChannelProcessing { channel: 1 },
            Command::CopyChannelProcessing {
                from: 1,
                to: Vec::new(),
            },
            Command::StripPresets,
            Command::SaveStripPreset {
                name: text(),
                settings: StripSettings::default(),
            },
            Command::DeleteStripPreset { name: text() },
            Command::ApplyStripPreset {
                name: text(),
                channel: 1,
            },
            Command::EqResponse {
                settings: EqSettings::default(),
                points: None,
            },
        ]
    }

    #[test]
    fn the_commands_are_the_67_the_tauri_shell_registered() {
        let names: Vec<_> = every_command().iter().map(Command::name).collect();
        assert_eq!(names, TAURI_COMMANDS);
    }

    #[test]
    fn a_flow_asks_for_an_answer_by_the_type_it_carries() {
        assert_eq!(<()>::from_answer(Answer::Done).ok(), Some(()));
        assert_eq!(
            Option::<String>::from_answer(Answer::SessionNotice(Some("kept".to_owned()))).ok(),
            Some(Some("kept".to_owned()))
        );
        assert!(Vec::<SessionPreset>::from_answer(Answer::Done).is_err());
    }
}
