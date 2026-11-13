//! The folders the desktop app keeps its things in, named exactly as Tauri names them.
//!
//! The session, the named sessions and the mix backups live in the config folder, the log in the
//! log folder, and takes go to the user's Music folder by default (see
//! [crate::commands::recording]). The desk ran on Tauri first, so whatever shell draws the window
//! must find them where Tauri 2.12's path resolver put them for [APP_IDENTIFIER], with no folder
//! override configured: `app_config_dir`, `app_local_data_dir`, `app_log_dir` and `audio_dir`,
//! which name them from the system's own folders through the same `dirs` crate. A path that
//! differs by one byte is a desk that seems to have lost its sessions, backups and log.

use std::path::PathBuf;

/// The desk's identifier, the name of its own folders (`identifier` in `tauri.conf.json`).
pub const APP_IDENTIFIER: &str = "com.bandear.desktop";

/// The app's own folders.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppFolders {
    /// `<config folder>/<identifier>`: the session, the named sessions and the mix backups.
    pub config: PathBuf,
    /// `<local data folder>/<identifier>`, where takes go when the Music folder cannot be found;
    /// `None` when the system has no local data folder.
    pub local_data: Option<PathBuf>,
    /// `~/Library/Logs/<identifier>` on macOS, `<local data folder>/<identifier>/logs` elsewhere;
    /// `None` when the folder it is named from is unknown, and then the log stays on stderr.
    pub log: Option<PathBuf>,
    /// The user's Music folder, when the system has one.
    pub music: Option<PathBuf>,
}

/// The system's own folders the app's are named from, as the `dirs` crate finds them; `None` for
/// one it cannot find.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SystemFolders {
    pub config: Option<PathBuf>,
    pub data_local: Option<PathBuf>,
    pub home: Option<PathBuf>,
    pub audio: Option<PathBuf>,
}

impl SystemFolders {
    /// This user's folders on this machine.
    pub fn current() -> Self {
        Self {
            config: dirs::config_dir(),
            data_local: dirs::data_local_dir(),
            home: dirs::home_dir(),
            audio: dirs::audio_dir(),
        }
    }
}

impl AppFolders {
    /// The desk's folders on this machine, for [APP_IDENTIFIER]. Fails only when the system has
    /// no config folder, as Tauri's `app_config_dir` does.
    pub fn resolve() -> Result<Self, String> {
        Self::from_system(APP_IDENTIFIER, &SystemFolders::current())
    }

    /// The folders of the app `identifier` names, from the system's `system`, laid out as this
    /// platform's Tauri lays them out.
    pub fn from_system(identifier: &str, system: &SystemFolders) -> Result<Self, String> {
        named(identifier, system, cfg!(target_os = "macos"))
    }
}

/// [AppFolders::from_system] with the macOS layout of the log folder or the other platforms'.
fn named(identifier: &str, system: &SystemFolders, macos: bool) -> Result<AppFolders, String> {
    let config = system
        .config
        .as_ref()
        .map(|folder| folder.join(identifier))
        .ok_or_else(|| "the system has no config folder for the app's session".to_owned())?;
    // Joined exactly as Tauri joins them (`Library/Logs` in one step): the same bytes.
    let log = if macos {
        system
            .home
            .as_ref()
            .map(|home| home.join("Library/Logs").join(identifier))
    } else {
        system
            .data_local
            .as_ref()
            .map(|folder| folder.join(identifier).join("logs"))
    };
    Ok(AppFolders {
        config,
        local_data: system
            .data_local
            .as_ref()
            .map(|folder| folder.join(identifier)),
        log,
        music: system.audio.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn system() -> SystemFolders {
        SystemFolders {
            config: Some(PathBuf::from("/home/ana/.config")),
            data_local: Some(PathBuf::from("/home/ana/.local/share")),
            home: Some(PathBuf::from("/home/ana")),
            audio: Some(PathBuf::from("/home/ana/Music")),
        }
    }

    #[test]
    fn each_folder_is_named_after_the_identifier_as_tauri_names_it() {
        let folders = named("com.bandear.desktop", &system(), false).expect("folders");

        assert_eq!(
            folders,
            AppFolders {
                config: PathBuf::from("/home/ana/.config/com.bandear.desktop"),
                local_data: Some(PathBuf::from("/home/ana/.local/share/com.bandear.desktop")),
                log: Some(PathBuf::from(
                    "/home/ana/.local/share/com.bandear.desktop/logs"
                )),
                music: Some(PathBuf::from("/home/ana/Music")),
            }
        );
    }

    #[test]
    fn on_macos_the_log_folder_is_in_the_home_library_whatever_the_local_data_folder() {
        let folders = named("com.bandear.desktop", &system(), true).expect("folders");

        assert_eq!(
            folders.log,
            Some(PathBuf::from("/home/ana/Library/Logs/com.bandear.desktop"))
        );
        let homeless = SystemFolders {
            home: None,
            ..system()
        };
        assert_eq!(
            named("com.bandear.desktop", &homeless, true)
                .expect("folders")
                .log,
            None,
            "no home, no log folder, even with a local data folder"
        );
    }

    #[test]
    fn elsewhere_the_log_folder_is_in_the_local_data_folder_whatever_the_home() {
        let without_local_data = SystemFolders {
            data_local: None,
            ..system()
        };

        let folders = named("com.bandear.desktop", &without_local_data, false).expect("folders");

        assert_eq!(folders.log, None, "no local data folder, no log folder");
        assert_eq!(folders.local_data, None);
        assert_eq!(
            folders.config,
            PathBuf::from("/home/ana/.config/com.bandear.desktop")
        );
    }

    #[test]
    fn a_system_without_a_music_folder_leaves_it_out() {
        let without_music = SystemFolders {
            audio: None,
            ..system()
        };

        assert_eq!(
            named("com.bandear.desktop", &without_music, false)
                .expect("folders")
                .music,
            None
        );
    }

    #[test]
    fn with_no_config_folder_there_are_no_folders() {
        let without_config = SystemFolders {
            config: None,
            ..system()
        };

        for macos in [false, true] {
            let error =
                named("com.bandear.desktop", &without_config, macos).expect_err("no folders");
            assert!(error.contains("config folder"), "{error}");
        }
    }

    /// The folders Tauri's documentation gives on macOS, which every desk that ran there has.
    #[cfg(target_os = "macos")]
    #[test]
    fn on_this_mac_the_folders_are_the_ones_the_tauri_desk_used() {
        let home = dirs::home_dir().expect("a home folder");

        let folders = AppFolders::resolve().expect("folders");

        let support = home.join("Library/Application Support/com.bandear.desktop");
        assert_eq!(folders.config, support);
        assert_eq!(folders.local_data, Some(support));
        assert_eq!(
            folders.log,
            Some(home.join("Library/Logs/com.bandear.desktop"))
        );
        assert_eq!(folders.music, Some(home.join("Music")));
    }
}
