//! The Processing screen's commands to the desk (the processing part of `console-app.js`): the
//! strip edits and the effect sends, each on lanes of their own, the resets, copies and presets on
//! the console's serial desk lane, the strip presets and the equaliser's curve.
//!
//! - A strip edit is shown at once (the console's pending strip) and sent through its channel's
//!   lane (`set_channel_processing`): one in flight, the latest waiting, so a dragged knob sends
//!   only its latest value after the one in flight. It answers the last answer in words once the
//!   channel has nothing left to send. A session load holds it back: it is dropped once the load
//!   lands, and said so, or sent once the load was refused (C31).
//! - A send (`set_channel_send`) has a lane per channel and effect, its level clamped to a whole
//!   0-100, shown at once until it is answered.
//! - A reset, a copy or a preset runs on the serial desk lane once the strip lanes of the channels
//!   it touches and of their pair partners are quiet, so it lands after them (C29).
//! - An answer that comes while the strips were never read (or not read after a load) reads them
//!   all: the answer names only the strips it changed.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::future::Future;
use std::rc::Rc;

use bandear_desktop_console::coalesce::Lanes;
use bandear_desktop_console::command::Command;
use bandear_desktop_console::console::Console;
use bandear_desktop_console::exec::Slot;
use bandear_desktop_console::format::sentence_with_reason;
use bandear_desktop_console::state::{Action, ConsoleState};
use bandear_desktop_console::types::{
    Effect, EffectLevels, EqPointDto, EqSettings, ProcessingSetDto, StripPresetsSetDto,
    StripSettings,
};
use bandear_desktop_console::values::clamp_level;
use bandear_desktop_console::wire::effect_name;

use crate::strip::{Said, format_processing_set, format_strip_presets_set, strip_unit_channels};

/// The lane of a send: its channel (1-based) and its effect (0 the reverb, 1 the delay).
type SendKey = (usize, u8);

fn send_key(channel: usize, effect: Effect) -> SendKey {
    (
        channel,
        match effect {
            Effect::Reverb => 0,
            Effect::Delay => 1,
        },
    )
}

fn key_effect(key: SendKey) -> Effect {
    if key.1 == 0 {
        Effect::Reverb
    } else {
        Effect::Delay
    }
}

/// Why a strip edit that waited for a session load was never sent.
fn loaded_meanwhile(channel: usize) -> Said {
    Said::refused(sentence_with_reason(
        &format!("CH {channel}'s processing was not changed"),
        Some("a session was loaded meanwhile"),
    ))
}

#[derive(Default)]
struct Shared {
    /// The last answer to a strip edit of each channel, in words.
    said: RefCell<BTreeMap<usize, Said>>,
    /// The last answer to a send of each channel and effect, in words.
    send_said: RefCell<BTreeMap<SendKey, Said>>,
    /// The channels with a strip edit waiting, as of the last change of the state: what a loaded
    /// session drops.
    waiting: RefCell<Vec<usize>>,
    /// The edits waiting for a session load to end.
    released: RefCell<Vec<Slot<()>>>,
}

/// The processing commands, over the console; clones share the lanes.
#[derive(Clone)]
pub struct Strips {
    console: Console,
    shared: Rc<Shared>,
    strip_lanes: Lanes<usize, ()>,
    send_lanes: Lanes<SendKey, u8>,
}

/// Reads every strip once a change was answered while they were unknown.
async fn read_strips_if_unknown(console: &Console) {
    let unknown = console.state().processing.is_none();
    if unknown {
        console.load_processing().await;
    }
}

impl Strips {
    /// The commands over `console` (a handle that sends them for the Processing screen).
    pub fn new(console: &Console) -> Self {
        let shared = Rc::new(Shared::default());
        let strip_lanes = Lanes::new(console, strip_sender(console, &shared));
        let send_lanes = Lanes::new(console, send_sender(console, &shared));
        let strips = Self {
            console: console.clone(),
            shared,
            strip_lanes,
            send_lanes,
        };
        strips.follow_session_loads();
        strips
    }

    /// The console the commands go through.
    pub fn console(&self) -> &Console {
        &self.console
    }

    /// While a session loads every strip edit is held back. Once it loaded, the edits waiting were
    /// meant for the session before: the state drops them and they are said to be dropped. When
    /// the hold ends the edits still waiting (all of them after a refused load) go out.
    fn follow_session_loads(&self) {
        let shared = Rc::clone(&self.shared);
        self.console.subscribe(move |console, action| {
            if let Action::SessionLoaded(_) = action {
                let waiting = shared.waiting.borrow().clone();
                let mut said = shared.said.borrow_mut();
                for channel in waiting {
                    said.insert(channel, loaded_meanwhile(channel));
                }
            }
            *shared.waiting.borrow_mut() =
                console.state().processing_pending.keys().copied().collect();
        });
        let shared = Rc::clone(&self.shared);
        let lanes = self.strip_lanes.clone();
        self.console.on_sends_released(move |console| {
            let waiting: Vec<usize> = console.state().processing_pending.keys().copied().collect();
            for channel in waiting {
                lanes.push(channel, ());
            }
            let released: Vec<Slot<()>> = shared.released.borrow_mut().drain(..).collect();
            for slot in released {
                slot.fill(());
            }
        });
    }

