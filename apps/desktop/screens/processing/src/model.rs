//! The Processing screen's model (`processing-editor.js`): which strip is open, the changes it
//! sends through the processing commands ([Strips]), A/B, the equaliser curve asked of the desk
//! once the edits pause, presets, copies, the sends and the last outcome in words. Pure: no window,
//! tested with plain `#[test]`s over the console's simulated desk.
//!
//! A/B: "Bypass" keeps the open strip here and sends a neutral one, so the engineer hears the
//! channel untouched; turning it off sends the kept strip back. While bypassed the strip shown is
//! the kept one and nothing else is sent for it. A strip the desk shows working meanwhile, with no
//! edit of the editor's on its way, was changed elsewhere (a session load, a preset): its bypass
//! ends and the kept strip is dropped. Leaving the screen ends every bypass ([Model::leave]).
//!
//! The model lives as long as the window, so it comes back to the strip it had open last; each time
//! the screen is shown ([Model::shown]) it starts with no outcome, as a freshly opened editor did.

use std::cell::RefCell;
use std::future::Future;
use std::rc::Rc;
use std::time::Duration;

use bandear_desktop_console::console::Console;
use bandear_desktop_console::types::{Effect, EffectLevels, EqSettings, StripSettings};

use crate::choices::{
    Ab, FailureLine, PresetOption, StripChoice, ab_bypass, ab_restore, ab_settle, preset_options,
    step_choice, strip_choices,
};
use crate::desk::{Strips, channel_sends};
use crate::fields::{Key, Part, with_enabled, with_number};
use crate::graph::{Curve, curve_source};
use crate::strip::{EQ_RESPONSE_POINTS, Said, clamp_strip};

/// How long the edits must pause before the desk is asked for the equaliser's curve.
pub const CURVE_DEBOUNCE: Duration = Duration::from_millis(120);

/// The blocked screen's note while no engine runs.
pub const NOT_RUNNING: &str = "The engine is not running, so this desk does not know its inputs yet. Start it from the Mix screen.";
/// The blocked screen's note while the strips could not be read.
pub const UNREAD: &str = "The channel processing could not be read.";

/// What the screen draws now.
#[derive(Clone, Debug, PartialEq)]
pub struct EditorView {
    /// False while no engine runs (no inputs to open).
    pub known: bool,
    /// True while the strips were never read.
    pub unread: bool,
    pub choices: Vec<StripChoice>,
    pub choice: Option<StripChoice>,
    /// The strip open (its first side, 1-based).
    pub channel: usize,
    pub sides: Vec<usize>,
    /// The strip shown: the kept one while bypassed.
    pub settings: StripSettings,
    pub active: bool,
    pub pending: bool,
    pub bypassed: bool,
    pub can_bypass: bool,
    pub presets: Vec<PresetOption>,
    pub curve: Curve,
    /// The last outcome of the editor's own commands.
    pub outcome: Option<Said>,
    /// The strips' error, when it is about the strip open (or none).
    pub error: Option<String>,
    /// The failure line: the latest failure among a refused send of this strip, the editor's last
    /// refusal and the strips' error.
    pub failure: String,
    /// The open strip's sends, the change on its way first.
    pub sends: EffectLevels,
}

impl EditorView {
    /// The open strip's label: "CH 3–4 · Keys", or "CH 3" before the inputs are known.
    pub fn label(&self) -> String {
        self.choice
            .as_ref()
            .map(|choice| choice.label.clone())
            .unwrap_or_else(|| format!("CH {}", self.channel))
    }

    /// The status line: the outcome when it worked.
    pub fn status_line(&self) -> String {
        match &self.outcome {
            Some(outcome) if outcome.ok => outcome.message.clone(),
            _ => String::new(),
        }
    }
}

struct State {
    open: usize,
    ab: Ab,
    outcome: Option<Said>,
    /// The curve asks waiting for their pause, the last one's number.
    curve_timer: u64,
    /// The curve asks sent, the last one's number: an older answer is dropped.
    curve_ask: u64,
    /// The equaliser the desk's curve of a channel was worked out for.
    curve_for: Option<(usize, EqSettings)>,
    failures: FailureLine,
    changed: Option<Rc<dyn Fn()>>,
}

/// The screen's model; clones share it.
#[derive(Clone)]
pub struct Model {
    strips: Strips,
    state: Rc<RefCell<State>>,
}

impl Model {
    /// The model over `console` (a handle that sends its commands for this screen), on CH 1.
    pub fn new(console: Console) -> Self {
        let model = Self {
            strips: Strips::new(&console),
            state: Rc::new(RefCell::new(State {
                open: 1,
                ab: Ab::new(),
                outcome: None,
                curve_timer: 0,
                curve_ask: 0,
                curve_for: None,
                failures: FailureLine::default(),
                changed: None,
            })),
        };
        let settled = model.clone();
        console.subscribe(move |_, _| settled.settle());
        model.normalise();
        model
    }

