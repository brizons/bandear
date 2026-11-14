//! The desk's processing commands for the screen's tests, on the console's simulated desk, as
//! `commands/processing.rs` answers them: 32 strips, a linked pair as one strip, the built-in
//! presets first and never touched, a channel past CH 32 `invalid`, and every change refused while
//! no engine runs.

use std::cell::RefCell;
use std::rc::Rc;

use bandear_desktop_console::command::{Answer, Command};
use bandear_desktop_console::state::pair_units;
use bandear_desktop_console::testing::FakeDesk;
use bandear_desktop_console::types::{
    ChannelProcessingDto, EffectLevels, ProcessingSetDto, StripPresetDto, StripPresetsSetDto,
    StripSettings,
};
use bandear_desktop_console::wire::effect_name;

use crate::strip::{STRIP_CHANNELS, channel_list, default_strip, eq_response_of};

/// What the desk keeps: every channel's strip (CH 1 first) and the strip presets.
#[derive(Clone, Debug)]
pub struct Kept {
    pub strips: Vec<StripSettings>,
    /// `(name, settings, built in)`, the built-in ones first.
    pub presets: Vec<(String, StripSettings, bool)>,
}

pub type Shared = Rc<RefCell<Kept>>;

/// The built-in presets, each doing something.
pub fn factory_presets() -> Vec<(String, StripSettings, bool)> {
    [
        "Vocal",
        "Kick",
        "Bass DI",
        "Acoustic guitar",
        "Gentle compression",
    ]
    .iter()
    .enumerate()
    .map(|(index, name)| {
        let mut settings = default_strip();
        settings.eq.high_pass.enabled = true;
        settings.eq.high_pass.frequency_hz = 60.0 + 20.0 * index as f32;
        settings.compressor.enabled = index % 2 == 0;
        ((*name).to_owned(), settings, true)
    })
    .collect()
}

fn dto(kept: &Kept, channel: usize) -> ChannelProcessingDto {
    let settings = kept.strips[channel - 1];
    ChannelProcessingDto {
        channel,
        settings,
        active: !settings.is_neutral(),
    }
}

fn verb(channels: &[usize]) -> &'static str {
    if channels.len() == 1 { "is" } else { "are" }
}

/// `channels` with each one's pair partner on the running device, in channel order.
fn with_partners(desk: &FakeDesk, channels: &[usize]) -> Vec<usize> {
    let status = &desk.report.status;
    let units = pair_units(&status.channels[..status.channels.len().min(status.source_channels)]);
    let mut all: Vec<usize> = Vec::new();
    for channel in channels {
        let unit = units
            .iter()
            .find(|unit| unit.contains(&(channel - 1)))
            .cloned()
            .unwrap_or_else(|| vec![channel - 1]);
        for index in unit {
            if !all.contains(&(index + 1)) {
                all.push(index + 1);
            }
        }
    }
    all.sort_unstable();
    all
}

fn invalid(message: String) -> ProcessingSetDto {
    ProcessingSetDto {
        outcome: "invalid",
        channels: Vec::new(),
        message,
    }
}

fn outside(channel: usize) -> Option<ProcessingSetDto> {
    (!(1..=STRIP_CHANNELS).contains(&channel)).then(|| {
        invalid(format!(
            "CH {channel} is not one of the mixer's channels (CH 1-{STRIP_CHANNELS})"
        ))
    })
}

fn not_running(desk: &FakeDesk) -> Result<(), String> {
    if desk.running {
        Ok(())
    } else {
        Err("the engine is not running".to_owned())
    }
}

/// Puts `settings` on `channels`, each keeping its own sends unless `with_sends`.
fn put(
    kept: &mut Kept,
    channels: &[usize],
    settings: StripSettings,
    with_sends: bool,
) -> Vec<ChannelProcessingDto> {
    for channel in channels {
        let strip = &mut kept.strips[channel - 1];
        let sends = strip.sends;
        *strip = settings.clamped();
        if !with_sends {
            strip.sends = sends;
        }
    }
    channels.iter().map(|channel| dto(kept, *channel)).collect()
}