    /// Resolves once the session load holding the sends back has ended.
    fn released(&self) -> impl Future<Output = ()> + 'static {
        let slot = Slot::new();
        self.shared.released.borrow_mut().push(slot.clone());
        slot.wait()
    }

    /// Edits `channel`'s strip to `settings`, the whole strip: shown at once and sent through the
    /// channel's lane. Resolves to the last answer in words once the channel has nothing left to
    /// send, and only then, so a held edit never answers a success it did not have.
    pub fn edit_processing(
        &self,
        channel: usize,
        settings: StripSettings,
    ) -> impl Future<Output = Said> + 'static {
        self.shared.said.borrow_mut().remove(&channel);
        self.console
            .dispatch(Action::ProcessingEdit { channel, settings });
        self.strip_lanes.push(channel, ());
        let strips = self.clone();
        async move {
            strips.strip_lanes.idle(&channel).await;
            while strips.console.holding() {
                strips.released().await;
                strips.strip_lanes.idle(&channel).await;
            }
            let said = strips.shared.said.borrow().get(&channel).cloned();
            said.unwrap_or_else(|| {
                Said::refused(sentence_with_reason(
                    &format!("CH {channel}'s processing was not changed"),
                    Some("it was not sent"),
                ))
            })
        }
    }

    /// Changes the send of `channel` (1-based) to `effect` to `level` percent: shown at once, sent
    /// through the lane of that channel and effect (the latest level wins), answered once settled.
    pub fn edit_send(
        &self,
        channel: usize,
        effect: Effect,
        level: f64,
    ) -> impl Future<Output = Said> + 'static {
        let key = send_key(channel, effect);
        let held = clamp_level(level);
        self.console.dispatch(Action::SendEdit {
            channel,
            effect,
            level: held,
        });
        self.send_lanes.push(key, held);
        let strips = self.clone();
        async move {
            strips.send_lanes.idle(&key).await;
            let said = strips.shared.send_said.borrow().get(&key).cloned();
            said.unwrap_or_else(|| Said::refused("The send was not changed."))
        }
    }

    /// Runs a processing change that is not an edit (a reset, a copy, a preset) on the serial desk
    /// lane, once the strip lanes of `channels` and of their pair partners are quiet.
    fn processing_command(
        &self,
        channels: Vec<usize>,
        command: Command,
        refused: String,
    ) -> impl Future<Output = Said> + 'static {
        let strips = self.clone();
        self.console.in_turn(false, async move {
            let touched = {
                let state = strips.console.state();
                let table = state
                    .engine
                    .status
                    .as_ref()
                    .map(|status| status.channels.clone())
                    .unwrap_or_default();
                strip_unit_channels(&table, &channels)
            };
            for channel in touched {
                strips.strip_lanes.idle(&channel).await;
            }
            match strips.console.call::<ProcessingSetDto>(command).await {
                Ok(answer) => {
                    let said = format_processing_set(&answer);
                    if said.ok {
                        strips.console.dispatch(Action::ProcessingApplied {
                            channel: None,
                            sent: None,
                            channels: answer.channels,
                        });
                        read_strips_if_unknown(&strips.console).await;
                    }
                    said
                }
                Err(error) => Said::refused(sentence_with_reason(&refused, Some(&error))),
            }
        })
    }

    /// Takes every part of `channel`'s strip off (its sends stay).
    pub fn reset(&self, channel: usize) -> impl Future<Output = Said> + 'static {
        self.processing_command(
            vec![channel],
            Command::ResetChannelProcessing { channel },
            format!("CH {channel}'s processing was not reset"),
        )
    }

    /// Copies the strip of `from` onto every channel in `to` (1-based).
    pub fn copy(&self, from: usize, to: Vec<usize>) -> impl Future<Output = Said> + 'static {
        let mut channels = vec![from];
        channels.extend(&to);
        self.processing_command(
            channels,
            Command::CopyChannelProcessing { from, to },
            format!("CH {from}'s processing was not copied"),
        )
    }

    /// Puts the strip preset `name` on `channel`.
    pub fn apply_preset(
        &self,
        name: String,
        channel: usize,
    ) -> impl Future<Output = Said> + 'static {
        self.processing_command(
            vec![channel],
            Command::ApplyStripPreset {
                name: name.clone(),
                channel,
            },
            format!("\"{name}\" was not applied to CH {channel}"),
        )
    }

    /// Runs a strip preset change; the presets of its answer replace the ones known.
    fn preset_command(
        &self,
        command: Command,
        refused: &'static str,
    ) -> impl Future<Output = Said> + 'static {
        let console = self.console.clone();
        async move {
            match console.call::<StripPresetsSetDto>(command).await {
                Ok(answer) => {
                    let said = format_strip_presets_set(&answer);
                    console.dispatch(Action::StripPresets(Some(answer.presets)));
                    said
                }
                Err(error) => Said::refused(sentence_with_reason(refused, Some(&error))),
            }
        }
    }

    /// Saves `settings` as the strip preset `name`.
    pub fn save_preset(
        &self,
        name: String,
        settings: StripSettings,
    ) -> impl Future<Output = Said> + 'static {
        self.preset_command(
            Command::SaveStripPreset { name, settings },
            "The strip preset was not saved",
        )
    }

    /// Deletes the strip preset `name`.
    pub fn delete_preset(&self, name: String) -> impl Future<Output = Said> + 'static {
        self.preset_command(
            Command::DeleteStripPreset { name },
            "The strip preset was not deleted",
        )
    }

    /// Asks the desk for the response of the equaliser `eq` (128 points) and keeps it as
    /// `channel`'s curve; the points, or why they could not be worked out.
    pub fn load_eq_response(
        &self,
        channel: usize,
        eq: EqSettings,
    ) -> impl Future<Output = Result<Vec<EqPointDto>, String>> + 'static {
        let console = self.console.clone();
        async move {
            match console
                .call::<Vec<EqPointDto>>(Command::EqResponse {
                    settings: eq,
                    points: None,
                })
                .await
            {
                Ok(points) => {
                    console.dispatch(Action::EqCurve {
                        channel,
                        points: points.clone(),
                    });
                    Ok(points)
                }
                Err(error) => Err(sentence_with_reason(
                    "The equaliser's curve could not be drawn",
                    Some(&error),
                )),
            }
        }
    }
}

