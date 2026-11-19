//! The port the console's commands go out through ([Desk]), and its implementation over the desktop
//! core's commands: the same calls the Tauri shell made, with the same arguments.

use bandear_desktop_core::commands::{self, DesktopState};
use bandear_desktop_core::logfile;

use crate::command::{Answer, Command, Reply};
use crate::wire::EngineState;

/// Runs the console's commands. The app runs it on a background thread, so it must be shareable.
pub trait Desk: Send + Sync {
    fn run(&self, command: Command) -> Reply;
}

impl Desk for DesktopState {
    fn run(&self, command: Command) -> Reply {
        execute(self, command)
    }
}

/// Runs `command` on the desk `state`, as the Tauri shell's command of the same name did.
pub fn execute(state: &DesktopState, command: Command) -> Reply {
    use bandear_desktop_core::commands::{
        backups, effects, limiter, link_history, local, playback_buffer, processing, recording,
        redundancy, soundcheck,
    };
    Ok(match command {
        Command::StartEngine(request) => Answer::Started(commands::start_engine(state, request)?),
        Command::EngineStatus => Answer::EngineState(engine_state(state)?),
        Command::StopEngine => {
            commands::stop_engine(state)?;
            Answer::Done
        }
        Command::ListInputDevices { host } => {
            Answer::InputDevices(commands::list_input_devices(host)?)
        }
        Command::SwitchDevice { device } => {
            Answer::Started(commands::switch_device(state, device)?)
        }
        Command::AddTarget { address } => Answer::Address(commands::add_target(state, address)?),
        Command::RemoveTarget { address } => {
            Answer::Address(commands::remove_target(state, address)?)
        }
        Command::SetMix { address, mix } => Answer::Mix(commands::set_mix(state, address, mix)?),
        Command::SetLocalMix { mix } => Answer::Mix(local::set_local_mix(state, mix)?),
        Command::OutputDevices => Answer::OutputDevices(local::output_devices(state)?),
        Command::SetLocalOutput { output } => {
            Answer::LocalOutputSet(local::set_local_output(state, output)?)
        }
        Command::SetMusicianName { address, name } => {
            commands::set_musician_name(state, address, name)?;
            Answer::Done
        }
        Command::SetGroups { groups } => Answer::GroupsSet(commands::set_groups(state, groups)?),
        Command::SetChannels { channels } => {
            Answer::ChannelsSet(commands::set_channels(state, channels)?)
        }
        Command::ChannelTable => Answer::ChannelTable(commands::channel_table(state)?),
        Command::SetEngineerControls {
            address,
            client_id,
            controls,
        } => Answer::EngineerControlsSet(commands::set_engineer_controls(
            state, address, client_id, controls,
        )?),
        Command::AudioSettings => Answer::AudioSettings(commands::audio_settings(state)),
        Command::SetAudioSettings { settings } => {
            Answer::AudioSaved(commands::set_audio_settings(state, settings)?)
        }
        Command::Presets => Answer::Presets(commands::presets(state)),
        Command::SavePreset { address, name } => {
            Answer::PresetsSet(commands::save_preset(state, address, name)?)
        }
        Command::RenamePreset { id, name } => {
            Answer::PresetsSet(commands::rename_preset(state, id, name)?)
        }
        Command::DeletePreset { id } => Answer::PresetsSet(commands::delete_preset(state, id)?),
        Command::ApplyPreset { id, musicians } => {
            Answer::Mixes(commands::apply_preset(state, id, musicians)?)
        }
        Command::CopyMix { from, to } => Answer::Mixes(commands::copy_mix(state, from, to)?),
        Command::SetMirror {
            address,
            client_id,
            leader,
        } => Answer::MirrorSet(commands::set_mirror(state, address, client_id, leader)?),
        Command::SessionNotice => Answer::SessionNotice(commands::session_notice(state)),
        Command::Sessions => Answer::Sessions(commands::sessions(state)?),
        Command::SaveSession { name } => Answer::SessionSaved(commands::save_session(state, name)?),
        Command::RenameSession { id, name } => {
            Answer::SessionRenamed(commands::rename_session(state, id, name)?)
        }
        Command::DeleteSession { id } => {
            commands::delete_session(state, id)?;
            Answer::Done
        }
        Command::LoadSession { id } => Answer::SessionLoaded(commands::load_session(state, id)?),
        Command::RecordingSettings => {
            Answer::RecordingSettings(recording::recording_settings(state)?)
        }
        Command::SetRecordingSettings { settings } => {
            Answer::RecordingSettings(recording::set_recording_settings(state, settings)?)
        }
        Command::StartRecording {
            take_name,
            utc_offset_minutes,
        } => Answer::RecordingStarted(recording::start_recording(
            state,
            take_name,
            utc_offset_minutes,
        )?),
        Command::StopRecording => Answer::RecordingStopped(recording::stop_recording(state)?),
        Command::AddMarker { name } => Answer::Marker(recording::add_marker(state, name)?),
        Command::RecordEstimate => Answer::Estimate(recording::record_estimate(state)?),
        Command::ListTakes => Answer::Takes(recording::list_takes(state)?),
        Command::OpenTakeFolder { folder } => {
            recording::open_take_folder(state, folder)?;
            Answer::Done
        }
        Command::StartSoundcheck {
            take_folder,
            start_frame,
            looping,
        } => Answer::Soundcheck(soundcheck::start_soundcheck(
            state,
            take_folder,
            start_frame,
            looping,
        )?),
        Command::PauseSoundcheck => Answer::Soundcheck(soundcheck::pause_soundcheck(state)?),
        Command::ResumeSoundcheck => Answer::Soundcheck(soundcheck::resume_soundcheck(state)?),
        Command::SeekSoundcheck { frame } => {
            Answer::Soundcheck(soundcheck::seek_soundcheck(state, frame)?)
        }
        Command::SetSoundcheckLoop { looping } => {
            Answer::Soundcheck(soundcheck::set_soundcheck_loop(state, looping)?)
        }
        Command::StopSoundcheck => Answer::Soundcheck(soundcheck::stop_soundcheck(state)?),
        Command::LogLocation => Answer::LogLocation(
            logfile::location()
                .ok_or_else(|| "the log file is not set up: lines go to stderr".to_owned())?,
        ),
        Command::ListMixBackups => Answer::MixBackups(backups::list_mix_backups(state)?),
        Command::RestoreMixBackup { stamp } => {
            Answer::MixRestored(backups::restore_mix_backup(state, stamp)?)
        }
        Command::LinkHistory {
            musician_key,
            since_seq,
        } => Answer::LinkHistory(link_history::link_history(state, &musician_key, since_seq)?),
        Command::SetMusicianBuffer {
            musician_key,
            mode,
            ms,
        } => Answer::BufferSet(playback_buffer::set_musician_buffer(
            state,
            &musician_key,
            &mode,
            ms,
        )?),
        Command::SetMusicianRedundancy {
            musician_key,
            on,
            delay_blocks,
        } => Answer::RedundancySet(redundancy::set_musician_redundancy(
            state,
            &musician_key,
            on,
            delay_blocks,
        )?),
        Command::SetMusicianLimiter {
            musician_key,
            settings,
        } => Answer::LimiterSet(limiter::set_musician_limiter(
            state,
            &musician_key,
            settings,
        )?),
        Command::SetLocalLimiter { settings } => {
            Answer::LimiterSet(limiter::set_local_limiter(state, settings)?)
        }
        Command::SetChannelSend {
            channel,
            effect,
            level,
        } => Answer::ProcessingSet(processing::set_channel_send(state, channel, effect, level)?),
        Command::SetEffect { effect, settings } => {
            Answer::EffectSet(effects::set_effect(state, effect, settings)?)
        }
        Command::SetMusicianFxReturn {
            musician_key,
            effect,
            level,
        } => Answer::FxReturnSet(effects::set_musician_fx_return(
            state,
            &musician_key,
            effect,
            level,
        )?),
        Command::SetLocalFxReturn { effect, level } => {
            Answer::FxReturnSet(effects::set_local_fx_return(state, effect, level)?)
        }
        Command::Processing => Answer::Processing(processing::processing(state)?),
        Command::ChannelProcessing { channel } => {
            Answer::ChannelProcessing(processing::channel_processing(state, channel)?)
        }
        Command::SetChannelProcessing { channel, settings } => Answer::ProcessingSet(
            processing::set_channel_processing(state, channel, settings)?,
        ),
        Command::ResetChannelProcessing { channel } => {
            Answer::ProcessingSet(processing::reset_channel_processing(state, channel)?)
        }
        Command::CopyChannelProcessing { from, to } => {
            Answer::ProcessingSet(processing::copy_channel_processing(state, from, to)?)
        }
        Command::StripPresets => Answer::StripPresets(processing::strip_presets(state)),
        Command::SaveStripPreset { name, settings } => {
            Answer::StripPresetsSet(processing::save_strip_preset(state, name, settings)?)
        }
        Command::DeleteStripPreset { name } => {
            Answer::StripPresetsSet(processing::delete_strip_preset(state, name)?)
        }
        Command::ApplyStripPreset { name, channel } => {
            Answer::ProcessingSet(processing::apply_strip_preset(state, name, channel)?)
        }
        Command::EqResponse { settings, points } => {
            Answer::EqResponse(processing::eq_response(&settings, points))
        }
    })
}