fn presets_of(kept: &Kept) -> Vec<StripPresetDto> {
    kept.presets
        .iter()
        .map(|(name, settings, factory)| StripPresetDto {
            name: name.clone(),
            settings: *settings,
            factory: *factory,
        })
        .collect()
}

fn presets_answer(kept: &Kept, outcome: &'static str, message: String) -> Answer {
    Answer::StripPresetsSet(StripPresetsSetDto {
        outcome,
        preset: None,
        presets: presets_of(kept),
        message,
    })
}

/// Scripts `desk` with the processing commands; every strip starts neutral.
pub fn processing_desk(desk: &mut FakeDesk) -> Shared {
    let kept = Rc::new(RefCell::new(Kept {
        strips: vec![default_strip(); STRIP_CHANNELS],
        presets: factory_presets(),
    }));

    let shared = Rc::clone(&kept);
    desk.script("processing", move |_, _| {
        let kept = shared.borrow();
        Ok(Answer::Processing(
            (1..=STRIP_CHANNELS)
                .map(|channel| dto(&kept, channel))
                .collect(),
        ))
    });
    let shared = Rc::clone(&kept);
    desk.script("strip_presets", move |_, _| {
        Ok(Answer::StripPresets(presets_of(&shared.borrow())))
    });
    desk.script("eq_response", |_, command| match command {
        Command::EqResponse { settings, points } => Ok(Answer::EqResponse(eq_response_of(
            settings,
            points.unwrap_or(128),
        ))),
        _ => Err("not eq_response".to_owned()),
    });

    let shared = Rc::clone(&kept);
    desk.script("set_channel_processing", move |desk, command| {
        let Command::SetChannelProcessing { channel, settings } = command else {
            return Err("not set_channel_processing".to_owned());
        };
        if let Some(refused) = outside(*channel) {
            return Ok(Answer::ProcessingSet(refused));
        }
        not_running(desk)?;
        let channels = with_partners(desk, &[*channel]);
        let changed = put(&mut shared.borrow_mut(), &channels, *settings, true);
        Ok(Answer::ProcessingSet(ProcessingSetDto {
            outcome: "set",
            channels: changed,
            message: format!(
                "{} {} set to the new processing.",
                channel_list(&channels),
                verb(&channels)
            ),
        }))
    });

    let shared = Rc::clone(&kept);
    desk.script("reset_channel_processing", move |desk, command| {
        let Command::ResetChannelProcessing { channel } = command else {
            return Err("not reset_channel_processing".to_owned());
        };
        if let Some(refused) = outside(*channel) {
            return Ok(Answer::ProcessingSet(refused));
        }
        not_running(desk)?;
        let channels = with_partners(desk, &[*channel]);
        let changed = put(&mut shared.borrow_mut(), &channels, default_strip(), false);
        Ok(Answer::ProcessingSet(ProcessingSetDto {
            outcome: "set",
            channels: changed,
            message: format!(
                "{} {} back to no processing.",
                channel_list(&channels),
                verb(&channels)
            ),
        }))
    });

    let shared = Rc::clone(&kept);
    desk.script("copy_channel_processing", move |desk, command| {
        let Command::CopyChannelProcessing { from, to } = command else {
            return Err("not copy_channel_processing".to_owned());
        };
        not_running(desk)?;
        let mut targets: Vec<usize> = Vec::new();
        for channel in to {
            if *channel != *from && !targets.contains(channel) {
                targets.push(*channel);
            }
        }
        if targets.is_empty() {
            return Ok(Answer::ProcessingSet(invalid(format!(
                "There is no channel to copy CH {from}'s processing to; nothing changed."
            ))));
        }
        let channels = with_partners(desk, &targets);
        let mut kept = shared.borrow_mut();
        let source = kept.strips[from - 1];
        let changed = put(&mut kept, &channels, source, false);
        Ok(Answer::ProcessingSet(ProcessingSetDto {
            outcome: "set",
            channels: changed,
            message: format!(
                "CH {from}'s processing is on {} now.",
                channel_list(&channels)
            ),
        }))
    });

    let shared = Rc::clone(&kept);
    desk.script("apply_strip_preset", move |desk, command| {
        let Command::ApplyStripPreset { name, channel } = command else {
            return Err("not apply_strip_preset".to_owned());
        };
        not_running(desk)?;
        let mut kept = shared.borrow_mut();
        let found = kept
            .presets
            .iter()
            .find(|(kept, _, _)| kept.to_lowercase() == name.trim().to_lowercase())
            .map(|(name, settings, _)| (name.clone(), *settings));
        let Some((name, settings)) = found else {
            return Ok(Answer::ProcessingSet(ProcessingSetDto {
                outcome: "notFound",
                channels: Vec::new(),
                message: format!("There is no strip preset called \"{name}\"; nothing changed."),
            }));
        };
        let channels = with_partners(desk, &[*channel]);
        let changed = put(&mut kept, &channels, settings, false);
        Ok(Answer::ProcessingSet(ProcessingSetDto {
            outcome: "set",
            channels: changed,
            message: format!("\"{name}\" is on {} now.", channel_list(&channels)),
        }))
    });

    let shared = Rc::clone(&kept);
    desk.script("save_strip_preset", move |_, command| {
        let Command::SaveStripPreset { name, settings } = command else {
            return Err("not save_strip_preset".to_owned());
        };
        let name = name.trim().to_owned();
        let mut kept = shared.borrow_mut();
        if name.is_empty() {
            let message = "A strip preset needs a name; nothing was saved.".to_owned();
            return Ok(presets_answer(&kept, "invalid", message));
        }
        let mut settings = settings.clamped();
        settings.sends = EffectLevels::default();
        let same =
            |kept: &(String, StripSettings, bool)| kept.0.to_lowercase() == name.to_lowercase();
        match kept.presets.iter().position(same) {
            Some(at) if kept.presets[at].2 => {
                let message = format!(
                    "\"{name}\" is a built-in preset; save under another name. Nothing was saved."
                );
                Ok(presets_answer(&kept, "invalid", message))
            }
            Some(at) => {
                kept.presets[at] = (name.clone(), settings, false);
                let message = format!("The strip preset \"{name}\" was replaced.");
                Ok(presets_answer(&kept, "replaced", message))
            }
            None => {
                kept.presets.push((name.clone(), settings, false));
                let message = format!("Saved the strip preset \"{name}\".");
                Ok(presets_answer(&kept, "saved", message))
            }
        }
    });

    let shared = Rc::clone(&kept);
    desk.script("delete_strip_preset", move |_, command| {
        let Command::DeleteStripPreset { name } = command else {
            return Err("not delete_strip_preset".to_owned());
        };
        let mut kept = shared.borrow_mut();
        let at = kept
            .presets
            .iter()
            .position(|kept| kept.0.to_lowercase() == name.trim().to_lowercase());
        match at {
            Some(at) if kept.presets[at].2 => {
                let message = format!("\"{name}\" is a built-in preset and cannot be deleted.");
                Ok(presets_answer(&kept, "factory", message))
            }
            Some(at) => {
                let (name, _, _) = kept.presets.remove(at);
                let message = format!("Deleted the strip preset \"{name}\".");
                Ok(presets_answer(&kept, "deleted", message))
            }
            None => {
                let message =
                    format!("There is no strip preset called \"{name}\"; nothing was deleted.");
                Ok(presets_answer(&kept, "notFound", message))
            }
        }
    });

    let shared = Rc::clone(&kept);
    desk.script("set_channel_send", move |desk, command| {
        let Command::SetChannelSend {
            channel,
            effect,
            level,
        } = command
        else {
            return Err("not set_channel_send".to_owned());
        };
        if let Some(refused) = outside(*channel) {
            return Ok(Answer::ProcessingSet(refused));
        }
        not_running(desk)?;
        let channels = with_partners(desk, &[*channel]);
        let mut kept = shared.borrow_mut();
        for side in &channels {
            let strip = &mut kept.strips[side - 1];
            strip.sends = strip.sends.with(*effect, *level);
        }
        let changed = channels.iter().map(|side| dto(&kept, *side)).collect();
        Ok(Answer::ProcessingSet(ProcessingSetDto {
            outcome: "set",
            channels: changed,
            message: format!(
                "CH {channel} is sending {level}% to the {}.",
                effect_name(*effect)
            ),
        }))
    });
    kept
}
