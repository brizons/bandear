//! What the desktop app does as it starts and as it quits, whatever shell draws its window.
//!
//! A shell finds the app's [AppFolders] (Tauri through its own path resolver, any other through
//! [AppFolders::resolve], which names them the same), opens the desk with [open], adds its event
//! pump with [DesktopState::with_event_pump], and calls [exit] once as the app quits.

use personal_monitoring::{log_info, log_warn};

use crate::commands::{self, DesktopState};
use crate::folders::AppFolders;
use crate::session::SessionStore;
use crate::{logfile, migration};

/// Opens the desk the app `version` runs in `folders`: the log first, then the default recordings
/// folder, the MixLink desk's data brought over on a first start, and the session. Never fails:
/// a missing log folder leaves the lines on stderr, and a damaged session is set aside and logged.
pub fn open(folders: &AppFolders, version: &str) -> DesktopState {
    // First, so every later line (the engine's too) reaches the log file: a release build has no
    // console. Without a log folder the lines stay on stderr.
    match &folders.log {
        Some(log_directory) => {
            let log_file = logfile::install(log_directory.clone());
            log_info!(
                "app",
                "BandEar desktop {} on {} {}; log file \"{}\"",
                version,
                std::env::consts::OS,
                std::env::consts::ARCH,
                log_file.display()
            );
        }
        // The folder it is named from being unknown is the only reason there is none, and the
        // line says so as it did when Tauri's resolver answered.
        None => log_warn!(
            "app",
            "no log folder (unknown path); logging to stderr only"
        ),
    }
    // The session (musician labels, the engineer's channel details, groups and controls, the audio
    // input, and the local mix and its output) is a desktop-app concern and lives in the app's
    // config folder, so it survives a restart without touching the engine or any client protocol.
    // Opening it never fails: a damaged file is set aside and logged. The engineer's named
    // sessions live beside it, in `sessions`.
    let directory = &folders.config;
    // Takes go to the user's Music folder + BandEar\Recordings by default; when it cannot be
    // found, to the app's own data folder, and the recording settings say so. Never inside the
    // config folder, which the data folder is on macOS: then the next place.
    let recordings = commands::recording::default_recordings_folder(
        folders.music.clone(),
        folders.local_data.clone(),
        directory,
    );
    if let Some(notice) = &recordings.notice {
        log_warn!("app", "{notice}");
    }
    // A desk that ran as MixLink (identifier com.mixlink.desktop) has its session, named sessions
    // and backups in the old folders: on the first start they are copied here, before the session
    // is opened, and takes in its default folder stay listed.
    migration::bring_over_and_log(&migration::DeskFolders {
        config: directory.clone(),
        local_data: folders.local_data.clone(),
        music: folders.music.clone(),
        recordings: recordings.folder.clone(),
    });
    DesktopState::new(SessionStore::open(directory)).with_recordings_folder(recordings)
}

/// What the app does as it quits, given its state once it has one: the local mix and the channel
/// processing still waiting to be kept (both are kept off the engine lock, a few times a second at
/// most) are written before the app goes, so is a backup of the musicians' mixes as the engine has
/// them, and then the log is flushed so its last lines stay.
pub fn exit(state: Option<&DesktopState>) {
    if let Some(state) = state {
        commands::local::flush_local_mix(state);
        commands::processing::flush_processing(state);
        commands::backups::flush_mix_backup(state);
    }
    log_info!("app", "exiting");
    personal_monitoring::logging::flush();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    use crate::migration::{LEGACY_IDENTIFIER, MIGRATION_MARKER};
    use crate::session::{SessionAudio, SessionMix, SESSION_FILE};

    /// A scratch home with the folders Tauri gives the desk, and no log folder: a test must not
    /// send the whole test run's lines to a file.
    fn folders(test: &str) -> (PathBuf, AppFolders) {
        let root = std::env::temp_dir()
            .join("bandear-lifecycle-tests")
            .join(test);
        let _ = fs::remove_dir_all(&root);
        let folders = AppFolders {
            config: root.join("Roaming").join("com.bandear.desktop"),
            local_data: Some(root.join("Local").join("com.bandear.desktop")),
            log: None,
            music: Some(root.join("Music")),
        };
        (root, folders)
    }

    #[test]
    fn the_desk_records_into_the_music_folder_by_default() {
        let (root, folders) = folders("recordings");

        let state = open(&folders, "0.3.0");

        let settings = commands::recording::recording_settings(&state).expect("settings");
        assert_eq!(
            PathBuf::from(settings.default_folder),
            root.join("Music").join("BandEar").join("Recordings")
        );
        assert_eq!(settings.notice, None);
    }

    #[test]
    fn a_mixlink_desk_is_brought_over_before_its_session_is_opened() {
        let (root, folders) = folders("migration");
        let old = root.join("Roaming").join(LEGACY_IDENTIFIER);
        fs::create_dir_all(&old).expect("old config folder");
        fs::write(
            old.join(SESSION_FILE),
            r#"{"version":1,"audio":{"host":"demo","bufferFrames":256}}"#,
        )
        .expect("old session");

        let state = open(&folders, "0.3.0");

        assert!(folders.config.join(MIGRATION_MARKER).exists());
        assert_eq!(
            commands::audio_settings(&state),
            Some(SessionAudio {
                host: "demo".to_owned(),
                device: None,
                buffer_frames: Some(256),
                asio_driver: None,
            }),
            "the session opened is the one brought over"
        );
    }

    fn mix(volume_percent: u8) -> SessionMix {
        SessionMix {
            channels: Vec::new(),
            groups: Default::default(),
            volume_percent,
            muted: false,
        }
    }

    #[test]
    fn quitting_writes_the_local_mix_still_waiting() {
        let (_root, folders) = folders("exit");
        // The state's saver writes only when flushed, so whatever reaches the session was written
        // by the exit.
        let _manual = commands::local::saver::manual_savers();
        let state = DesktopState::new(SessionStore::open(&folders.config));
        state.mark_local_mix(mix(10));
        assert_eq!(state.kept_local(), None, "nothing is written before");

        exit(Some(&state));

        assert_eq!(
            state
                .kept_local()
                .and_then(|local| local.mix)
                .map(|mix| mix.volume_percent),
            Some(10)
        );
        assert_eq!(
            SessionStore::open(&folders.config)
                .local()
                .and_then(|local| local.mix)
                .map(|mix| mix.volume_percent),
            Some(10),
            "and it is on the disk"
        );
        exit(None);
    }
}