/// The strip lanes' send: the strip waiting for `channel`, unless a session load holds it back.
fn strip_sender(
    console: &Console,
    shared: &Rc<Shared>,
) -> bandear_desktop_console::coalesce::Send<usize, ()> {
    let console = console.clone();
    let shared = Rc::clone(shared);
    Rc::new(move |channel, ()| {
        let console = console.clone();
        let shared = Rc::clone(&shared);
        Box::pin(async move {
            let sent = console.state().processing_pending.get(&channel).cloned();
            let Some(sent) = sent else {
                return Ok(());
            };
            if console.holding() {
                return Ok(());
            }
            let answer = console
                .call::<ProcessingSetDto>(Command::SetChannelProcessing {
                    channel,
                    settings: sent.value,
                })
                .await;
            let said = match &answer {
                Ok(answer) => format_processing_set(answer),
                Err(error) => Said::refused(sentence_with_reason(
                    &format!("CH {channel}'s processing was not changed"),
                    Some(error),
                )),
            };
            shared.said.borrow_mut().insert(channel, said.clone());
            match answer {
                Ok(answer) if said.ok => {
                    console.dispatch(Action::ProcessingApplied {
                        channel: Some(channel),
                        sent: Some(sent.stamp),
                        channels: answer.channels,
                    });
                    read_strips_if_unknown(&console).await;
                }
                _ => {
                    console.dispatch(Action::ProcessingFailed {
                        channel,
                        sent: Some(sent.stamp),
                        message: said.message,
                    });
                }
            }
            Ok(())
        })
    })
}

/// The send lanes' send: `level` for the channel and effect of `key`; the send shown meanwhile
/// is settled either way.
fn send_sender(
    console: &Console,
    shared: &Rc<Shared>,
) -> bandear_desktop_console::coalesce::Send<SendKey, u8> {
    let console = console.clone();
    let shared = Rc::clone(shared);
    Rc::new(move |key, level| {
        let console = console.clone();
        let shared = Rc::clone(&shared);
        Box::pin(async move {
            let (channel, effect) = (key.0, key_effect(key));
            let answer = console
                .call::<ProcessingSetDto>(Command::SetChannelSend {
                    channel,
                    effect,
                    level,
                })
                .await;
            match answer {
                Ok(answer) => {
                    let said = format_processing_set(&answer);
                    shared.send_said.borrow_mut().insert(key, said.clone());
                    if said.ok {
                        console.dispatch(Action::ProcessingApplied {
                            channel: Some(channel),
                            sent: None,
                            channels: answer.channels,
                        });
                        read_strips_if_unknown(&console).await;
                    }
                }
                Err(error) => {
                    shared.send_said.borrow_mut().insert(
                        key,
                        Said::refused(sentence_with_reason(
                            &format!(
                                "CH {channel}'s {} send was not changed",
                                effect_name(effect)
                            ),
                            Some(&error),
                        )),
                    );
                }
            }
            console.dispatch(Action::SendSettled {
                channel,
                effect,
                level,
            });
            Ok(())
        })
    })
}

/// The sends of `channel`'s strip (a pair shares its first side's): the change on its way first,
/// else the desk's.
pub fn channel_sends(state: &ConsoleState, channel: usize) -> EffectLevels {
    let kept = state
        .processing
        .as_ref()
        .and_then(|strips| strips.iter().find(|strip| strip.channel == channel))
        .map(|strip| strip.settings.sends.clamped())
        .unwrap_or_default();
    let waiting = state.sends_pending.get(&channel);
    EffectLevels {
        reverb: waiting
            .and_then(|levels| levels.reverb)
            .unwrap_or(kept.reverb),
        delay: waiting
            .and_then(|levels| levels.delay)
            .unwrap_or(kept.delay),
    }
}

#[cfg(test)]
pub(crate) mod tests;
