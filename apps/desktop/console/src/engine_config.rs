//! The engine's start request and the audio choices it is built from (`engine-config.js`).
//!
//! The request is the one the first window sent: nobody configured (phones join on their own or
//! are added by address), the default control port, no groups, the chosen driver and buffer. The
//! driver is the system's own ([Host::Auto]) unless the engineer chose another: ASIO is never picked
//! by default, and an ASIO request always names the driver the engineer picked; without one there
//! is no request at all, since the engine would load the first ASIO driver it finds, a kernel-mode
//! driver nobody chose.
//!
//! A plain start reopens the input picked for the driver ([remembered_device]). The desk keeps the
//! choices in its session, and the window starts from them ([restored_audio]); when the input
//! they name is gone, a start gives way to the system's default input on the same driver, or on the
//! system's own when the choice was ASIO ([fallback_audio]), never on ASIO. The demo source
//! ([Host::Demo]) is chosen only by the engineer; nothing here ever picks it on its own.

use bandear_desktop_core::dto::{AudioSettingsRequest, StartRequest};
use bandear_desktop_core::session::SessionAudio;

/// An audio driver the engine knows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Host {
    /// The system's own driver.
    Auto,
    Wasapi,
    Asio,
    /// The demo source: a generated band, no interface.
    Demo,
}

/// The drivers, in the order the audio setup offers them.
pub const HOSTS: [Host; 4] = [Host::Auto, Host::Wasapi, Host::Asio, Host::Demo];

/// The control port of every start.
pub const DEFAULT_CONTROL_PORT: u16 = 50001;

/// Why no ASIO start can be built: no driver was picked.
pub const ASIO_NEEDS_DRIVER: &str =
    "ASIO is chosen but no ASIO driver is picked: pick one under Network and system.";

impl Host {
    /// The driver's name, as the desk names it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Wasapi => "wasapi",
            Self::Asio => "asio",
            Self::Demo => "demo",
        }
    }
}

/// The driver `host` names, trimmed and in any case; anything else is the system's own.
pub fn normalize_host(host: Option<&str>) -> Host {
    let value = host.unwrap_or("").trim().to_lowercase();
    HOSTS
        .into_iter()
        .find(|host| host.name() == value)
        .unwrap_or(Host::Auto)
}

/// A buffer size in whole frames, or `None` for the device's own.
pub fn normalize_buffer(frames: Option<f64>) -> Option<u32> {
    match frames {
        Some(frames) if frames.fract() == 0.0 && frames > 0.0 && frames <= f64::from(u32::MAX) => {
            Some(frames as u32)
        }
        _ => None,
    }
}

/// A buffer size written as text (a select's value), as [normalize_buffer] reads a number.
pub fn parse_buffer(frames: &str) -> Option<u32> {
    let text = frames.trim();
    if text.is_empty() {
        return None;
    }
    normalize_buffer(text.parse::<f64>().ok())
}

/// A device or driver name, trimmed; `None` for none.
fn normalize_name(name: Option<&str>) -> Option<String> {
    let name = name.unwrap_or("").trim();
    (!name.is_empty()).then(|| name.to_owned())
}

/// The audio choices: the driver, the input picked under it (`None` for the system's default), the
/// buffer (`None` for the device's own) and the picked ASIO driver.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Audio {
    pub host: Host,
    pub device: Option<String>,
    pub buffer_frames: Option<u32>,
    pub asio_driver: Option<String>,
}

/// The choices before the engineer makes any: the system's driver and its default input, 256
/// frames, no ASIO driver picked.
pub fn default_audio() -> Audio {
    Audio {
        host: Host::Auto,
        device: None,
        buffer_frames: Some(256),
        asio_driver: None,
    }
}

/// A change of the audio choices; a field left `None` keeps its value. `host` is any text (it is
/// normalized), and the inner `None`s clear a field.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AudioChange {
    pub host: Option<String>,
    pub device: Option<Option<String>>,
    pub buffer_frames: Option<Option<f64>>,
    pub asio_driver: Option<Option<String>>,
}

