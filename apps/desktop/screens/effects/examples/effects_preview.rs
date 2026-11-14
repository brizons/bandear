//! Opens the screen alone over the simulated desk, with its engine running on the sample
//! interface and musicians: no engine starts, no input opens and nothing goes on the network. The
//! desk answers `set_effect` as the real desk does (the effect set, both effects, its words).

use bandear_desktop_console::command::{Answer, Command};
use bandear_desktop_console::testing::FakeDesk;
use bandear_desktop_console::types::dto::EffectSetDto;
use bandear_desktop_console::types::{DelaySettings, Effect, ReverbSettings};

fn main() {
    let mut desk = FakeDesk::running();
    desk.script("set_effect", |desk, command| {
        let Command::SetEffect { effect, settings } = command else {
            return Err("not an effect".to_owned());
        };
        let effects = &mut desk.report.status.effects;
        let message = match effect {
            Effect::Reverb => {
                effects.reverb = serde_json::from_value::<ReverbSettings>(settings.clone())
                    .map_err(|error| error.to_string())?
                    .clamped();
                let reverb = effects.reverb;
                format!(
                    "The reverb is set: size {}%, damping {}%, pre-delay {} ms, width {}%.",
                    reverb.size_percent.round(),
                    reverb.damping_percent.round(),
                    reverb.pre_delay_ms.round(),
                    reverb.width_percent.round()
                )
            }
            Effect::Delay => {
                effects.delay = serde_json::from_value::<DelaySettings>(settings.clone())
                    .map_err(|error| error.to_string())?
                    .clamped();
                let delay = effects.delay;
                format!(
                    "The delay is set: {} ms, feedback {}%{}.",
                    delay.time_ms_in_force().round(),
                    delay.feedback_percent.round(),
                    if delay.ping_pong { ", ping-pong" } else { "" }
                )
            }
        };
        let effects = *effects;
        desk.emit_status();
        Ok(Answer::EffectSet(EffectSetDto {
            outcome: "set",
            effect: *effect,
            effects,
            saved: true,
            save_error: None,
            message,
        }))
    });
    bandear_desktop_preview::run::<bandear_desktop_effects::view::EffectsScreen>(desk);
}