    /// The console this screen talks to the desk through.
    pub fn console(&self) -> &Console {
        self.strips.console()
    }

    /// The processing commands.
    pub fn strips(&self) -> &Strips {
        &self.strips
    }

    /// Calls `changed` whenever the model's own state changes outside the console's (an outcome,
    /// the desk's curve, a bypass), so the view draws again.
    pub fn on_change(&self, changed: impl Fn() + 'static) {
        self.state.borrow_mut().changed = Some(Rc::new(changed));
    }

    fn changed(&self) {
        let changed = self.state.borrow().changed.clone();
        if let Some(changed) = changed {
            changed();
        }
    }

    fn say(&self, said: Said) -> Said {
        self.state.borrow_mut().outcome = Some(said.clone());
        self.changed();
        said
    }

    fn choices(&self) -> Vec<StripChoice> {
        strip_choices(self.console().state().engine.status.as_ref())
    }

    fn choice_of(&self, number: usize) -> Option<StripChoice> {
        self.choices()
            .into_iter()
            .find(|choice| choice.sides.contains(&number))
    }

    /// The open channel moves to the choice holding it, else the first; it stays while there is
    /// none.
    fn normalise(&self) {
        let open = self.open_channel();
        let choice = self
            .choice_of(open)
            .or_else(|| self.choices().into_iter().next());
        if let Some(choice) = choice {
            self.state.borrow_mut().open = choice.channel;
        }
    }

    /// Ends the bypass of every strip the desk now shows working with no edit of the editor's on
    /// its way: it was changed elsewhere.
    fn settle(&self) {
        let bypassed: Vec<usize> = self.state.borrow().ab.keys().copied().collect();
        for channel in bypassed {
            let shown = self.console().state().channel_strip(channel);
            let settled = {
                let mut state = self.state.borrow_mut();
                let next = ab_settle(&state.ab, channel, &shown);
                let changed = next != state.ab;
                state.ab = next;
                changed
            };
            if settled {
                self.changed();
            }
        }
    }

    /// The strip open (its first side, 1-based).
    pub fn open_channel(&self) -> usize {
        self.state.borrow().open
    }

    fn bypassed(&self) -> bool {
        let state = self.state.borrow();
        state.ab.contains_key(&state.open)
    }

    /// The strip shown: the kept one while bypassed, else the console's.
    fn shown_strip(&self) -> StripSettings {
        let (open, kept) = {
            let state = self.state.borrow();
            (state.open, state.ab.get(&state.open).copied())
        };
        kept.unwrap_or_else(|| self.console().state().channel_strip(open).settings)
    }

    fn bypass_refusal(channel: usize) -> Said {
        Said::refused(format!(
            "CH {channel} is bypassed: turn the bypass off to change its processing."
        ))
    }

    /// What the screen draws now.
    pub fn view(&self) -> EditorView {
        self.normalise();
        self.settle();
        let console = self.console().clone();
        let state = console.state();
        let choices = strip_choices(state.engine.status.as_ref());
        let mut editor = self.state.borrow_mut();
        let open = editor.open;
        let choice = choices
            .iter()
            .find(|choice| choice.sides.contains(&open))
            .cloned();
        let strip = state.channel_strip(open);
        let bypassed = editor.ab.contains_key(&open);
        let settings = editor.ab.get(&open).copied().unwrap_or(strip.settings);
        let asked = editor
            .curve_for
            .as_ref()
            .filter(|(channel, _)| *channel == open)
            .map(|(_, eq)| *eq);
        let curve = curve_source(
            asked.as_ref(),
            &settings.eq,
            state.eq_curves.get(&open).map(Vec::as_slice),
            EQ_RESPONSE_POINTS,
        );
        let sides = choice
            .as_ref()
            .map(|choice| choice.sides.clone())
            .unwrap_or_else(|| vec![open]);
        let error = state.processing_error.as_ref().and_then(|error| {
            let about = error.channel.is_none_or(|channel| {
                choice
                    .as_ref()
                    .is_some_and(|choice| choice.sides.contains(&channel))
            });
            about.then(|| error.message.clone())
        });
        let outcome = editor.outcome.clone();
        let failure = editor
            .failures
            .text(open, outcome.as_ref(), error.as_deref());
        EditorView {
            known: choice.is_some(),
            unread: state.processing.is_none(),
            presets: preset_options(state.strip_presets.as_deref()),
            sends: channel_sends(&state, open),
            choices,
            choice,
            channel: open,
            sides,
            settings,
            active: strip.active,
            pending: strip.pending,
            bypassed,
            can_bypass: bypassed || strip.active,
            curve,
            outcome,
            error,
            failure,
        }
    }