impl AudioChange {
    /// A change of every field to `audio`'s.
    pub fn to(audio: &Audio) -> Self {
        Self {
            host: Some(audio.host.name().to_owned()),
            device: Some(audio.device.clone()),
            buffer_frames: Some(audio.buffer_frames.map(f64::from)),
            asio_driver: Some(audio.asio_driver.clone()),
        }
    }
}

/// The choices with `change` applied. An input belongs to its driver, so a change of driver that
/// names no input starts on the new driver's default input.
pub fn audio_choice(current: &Audio, change: &AudioChange) -> Audio {
    let host = match &change.host {
        Some(host) => normalize_host(Some(host)),
        None => current.host,
    };
    // An input belongs to its driver: a new driver with no input named opens its default one.
    let keeps_device = change.device.is_some() || host == current.host;
    let device = match &change.device {
        Some(device) => normalize_name(device.as_deref()),
        None if keeps_device => current.device.clone(),
        None => None,
    };
    Audio {
        host,
        device,
        buffer_frames: match change.buffer_frames {
            Some(frames) => normalize_buffer(frames),
            None => current.buffer_frames,
        },
        asio_driver: match &change.asio_driver {
            Some(driver) => normalize_name(driver.as_deref()),
            None => current.asio_driver.clone(),
        },
    }
}

/// The input a plain start of `audio` opens: the picked ASIO driver under ASIO, else the picked
/// input; `None` for the system's default input (or no ASIO driver picked).
pub fn remembered_device(audio: &Audio) -> Option<String> {
    if audio.host == Host::Asio {
        normalize_name(audio.asio_driver.as_deref())
    } else {
        normalize_name(audio.device.as_deref())
    }
}

/// The choices the desk's session kept (`audio_settings`), as every start takes them: nothing kept
/// is the defaults, and a choice that names ASIO without a driver (an older or hand-edited file) is
/// no ASIO at all but the system's own driver, so no ASIO driver is ever loaded that the engineer
/// did not pick.
pub fn restored_audio(saved: Option<&SessionAudio>) -> Audio {
    let Some(saved) = saved else {
        return default_audio();
    };
    let audio = audio_choice(
        &default_audio(),
        &AudioChange {
            host: Some(saved.host.clone()),
            device: Some(saved.device.clone()),
            buffer_frames: Some(saved.buffer_frames.map(f64::from)),
            asio_driver: Some(saved.asio_driver.clone()),
        },
    );
    if audio.host == Host::Asio && audio.asio_driver.is_none() {
        Audio {
            host: Host::Auto,
            ..audio
        }
    } else {
        audio
    }
}

/// The choices a start falls back to when the input `audio` names is gone: the system's default
/// input on the same driver, or on the system's own driver when the choice was ASIO. The buffer
/// stays.
pub fn fallback_audio(audio: &Audio) -> Audio {
    Audio {
        host: if audio.host == Host::Asio {
            Host::Auto
        } else {
            audio.host
        },
        device: None,
        ..audio.clone()
    }
}

/// What the console says when a start fell back because the input `audio` names was missing.
pub fn fallback_notice(audio: &Audio) -> String {
    let device = remembered_device(audio).unwrap_or_default();
    if audio.host == Host::Asio {
        return format!(
            "The saved ASIO driver {device} was not found, so the engine started on the system's own \
             driver and its default input, never on another ASIO driver. The saved driver is kept and \
             tried again at the next start."
        );
    }
    format!(
        "The saved input {device} was not found, so the engine started on the system's default \
         input. The saved input is kept and tried again at the next start."
    )
}

/// The `start_engine` request for `audio`, on `device_filter` when named, else on the input picked
/// for the driver. Every start goes through here; under ASIO with no driver there is none.
pub fn start_request(audio: &Audio, device_filter: Option<&str>) -> Option<StartRequest> {
    let device = match device_filter {
        Some(device) => Some(device.to_owned()),
        None => remembered_device(audio),
    };
    if audio.host == Host::Asio && device.is_none() {
        return None;
    }
    Some(StartRequest {
        device_filter: device,
        targets: Vec::new(),
        control_port: Some(DEFAULT_CONTROL_PORT),
        groups: Vec::new(),
        host: Some(audio.host.name().to_owned()),
        buffer_frames: audio.buffer_frames,
    })
}

