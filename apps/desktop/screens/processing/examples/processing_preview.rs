//! Opens the screen alone over the simulated desk, with its engine running on the sample
//! interface and musicians: no engine starts, no input opens and nothing goes on the network. The
//! desk answers the strips (CH 1 with its equaliser and compressor working), the built-in strip
//! presets and the equaliser's curve, so the editor shows something to edit.

use bandear_desktop_console::command::{Answer, Command};
use bandear_desktop_console::testing::FakeDesk;
use bandear_desktop_console::types::{ChannelProcessingDto, StripPresetDto, StripSettings};
use bandear_desktop_processing::strip::eq_response_of;

fn main() {
    let mut desk = FakeDesk::running();
    desk.script("processing", |_, _| {
        let mut working = StripSettings::default();
        working.eq.high_pass.enabled = true;
        working.eq.bands[1].gain_db = 4.5;
        working.compressor.enabled = true;
        working.sends.reverb = 20;
        Ok(Answer::Processing(
            (1..=32)
                .map(|channel| {
                    let settings = if channel == 1 {
                        working
                    } else {
                        StripSettings::default()
                    };
                    ChannelProcessingDto {
                        channel,
                        settings,
                        active: !settings.is_neutral(),
                    }
                })
                .collect(),
        ))
    });
    desk.script("strip_presets", |_, _| {
        Ok(Answer::StripPresets(
            [
                "Vocal",
                "Kick",
                "Bass DI",
                "Acoustic guitar",
                "Gentle compression",
            ]
            .iter()
            .map(|name| StripPresetDto {
                name: (*name).to_owned(),
                settings: StripSettings::default(),
                factory: true,
            })
            .collect(),
        ))
    });
    desk.script("eq_response", |_, command| match command {
        Command::EqResponse { settings, points } => Ok(Answer::EqResponse(eq_response_of(
            settings,
            points.unwrap_or(128),
        ))),
        _ => Err("not eq_response".to_owned()),
    });
    bandear_desktop_preview::run::<bandear_desktop_processing::view::ProcessingScreen>(desk);
}