    /// The screen is shown, on `channel` when the route names one: it opens that strip (or the
    /// one open last), starts with no outcome, reads the strip presets when they are not known
    /// and asks for the curve.
    pub fn shown(&self, channel: Option<u8>) {
        {
            let mut state = self.state.borrow_mut();
            state.outcome = None;
            state.failures = FailureLine::default();
        }
        if let Some(channel) = channel {
            self.open(usize::from(channel));
        }
        self.normalise();
        if self.console().state().strip_presets.is_none() {
            let read = self.console().load_strip_presets();
            self.console().spawn(read);
        }
        self.request_curve();
    }

    /// Opens the strip holding `number`; answers the channel open (unchanged when it has none).
    /// Opening clears the outcome.
    pub fn open(&self, number: usize) -> usize {
        if let Some(choice) = self.choice_of(number) {
            let mut state = self.state.borrow_mut();
            state.open = choice.channel;
            state.outcome = None;
        }
        self.open_channel()
    }

    /// Opens the strip `delta` places away (held at the ends); answers the channel open.
    pub fn step(&self, delta: i32) -> usize {
        let next = step_choice(&self.choices(), self.open_channel(), delta);
        self.open(next)
    }

    /// Sends the whole strip `settings` for the open channel unless it is bypassed: only a
    /// refusal is said, so a drag does not announce every step.
    fn send(&self, settings: StripSettings) -> impl Future<Output = Said> + 'static {
        let model = self.clone();
        let channel = self.open_channel();
        let edit = (!self.bypassed())
            .then(|| self.strips.edit_processing(channel, clamp_strip(&settings)));
        async move {
            match edit {
                None => model.say(Self::bypass_refusal(channel)),
                Some(edit) => {
                    let result = edit.await;
                    if result.ok { result } else { model.say(result) }
                }
            }
        }
    }

    /// Sets `part`'s `key` of the open strip to `value`.
    pub fn edit_number(
        &self,
        part: Part,
        key: Key,
        value: f64,
    ) -> impl Future<Output = Said> + 'static {
        self.send(with_number(&self.shown_strip(), part, key, value))
    }

    /// Turns `part` of the open strip on or off.
    pub fn edit_enabled(&self, part: Part, on: bool) -> impl Future<Output = Said> + 'static {
        self.send(with_enabled(&self.shown_strip(), part, on))
    }

    /// Sets the open strip's equaliser to `eq` (a handle dragged on the graph).
    pub fn edit_eq(&self, eq: EqSettings) -> impl Future<Output = Said> + 'static {
        let mut settings = self.shown_strip();
        settings.eq = eq;
        self.send(settings)
    }

    /// The open strip's equaliser as shown now, a gesture not drawn yet included.
    pub fn shown_eq(&self) -> EqSettings {
        self.shown_strip().eq
    }

    /// A/B: bypasses the open strip, or puts its kept strip back.
    pub fn toggle_bypass(&self) -> impl Future<Output = Said> + 'static {
        let model = self.clone();
        let channel = self.open_channel();
        let kept = self.state.borrow().ab.get(&channel).copied();
        if let Some(kept) = kept {
            // The kept strip stays here until the desk took it back: a send that fails leaves the
            // channel bypassed with its strip still kept, to try again.
            let edit = self.strips.edit_processing(channel, kept);
            return futures_either(async move {
                let result = edit.await;
                if !result.ok {
                    return model.say(result);
                }
                {
                    let mut state = model.state.borrow_mut();
                    if state.ab.get(&channel) == Some(&kept) {
                        state.ab = ab_restore(&state.ab, channel).0;
                    }
                }
                model.say(Said::ok(format!("CH {channel}'s processing is back on.")))
            });
        }
        let strip = self.console().state().channel_strip(channel);
        if !strip.active {
            let said = self.say(Said::refused(format!(
                "CH {channel} has no processing to bypass."
            )));
            return futures_either(async move { said });
        }
        let send = {
            let mut state = self.state.borrow_mut();
            let (ab, send) = ab_bypass(&state.ab, channel, &strip.settings);
            state.ab = ab;
            send
        };
        self.changed();
        let edit = self.strips.edit_processing(channel, send);
        futures_either(async move {
            let result = edit.await;
            if !result.ok {
                {
                    let mut state = model.state.borrow_mut();
                    state.ab = ab_restore(&state.ab, channel).0;
                }
                return model.say(result);
            }
            model.say(Said::ok(format!(
                "CH {channel} is bypassed: you hear it with no processing."
            )))
        })
    }

    /// Runs a change of the open strip unless it is bypassed, and says what it did.
    fn change(
        &self,
        run: impl FnOnce(&Strips, usize) -> std::pin::Pin<Box<dyn Future<Output = Said>>>,
    ) -> impl Future<Output = Said> + 'static {
        let model = self.clone();
        let channel = self.open_channel();
        let command = (!self.bypassed()).then(|| run(&self.strips, channel));
        async move {
            match command {
                None => model.say(Self::bypass_refusal(channel)),
                Some(command) => {
                    let said = command.await;
                    model.say(said)
                }
            }
        }
    }

    /// Takes every part of the open strip off.
    pub fn reset(&self) -> impl Future<Output = Said> + 'static {
        self.change(|strips, channel| Box::pin(strips.reset(channel)))
    }

    /// Copies the open strip onto `targets` (1-based first sides).
    pub fn copy_to(&self, targets: Vec<usize>) -> impl Future<Output = Said> + 'static {
        let model = self.clone();
        let channel = self.open_channel();
        let picked = !targets.is_empty();
        let copy =
            picked.then(|| self.change(move |strips, from| Box::pin(strips.copy(from, targets))));
        async move {
            match copy {
                Some(copy) => copy.await,
                None => model.say(Said::refused(format!(
                    "Pick the channels to copy CH {channel}'s processing to."
                ))),
            }
        }
    }

    /// Puts the strip preset `name` on the open strip.
    pub fn apply_preset(&self, name: impl Into<String>) -> impl Future<Output = Said> + 'static {
        let name = name.into();
        self.change(move |strips, channel| Box::pin(strips.apply_preset(name, channel)))
    }

    /// Saves the open strip as the strip preset `name` (trimmed).
    pub fn save_preset(&self, name: impl Into<String>) -> impl Future<Output = Said> + 'static {
        let model = self.clone();
        let trimmed = name.into().trim().to_owned();
        let save =
            (!trimmed.is_empty()).then(|| self.strips.save_preset(trimmed, self.shown_strip()));
        async move {
            match save {
                None => model.say(Said::refused("Name the preset to save it.")),
                Some(save) => {
                    let said = save.await;
                    model.say(said)
                }
            }
        }
    }

    /// Deletes the engineer's strip preset `name`.
    pub fn delete_preset(&self, name: impl Into<String>) -> impl Future<Output = Said> + 'static {
        let model = self.clone();
        let delete = self.strips.delete_preset(name.into());
        async move {
            let said = delete.await;
            model.say(said)
        }
    }

    /// Reads every strip again, after a read that failed; `None`, or why it failed again.
    pub fn read_again(&self) -> impl Future<Output = Option<String>> + 'static {
        self.console().load_processing()
    }

    /// Changes the open strip's send to `effect` (stays possible while bypassed: the sends are
    /// not part of the processing); a refusal goes to the failure line.
    pub fn edit_send(&self, effect: Effect, level: f64) -> impl Future<Output = Said> + 'static {
        let model = self.clone();
        let channel = self.open_channel();
        let send = self.strips.edit_send(channel, effect, level);
        async move {
            let said = send.await;
            model.state.borrow_mut().failures.sent(channel, &said);
            model.changed();
            said
        }
    }

    /// Asks the desk for the open strip's curve once the edits pause for [CURVE_DEBOUNCE]; an
    /// answer for an older ask is dropped, and a failed ask says nothing.
    pub fn request_curve(&self) {
        let ticket = {
            let mut state = self.state.borrow_mut();
            state.curve_timer += 1;
            state.curve_timer
        };
        let model = self.clone();
        let pause = self.console().sleep(CURVE_DEBOUNCE);
        self.console().spawn(async move {
            pause.await;
            if model.state.borrow().curve_timer != ticket {
                return;
            }
            let channel = model.open_channel();
            let eq = model.shown_strip().eq;
            let ask = {
                let mut state = model.state.borrow_mut();
                state.curve_ask += 1;
                state.curve_ask
            };
            if model.strips.load_eq_response(channel, eq).await.is_ok()
                && model.state.borrow().curve_ask == ask
            {
                model.state.borrow_mut().curve_for = Some((channel, eq));
                model.changed();
            }
        });
    }

    /// The screen is left: every bypass ends, its kept strip sent back; resolves once they were.
    pub fn leave(&self) -> impl Future<Output = ()> + 'static {
        let kept: Vec<(usize, StripSettings)> = std::mem::take(&mut self.state.borrow_mut().ab)
            .into_iter()
            .collect();
        let edits: Vec<_> = kept
            .into_iter()
            .map(|(channel, strip)| self.strips.edit_processing(channel, strip))
            .collect();
        async move {
            for edit in edits {
                edit.await;
            }
        }
    }
}

/// One future type for the three ways a bypass toggle goes.
fn futures_either(
    flow: impl Future<Output = Said> + 'static,
) -> std::pin::Pin<Box<dyn Future<Output = Said>>> {
    Box::pin(flow)
}

#[cfg(test)]
mod tests;