/// The `set_audio_settings` request that keeps `audio` in the desk's session.
pub fn settings_request(audio: &Audio) -> AudioSettingsRequest {
    AudioSettingsRequest {
        host: audio.host.name().to_owned(),
        device: audio.device.clone(),
        buffer_frames: audio.buffer_frames,
        asio_driver: audio.asio_driver.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VOLT: &str = "INPUT 1/2 (Volt 4)";

    fn audio(host: Host, device: Option<&str>, buffer: Option<u32>, asio: Option<&str>) -> Audio {
        Audio {
            host,
            device: device.map(str::to_owned),
            buffer_frames: buffer,
            asio_driver: asio.map(str::to_owned),
        }
    }

    fn saved(
        host: &str,
        device: Option<&str>,
        buffer: Option<u32>,
        asio: Option<&str>,
    ) -> SessionAudio {
        SessionAudio {
            host: host.to_owned(),
            device: device.map(str::to_owned),
            buffer_frames: buffer,
            asio_driver: asio.map(str::to_owned),
        }
    }

    fn summary(
        request: &StartRequest,
    ) -> (
        Option<String>,
        Vec<String>,
        Option<u16>,
        usize,
        Option<String>,
        Option<u32>,
    ) {
        (
            request.device_filter.clone(),
            request.targets.clone(),
            request.control_port,
            request.groups.len(),
            request.host.clone(),
            request.buffer_frames,
        )
    }

    fn change(host: Option<&str>) -> AudioChange {
        AudioChange {
            host: host.map(str::to_owned),
            ..AudioChange::default()
        }
    }

    #[test]
    fn the_default_start_is_the_old_windows_the_systems_own_driver_256_frames_nobody_added() {
        assert_eq!(default_audio(), audio(Host::Auto, None, Some(256), None));
        let request = start_request(&default_audio(), None).expect("a request");
        assert_eq!(
            summary(&request),
            (
                None,
                vec![],
                Some(50001),
                0,
                Some("auto".to_owned()),
                Some(256)
            )
        );
    }

    #[test]
    fn asio_starts_only_on_the_driver_the_engineer_picked_and_is_never_built_without_one() {
        let picked = audio(Host::Asio, None, Some(256), Some("Volt ASIO"));
        assert_eq!(
            start_request(&picked, None).unwrap().host.as_deref(),
            Some("asio")
        );
        assert_eq!(
            start_request(&picked, None)
                .unwrap()
                .device_filter
                .as_deref(),
            Some("Volt ASIO"),
            "a plain start names the driver"
        );
        assert_eq!(
            start_request(&picked, Some("Other ASIO"))
                .unwrap()
                .device_filter
                .as_deref(),
            Some("Other ASIO")
        );
        assert!(start_request(&audio(Host::Asio, None, Some(256), None), None).is_none());
        assert!(
            start_request(&audio(Host::Asio, Some(VOLT), Some(256), None), None).is_none(),
            "an input picked for another driver is no ASIO driver"
        );
        assert_eq!(normalize_host(Some("WASAPI")), Host::Wasapi);
        for host in [
            None,
            Some(""),
            Some("asio driver"),
            Some("kernel"),
            Some("3"),
        ] {
            assert_eq!(normalize_host(host), Host::Auto, "{host:?}");
        }
    }

    #[test]
    fn a_buffer_is_a_whole_number_of_frames_or_the_devices_own_when_there_is_none() {
        assert_eq!(normalize_buffer(Some(512.0)), Some(512));
        assert_eq!(parse_buffer("128"), Some(128));
        for frames in [None, Some(0.0), Some(-5.0), Some(12.5)] {
            assert_eq!(normalize_buffer(frames), None, "{frames:?}");
        }
        for frames in ["", "many", "12.5"] {
            assert_eq!(parse_buffer(frames), None, "{frames}");
        }
    }

    #[test]
    fn a_start_can_ask_for_an_input_device_by_name() {
        assert_eq!(
            start_request(&default_audio(), Some("Volt 4"))
                .unwrap()
                .device_filter
                .as_deref(),
            Some("Volt 4")
        );
    }

    #[test]
    fn a_plain_start_reopens_the_input_picked_for_the_driver_and_a_named_one_wins() {
        let volt = audio(Host::Wasapi, Some(VOLT), Some(256), Some("Volt ASIO"));
        assert_eq!(
            start_request(&volt, None).unwrap().device_filter.as_deref(),
            Some(VOLT),
            "not the system's default"
        );
        assert_eq!(
            start_request(&volt, None).unwrap().host.as_deref(),
            Some("wasapi")
        );
        let auto = Audio {
            host: Host::Auto,
            ..volt.clone()
        };
        assert_eq!(
            start_request(&auto, None).unwrap().device_filter.as_deref(),
            Some(VOLT)
        );
        assert_eq!(
            start_request(&volt, Some("Mic"))
                .unwrap()
                .device_filter
                .as_deref(),
            Some("Mic")
        );
        assert_eq!(remembered_device(&volt).as_deref(), Some(VOLT));
        let asio = Audio {
            host: Host::Asio,
            ..volt
        };
        assert_eq!(
            remembered_device(&asio).as_deref(),
            Some("Volt ASIO"),
            "under ASIO, its driver"
        );
        assert_eq!(
            remembered_device(&default_audio()),
            None,
            "the system's default"
        );
    }

    #[test]
    fn changing_one_audio_choice_keeps_the_other() {
        let wasapi = audio(Host::Wasapi, None, Some(128), None);
        let bigger = AudioChange {
            buffer_frames: Some(Some(512.0)),
            ..AudioChange::default()
        };
        assert_eq!(
            audio_choice(&wasapi, &bigger),
            audio(Host::Wasapi, None, Some(512), None)
        );
        let odd = AudioChange {
            host: Some("kernel".to_owned()),
            buffer_frames: Some(Some(0.0)),
            ..AudioChange::default()
        };
        assert_eq!(
            audio_choice(&default_audio(), &odd),
            audio(Host::Auto, None, None, None)
        );
        let asio = audio(Host::Asio, None, Some(256), Some("Volt ASIO"));
        assert_eq!(
            audio_choice(&asio, &bigger).asio_driver.as_deref(),
            Some("Volt ASIO")
        );
    }

    #[test]
    fn an_input_belongs_to_its_driver_a_new_driver_starts_on_its_default_unless_one_is_named() {
        let volt = audio(Host::Wasapi, Some(VOLT), Some(256), None);
        let device = |name: &str| AudioChange {
            device: Some(Some(name.to_owned())),
            ..AudioChange::default()
        };
        let bigger = AudioChange {
            buffer_frames: Some(Some(512.0)),
            ..AudioChange::default()
        };
        assert_eq!(audio_choice(&volt, &bigger).device.as_deref(), Some(VOLT));
        assert_eq!(
            audio_choice(&volt, &device("Mic")).device.as_deref(),
            Some("Mic")
        );
        assert_eq!(audio_choice(&volt, &change(Some("auto"))).device, None);
        let auto_mic = AudioChange {
            device: Some(Some("Mic".to_owned())),
            ..change(Some("auto"))
        };
        assert_eq!(
            audio_choice(&volt, &auto_mic).device.as_deref(),
            Some("Mic")
        );
        assert_eq!(
            audio_choice(&volt, &change(Some("WASAPI")))
                .device
                .as_deref(),
            Some(VOLT),
            "the same driver"
        );
        assert_eq!(
            audio_choice(&volt, &device("  ")).device,
            None,
            "an empty name is none"
        );
    }

    #[test]
    fn a_kept_choice_is_restored_as_the_next_start_takes_it_and_asio_without_a_driver_is_no_asio() {
        assert_eq!(
            restored_audio(None),
            default_audio(),
            "nothing kept: the system's own driver"
        );
        assert_eq!(
            restored_audio(Some(&saved("wasapi", Some(VOLT), Some(512), None))),
            audio(Host::Wasapi, Some(VOLT), Some(512), None)
        );
        assert_eq!(
            restored_audio(Some(&saved("asio", None, Some(128), Some("Volt ASIO")))),
            audio(Host::Asio, None, Some(128), Some("Volt ASIO")),
            "an ASIO driver the engineer picked"
        );
        for odd in [
            saved("asio", None, Some(256), None),
            saved("asio", Some(VOLT), Some(256), None),
            saved("ASIO", None, Some(256), Some("")),
        ] {
            let restored = restored_audio(Some(&odd));
            assert_eq!(restored.host, Host::Auto, "{odd:?}");
            assert_eq!(
                start_request(&restored, None)
                    .and_then(|request| request.host)
                    .as_deref(),
                Some("auto"),
                "no ASIO driver is ever loaded"
            );
        }
        assert_eq!(
            restored_audio(Some(&saved("kernel", None, None, None))).host,
            Host::Auto
        );
    }

    #[test]
    fn a_missing_input_gives_way_to_the_systems_default_on_its_driver_never_on_asio() {
        let volt = audio(Host::Wasapi, Some(VOLT), Some(256), None);
        let fallback = start_request(&fallback_audio(&volt), None).unwrap();
        let mut expected = summary(&start_request(&volt, None).unwrap());
        expected.0 = None;
        assert_eq!(summary(&fallback), expected);
        let auto = Audio {
            host: Host::Auto,
            ..volt
        };
        assert_eq!(
            start_request(&fallback_audio(&auto), None)
                .unwrap()
                .host
                .as_deref(),
            Some("auto")
        );

        let asio = audio(Host::Asio, None, Some(128), Some("Volt ASIO"));
        let fallback = start_request(&fallback_audio(&asio), None).unwrap();
        assert_eq!(
            fallback.host.as_deref(),
            Some("auto"),
            "the system's own driver"
        );
        assert_eq!(fallback.device_filter, None);
        assert_eq!(fallback.buffer_frames, Some(128));
    }

    #[test]
    fn the_fallback_is_said_with_the_input_that_was_missing() {
        assert_eq!(
            fallback_notice(&audio(Host::Wasapi, Some(VOLT), Some(256), None)),
            format!(
                "The saved input {VOLT} was not found, so the engine started on the system's default input. \
                 The saved input is kept and tried again at the next start."
            )
        );
        assert!(
            fallback_notice(&audio(Host::Asio, None, Some(256), Some("Volt ASIO"))).starts_with(
                "The saved ASIO driver Volt ASIO was not found, so the engine started on the system's own driver"
            )
        );
    }

    #[test]
    fn the_kept_choice_is_sent_as_the_session_keeps_it() {
        let request = settings_request(&audio(Host::Asio, None, Some(128), Some("Volt ASIO")));
        assert_eq!(
            request,
            AudioSettingsRequest {
                host: "asio".to_owned(),
                device: None,
                buffer_frames: Some(128),
                asio_driver: Some("Volt ASIO".to_owned()),
            }
        );
    }

    #[test]
    fn the_demo_is_a_driver_of_its_own_and_a_kept_demo_is_restored_as_chosen() {
        assert!(HOSTS.contains(&Host::Demo));
        assert_eq!(normalize_host(Some(" Demo ")), Host::Demo);
        assert_eq!(
            default_audio().host,
            Host::Auto,
            "the window starts on the system's own driver"
        );
        let demo = audio(Host::Demo, Some("Demo (no interface)"), Some(256), None);
        let request = start_request(&demo, None).unwrap();
        assert_eq!(request.host.as_deref(), Some("demo"));
        assert_eq!(
            request.device_filter.as_deref(),
            Some("Demo (no interface)")
        );
        assert_eq!(
            restored_audio(Some(&saved(
                "demo",
                Some("Demo (no interface)"),
                Some(256),
                None
            ))),
            demo
        );
        assert_eq!(
            restored_audio(None).host,
            Host::Auto,
            "nothing kept is never the demo"
        );
    }
}
