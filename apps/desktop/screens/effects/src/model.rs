//! The Effects screen's model: the effect settings lane of `console-app.js` (`editEffect`) and
//! what the screen keeps between draws (`effects-screen.js`: the last outcome). Pure: no window,
//! tested with plain `#[test]`s (the console's `testing` feature has a simulated desk to drive it).
//!
//! Every change sends the whole effect, the current view's values with the change merged: the
//! console shows it at once (`effects_pending`) and sends it through a lane per effect, one in
//! flight and the latest waiting, so a slider dragged across its range sends the first value and
//! then only the latest. An answer settles exactly the change it carried, and only that effect's
//! settings are kept from it, so an older answer for one effect never undoes the other's newer
//! one. Every settled change is said, successes included.

use std::cell::RefCell;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use bandear_desktop_console::coalesce::Lanes;
use bandear_desktop_console::command::Command;
use bandear_desktop_console::console::Console;
use bandear_desktop_console::format::sentence_with_reason;
use bandear_desktop_console::state::{Action, EffectChange, Stamped};
use bandear_desktop_console::types::dto::EffectSetDto;
use bandear_desktop_console::types::{DelayNote, DelaySettings, Effect, ReverbSettings};

use crate::effects::{Said, effects_view, format_effect_set, read_tempo};

/// A flow of the screen, run on the console's executor.
pub type Flow<T> = Pin<Box<dyn Future<Output = T>>>;

/// What the lane sends for an effect: the whole effect, stamped as the console keeps it.
type Sent = Stamped<EffectChange>;

/// The screen's model, built once as the window opens and kept for its life, so its lane goes on
/// while another screen is shown. Clones share it.
#[derive(Clone)]
pub struct Model {
    console: Console,
    lanes: Lanes<&'static str, Sent>,
    /// The last answer for each effect, in words.
    said: Rc<RefCell<HashMap<&'static str, Said>>>,
    /// The outcome the screen shows: the last change settled, or a tempo it refused.
    outcome: Rc<RefCell<Option<Said>>>,
}

impl Model {
    /// The model over `console` (a handle that sends its commands for this screen).
    pub fn new(console: Console) -> Self {
        let said = Rc::new(RefCell::new(HashMap::new()));
        let sender = console.clone();
        let heard = Rc::clone(&said);
        let lanes = Lanes::new(
            &console,
            Rc::new(move |_key, sent: Sent| {
                let console = sender.clone();
                let said = Rc::clone(&heard);
                Box::pin(async move {
                    send_effect(&console, &said, sent).await;
                    // A refusal is said here and settles the change; the lane goes on either way.
                    Ok(())
                }) as Pin<Box<dyn Future<Output = Result<(), String>>>>
            }),
        );
        Self {
            console,
            lanes,
            said,
            outcome: Rc::new(RefCell::new(None)),
        }
    }

    /// The console this screen talks to the desk through.
    pub fn console(&self) -> &Console {
        &self.console
    }

    /// The outcome the screen shows, if any.
    pub fn outcome(&self) -> Option<Said> {
        self.outcome.borrow().clone()
    }

    /// Changes the shared effect to `change` (the whole effect, held in range): shown at once,
    /// sent through the effect's lane, answered once settled.
    pub fn edit_effect(&self, change: EffectChange) -> Flow<Said> {
        let held = match change {
            EffectChange::Reverb(reverb) => EffectChange::Reverb(reverb.clamped()),
            EffectChange::Delay(delay) => EffectChange::Delay(delay.clamped()),
        };
        let effect = held.effect();
        self.console.dispatch(Action::EffectEdit(held));
        let stamp = self
            .console
            .state()
            .effects_pending
            .stamp(effect)
            .unwrap_or(0);
        let key = key_of(effect);
        self.lanes.push(key, Stamped { stamp, value: held });
        let idle = self.lanes.idle(&key);
        let said = Rc::clone(&self.said);
        Box::pin(async move {
            idle.await;
            said.borrow().get(key).cloned().unwrap_or_else(|| Said {
                ok: false,
                message: "The effect was not set.".to_owned(),
            })
        })
    }

    /// Sends `change` and keeps its outcome to be shown once settled.
    fn edit_shown(&self, change: EffectChange) -> Flow<Said> {
        let flow = self.edit_effect(change);
        let outcome = Rc::clone(&self.outcome);
        Box::pin(async move {
            let said = flow.await;
            *outcome.borrow_mut() = Some(said.clone());
            said
        })
    }

    /// Sends the reverb with `change` made to what the panel shows now; the outcome is kept to be
    /// shown once settled.
    pub fn edit_reverb(&self, change: impl FnOnce(&mut ReverbSettings)) -> Flow<Said> {
        let mut reverb = effects_view(&self.console.state()).reverb.settings;
        change(&mut reverb);
        self.edit_shown(EffectChange::Reverb(reverb))
    }

    /// Sends the delay with `change` made to what the panel shows now; the outcome is kept to be
    /// shown once settled.
    pub fn edit_delay(&self, change: impl FnOnce(&mut DelaySettings)) -> Flow<Said> {
        let mut delay = effects_view(&self.console.state()).delay.settings;
        change(&mut delay);
        self.edit_shown(EffectChange::Delay(delay))
    }

    /// Set tempo: the tempo `text` (empty for none) on `note`; a text that is not a tempo is said
    /// and nothing is sent (`None`).
    pub fn set_tempo(&self, text: &str, note: DelayNote) -> Option<Flow<Said>> {
        match read_tempo(text) {
            Ok(tempo_bpm) => Some(self.edit_delay(move |delay| {
                delay.tempo_bpm = tempo_bpm;
                delay.note = note;
            })),
            Err(message) => {
                *self.outcome.borrow_mut() = Some(Said { ok: false, message });
                None
            }
        }
    }
}

/// The name the desk gives `effect`, the lane's key.
fn key_of(effect: Effect) -> &'static str {
    effect.name()
}

/// Sends the effect change `sent` and keeps that effect as the desk answered it; the change shown
/// meanwhile is settled either way, and the answer is kept in words.
async fn send_effect(
    console: &Console,
    said: &Rc<RefCell<HashMap<&'static str, Said>>>,
    sent: Sent,
) {
    let effect = sent.value.effect();
    let settings = match sent.value {
        EffectChange::Reverb(reverb) => serde_json::to_value(reverb),
        EffectChange::Delay(delay) => serde_json::to_value(delay),
    };
    let answer = match settings {
        Ok(settings) => {
            console
                .call::<EffectSetDto>(Command::SetEffect { effect, settings })
                .await
        }
        Err(error) => Err(error.to_string()),
    };
    let (words, kept) = match answer {
        Ok(answer) => {
            let words = format_effect_set(&answer);
            let kept = words.ok.then(|| answer.effects.clamped());
            (words, kept)
        }
        Err(error) => (
            Said {
                ok: false,
                message: sentence_with_reason(
                    &format!("The {} was not set", effect.name()),
                    Some(&error),
                ),
            },
            None,
        ),
    };
    said.borrow_mut().insert(key_of(effect), words);
    console.dispatch(Action::EffectSettled {
        effect,
        sent: Some(sent.stamp),
        effects: kept,
    });
}

#[cfg(test)]
mod tests;
