//! The demo source: a generated band the engine plays in place of an audio interface, so the app
//! runs and streams to phones on a machine with no interface (`demo-source.js`).
//!
//! It is a driver of its own ([DEMO_HOST]) with one device, chosen only by the engineer: the
//! window never picks it on its own. While it plays, the chip strip says so ([demo_chip]) and the
//! window's title says "Demo source" ([crate::console::window_title]).

use bandear_desktop_core::dto::InputDeviceDto;
pub use personal_monitoring::{DEMO_CHANNELS, DEMO_DEVICE_NAME as DEMO_DEVICE, DEMO_VOICES};

use crate::chips::{Chip, ChipKind};
use crate::engine_config::Host;
use crate::router::Section;
use crate::wire::Status;

/// The demo source's driver.
pub const DEMO_HOST: Host = Host::Demo;

/// The devices `list_input_devices` offers for the demo driver: its one device.
pub fn demo_devices() -> Vec<InputDeviceDto> {
    vec![InputDeviceDto {
        name: DEMO_DEVICE.to_owned(),
        channels: DEMO_CHANNELS,
    }]
}

/// True while the running engine (its `status`) plays the demo source rather than an interface.
pub fn is_demo_running(status: Option<&Status>) -> bool {
    status.is_some_and(|status| !status.stopped && status.demo)
}

/// The chip strip's badge while the demo source plays, linking to the audio setup where an
/// interface is chosen; `None` while an interface (or nothing) runs.
pub fn demo_chip(status: Option<&Status>) -> Option<Chip> {
    if !is_demo_running(status) {
        return None;
    }
    Some(Chip {
        kind: ChipKind::Demo,
        word: "Demo source".to_owned(),
        time: String::new(),
        detail: "\u{2014} no interface in use".to_owned(),
        label: "Demo source \u{2014} no interface in use: the engine plays a generated band. Open \
                Network and system to choose an interface"
            .to_owned(),
        target: Section::Network,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(demo: bool, stopped: bool) -> Status {
        Status {
            run: Some(1),
            device_name: DEMO_DEVICE.to_owned(),
            demo,
            stopped,
            ..Status::default()
        }
    }

    #[test]
    fn the_demo_has_one_device_with_a_voice_per_input() {
        let devices = demo_devices();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].name, "Demo (no interface)");
        assert_eq!(devices[0].channels, 8);
        assert_eq!(DEMO_VOICES.len(), usize::from(DEMO_CHANNELS));
        assert_eq!(DEMO_HOST.name(), "demo");
    }

    #[test]
    fn the_badge_says_demo_source_only_while_the_demo_plays() {
        assert_eq!(demo_chip(None), None, "no engine");
        assert!(!is_demo_running(Some(&status(false, false))));
        assert_eq!(demo_chip(Some(&status(false, false))), None, "an interface");
        assert!(is_demo_running(Some(&status(true, false))));
        assert_eq!(
            demo_chip(Some(&status(true, false))),
            Some(Chip {
                kind: ChipKind::Demo,
                word: "Demo source".to_owned(),
                time: String::new(),
                detail: "\u{2014} no interface in use".to_owned(),
                label:
                    "Demo source \u{2014} no interface in use: the engine plays a generated band. \
                        Open Network and system to choose an interface"
                        .to_owned(),
                target: Section::Network,
            })
        );
        assert_eq!(
            demo_chip(Some(&status(true, true))),
            None,
            "a stopped engine plays nothing"
        );
    }
}