/// The `engine_status` answer as the console reads it: the desk's JSON, read as the `status`
/// event is.
fn engine_state(state: &DesktopState) -> Result<EngineState, String> {
    let answer = commands::engine_state(state)?;
    let value = serde_json::to_value(&answer)
        .map_err(|error| format!("the engine's status could not be read: {error}"))?;
    EngineState::from_json(value)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use bandear_desktop_core::session::SessionStore;

    use super::*;

    /// A desk with no engine, its session in a scratch folder outside the repository.
    fn desk(test: &str) -> DesktopState {
        let folder: PathBuf = std::env::temp_dir()
            .join("bandear-console-desk-tests")
            .join(test);
        drop(std::fs::remove_dir_all(&folder));
        DesktopState::new(SessionStore::open(&folder))
    }

    #[test]
    fn a_desk_with_no_engine_answers_from_its_session() {
        let desk = desk("session");
        assert!(matches!(
            Desk::run(&desk, Command::EngineStatus),
            Ok(Answer::EngineState(EngineState::NotRunning { run: 0 }))
        ));
        assert!(matches!(
            Desk::run(&desk, Command::AudioSettings),
            Ok(Answer::AudioSettings(None))
        ));
        assert!(
            matches!(Desk::run(&desk, Command::Presets), Ok(Answer::Presets(presets)) if presets.is_empty())
        );
        assert!(matches!(
            Desk::run(&desk, Command::SessionNotice),
            Ok(Answer::SessionNotice(_))
        ));
        assert!(matches!(
            Desk::run(&desk, Command::Sessions),
            Ok(Answer::Sessions(_))
        ));
        assert!(
            matches!(Desk::run(&desk, Command::StripPresets), Ok(Answer::StripPresets(presets)) if !presets.is_empty())
        );
        assert!(matches!(
            Desk::run(&desk, Command::Processing),
            Ok(Answer::Processing(_))
        ));
        assert!(matches!(
            Desk::run(&desk, Command::ListMixBackups),
            Ok(Answer::MixBackups(_))
        ));
        assert!(matches!(
            Desk::run(&desk, Command::RecordingSettings),
            Ok(Answer::RecordingSettings(_))
        ));
        assert!(matches!(
            Desk::run(&desk, Command::EqResponse { settings: Default::default(), points: Some(8) }),
            Ok(Answer::EqResponse(points)) if points.len() == 8
        ));
    }

    #[test]
    fn a_command_that_needs_an_engine_is_refused_in_the_desks_words() {
        let desk = desk("refused");
        let refused = Desk::run(
            &desk,
            Command::SetMix {
                address: "192.168.1.3:50000".to_owned(),
                mix: Default::default(),
            },
        );
        assert!(matches!(refused, Err(message) if !message.is_empty()));
    }
}
