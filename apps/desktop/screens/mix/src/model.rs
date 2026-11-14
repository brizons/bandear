//! The Mix screen's model: its own state and flows over the console (the Mix part of
//! `console-app.js` and the screen's wiring in `mix-screen.js`). Pure: no window, tested with plain
//! `#[test]`s over the console's simulated desk.
//!
//! The model owns the lanes of the commands only this screen sends: `set_mix` and `set_local_mix`
//! by mix, `set_engineer_controls` by musician, the master limiter by mix and the effect returns by
//! mix and effect. Each lane has one send in flight and sends the latest after it (C27); the mix
//! and engineer lanes build their request from the edits waiting when they send, and skip while a
//! saved session loads (C31), sending what still waits once the hold ends. The Mix tools' commands
//! (presets, copy, mirror) answer in words and read the status after.

use std::cell::RefCell;
use std::collections::HashMap;
use std::future::Future;
use std::rc::Rc;

use bandear_desktop_console::coalesce::Lanes;
use bandear_desktop_console::command::Command;
use bandear_desktop_console::console::Console;
use bandear_desktop_console::format::{musician_label, sentence_with_reason};
use bandear_desktop_console::mix::{EditKey, EditValue, Edits, Mix, MixField, split_edits};
use bandear_desktop_console::slider::SliderEvent;
use bandear_desktop_console::state::{Action, ConsoleState, LOCAL_MIX};
use bandear_desktop_console::types::dto::{
    EngineerControlsSetDto, FxReturnSetDto, LimiterSetDto, MixDto,
};
use bandear_desktop_console::types::{Effect, EffectLevels, LimiterSettings};
use bandear_desktop_console::wire::{Musician, effect_name};

use crate::Said;
use crate::limiter::{
    LimiterControl, LimiterField, LimiterView, ReturnsView, SliderKeys, clamp_limiter,
    format_limiter_set, format_return_set, limiter_change, limiter_reduction, master_limiter,
    returns_view,
};
use crate::mix_view::{
    ChannelStrip, GroupStrip, MasterStrip, MixMessages, MixNotice, MixTab, TopBarView,
    channel_edits, channel_strips, group_strips, master_levels, master_strip, mix_messages,
    mix_notice, mix_tabs, strip_level, top_bar_view,
};
use crate::request::{build_engineer_request, build_mix_request};
use crate::tools::{Clears, Tools, mix_tools_view};

/// The key the desk knows a musician's limiter and returns by: their phone's, else their address.
pub fn musician_key(musician: &Musician) -> String {
    match &musician.client_id {
        Some(id) => format!("client:{id}"),
        None => format!("address:{}", musician.address),
    }
}

fn label_of(musician: &Musician) -> String {
    musician_label(musician.name.as_deref(), &musician.address)
}

fn label_at(state: &ConsoleState, address: &str) -> String {
    state
        .musician_by_address(address)
        .map_or_else(|| address.to_owned(), label_of)
}

/// A control of the master strip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MasterControl {
    Level,
    /// The phone's own level cap.
    Cap,
    /// The engineer's limit.
    Limit,
}

/// The edit a gesture on the master's faders makes (`wireMasterControls`): the level and the
/// phone's cap as they move, the engineer's limit only when a gesture ends (the desk keeps every
/// limit it is given, so a drag sends its last value only).
pub fn master_edit(control: MasterControl, event: SliderEvent) -> Option<(MixField, EditValue)> {
    let level = |value: i32| EditValue::Level(value.clamp(0, 100) as u8);
    match (control, event) {
        (MasterControl::Level, SliderEvent::Input(value)) => {
            Some((MixField::VolumePercent, level(value)))
        }
        (MasterControl::Cap, SliderEvent::Input(value)) => {
            Some((MixField::MaxLevelPercent, level(value)))
        }
        (MasterControl::Limit, SliderEvent::Change(value)) => {
            Some((MixField::EngineerLimitPercent, level(value)))
        }
        _ => None,
    }
}

/// The open mix as the screen draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct Body {
    pub address: String,
    /// The tab panel's name: "Local mix" or "<name>'s mix".
    pub label: String,
    pub local: bool,
    pub strips: Vec<ChannelStrip>,
    /// Each strip's input meter, from the latest meters.
    pub strip_levels: Vec<u8>,
    pub groups: Vec<GroupStrip>,
    pub master: MasterStrip,
    /// The master's output meters, left and right.
    pub master_levels: (u8, u8),
    pub limiter: LimiterView,
    /// The limiter's gain reduction now, in dB (none while it is off).
    pub reduction_db: f64,
    pub returns: ReturnsView,
}

/// Everything the screen draws, from the console and the screen's own state.
#[derive(Clone, Debug, PartialEq)]
pub struct Page {
    pub top_bar: TopBarView,
    pub tabs: Vec<MixTab>,
    /// The tools show while a musician (not the local mix) is open.
    pub tools: bool,
    /// The last outcome of the tools, or a limiter or return change the desk refused.
    pub outcome: Option<Said>,
    pub messages: MixMessages,
    pub notice: Option<MixNotice>,
    pub body: Option<Body>,
}

/// The screen's own state, shared by its flows.
#[derive(Default)]
struct Local {
    tools: Tools,
    outcome: Option<Said>,
    limiter_said: HashMap<String, Said>,
    return_said: HashMap<String, Said>,
    clears: Clears,
    /// True while another screen shows: answers that come meanwhile say nothing.
    away: bool,
    keys: SliderKeys,
    listener: Option<Rc<dyn Fn()>>,
}

type Shared = Rc<RefCell<Local>>;

/// Tells the view that the screen's own state changed (the console's changes reach it through the
/// hub).
fn notify(local: &Shared) {
    let listener = local.borrow().listener.clone();
    if let Some(listener) = listener {
        listener();
    }
}

/// Gives the tools the open musician's data, as the web screen did on every render: the tools then
/// hold the status an answer brought before they hear the answer.
fn refresh_tools(console: &Console, local: &Shared) {
    let view = {
        let state = console.state();
        state
            .selected_musician()
            .map(|musician| mix_tools_view(&state, &musician.address))
    };
    if let Some(view) = view {
        local.borrow_mut().tools.set_data(view);
    }
}

/// The screen's model, built once as the window opens and kept for its life, so its flows and
/// lanes go on while another screen is shown. Clones share it.
#[derive(Clone)]
pub struct Model {
    console: Console,
    mixes: Lanes<String, ()>,
    engineer: Lanes<String, ()>,
    limiters: Lanes<String, (u64, LimiterSettings)>,
    returns: Lanes<String, (String, Effect, u8)>,
    local: Shared,
}

impl Model {
    /// The model over `console` (a handle that sends its commands for this screen).
    pub fn new(console: Console) -> Self {
        let local = Shared::default();
        let mixes = {
            let sender = console.clone();
            Lanes::new(
                &console,
                Rc::new(move |address: String, (): ()| {
                    let console = sender.clone();
                    Box::pin(async move {
                        send_mix(console, address).await;
                        Ok(())
                    })
                }),
            )
        };
        let engineer = {
            let sender = console.clone();
            Lanes::new(
                &console,
                Rc::new(move |address: String, (): ()| {
                    let console = sender.clone();
                    Box::pin(async move {
                        send_engineer(console, address).await;
                        Ok(())
                    })
                }),
            )
        };
        let limiters = {
            let (sender, kept) = (console.clone(), Rc::clone(&local));
            Lanes::new(
                &console,
                Rc::new(move |address: String, sent: (u64, LimiterSettings)| {
                    let (console, local) = (sender.clone(), Rc::clone(&kept));
                    Box::pin(async move {
                        send_limiter(console, local, address, sent).await;
                        Ok(())
                    })
                }),
            )
        };
        let returns = {
            let (sender, kept) = (console.clone(), Rc::clone(&local));
            Lanes::new(
                &console,
                Rc::new(move |key: String, sent: (String, Effect, u8)| {
                    let (console, local) = (sender.clone(), Rc::clone(&kept));
                    Box::pin(async move {
                        send_return(console, local, key, sent).await;
                        Ok(())
                    })
                }),
            )
        };
        // A refused session load changed nothing: the edits it held go out as the lanes would have
        // sent them. After a load none is left (the loaded session dropped them).
        {
            let (mixes, engineer) = (mixes.clone(), engineer.clone());
            console.on_sends_released(move |console| {
                let waiting: Vec<(String, bool, bool)> = console
                    .state()
                    .pending
                    .iter()
                    .map(|(address, edits)| {
                        let (own, controls) = split_edits(edits);
                        (address.clone(), !own.is_empty(), !controls.is_empty())
                    })
                    .collect();
                for (address, own, controls) in waiting {
                    if own {
                        mixes.push(address.clone(), ());
                    }
                    if controls {
                        engineer.push(address, ());
                    }
                }
            });
        }
        {
            let local = Rc::clone(&local);
            console.subscribe(move |console, _| refresh_tools(console, &local));
        }
        refresh_tools(&console, &local);
        Self {
            console,
            mixes,
            engineer,
            limiters,
            returns,
            local,
        }
    }

    /// The console this screen talks to the desk through.
    pub fn console(&self) -> &Console {
        &self.console
    }

    /// Calls `listener` whenever the screen's own state changes (an outcome, the tools), so the
    /// view draws again.
    pub fn on_change(&self, listener: impl Fn() + 'static) {
        self.local.borrow_mut().listener = Some(Rc::new(listener));
    }

    /// Everything the screen draws now.
    pub fn page(&self) -> Page {
        let meters = self.console.meters();
        let state = self.console.state();
        let local = self.local.borrow();
        let notice = mix_notice(&state);
        let address = state.selected.clone();
        let is_local = address.as_deref() == Some(LOCAL_MIX) && state.local_mix.is_some();
        let musician = state.selected_musician();
        let body = match (&notice, &address) {
            (None, Some(address)) if is_local || musician.is_some() => {
                let strips = channel_strips(&state, address);
                let channels = meters.as_ref().map(|meters| meters.channels.as_slice());
                let limiter = master_limiter(&state, address);
                master_strip(&state, address)
                    .zip(limiter)
                    .map(|(master, limiter)| Body {
                        address: address.clone(),
                        label: match musician {
                            Some(musician) if !is_local => {
                                format!("{}'s mix", label_of(musician))
                            }
                            _ => "Local mix".to_owned(),
                        },
                        local: is_local,
                        strip_levels: strips
                            .iter()
                            .map(|strip| strip_level(&strip.sides, channels.unwrap_or_default()))
                            .collect(),
                        strips,
                        groups: group_strips(&state, address),
                        master,
                        master_levels: master_levels(meters.as_deref(), Some(address)),
                        reduction_db: if limiter.settings.enabled {
                            limiter_reduction(meters.as_deref(), address)
                        } else {
                            0.0
                        },
                        limiter,
                        returns: returns_view(&state, address),
                    })
            }
            _ => None,
        };
        Page {
            top_bar: top_bar_view(&state),
            tabs: mix_tabs(&state),
            tools: musician.is_some(),
            outcome: local.outcome.clone(),
            messages: mix_messages(&state),
            notice,
            body,
        }
    }

    // ---- Selection and the engine ---------------------------------------------------------------

    /// Opens `address`'s mix (a musician's, or [LOCAL_MIX]).
    pub fn select(&self, address: &str) {
        self.console.select(address);
    }

    /// Starts the engine (Start engine, Try again).
    pub fn start(&self) {
        let start = self.console.start();
        self.console.spawn(start);
    }

    pub fn stop(&self) {
        let stop = self.console.stop();
        self.console.spawn(stop);
    }

    /// Connects again (Try again after a lost connection).
    pub fn reconnect(&self) {
        let connect = self.console.connect();
        self.console.spawn(connect);
    }

    // ---- Mix edits --------------------------------------------------------------------------------

    /// Local edits to `address`'s mix: shown at once, the mix's own fields sent with `set_mix` (or
    /// `set_local_mix`), the engineer's limit and locks with `set_engineer_controls`.
    pub fn edit(&self, address: &str, edits: Edits) {
        let (own, controls) = split_edits(&edits);
        if !self.console.dispatch(Action::MixEdit {
            address: address.to_owned(),
            edits,
        }) {
            return;
        }
        if !own.is_empty() {
            self.mixes.push(address.to_owned(), ());
        }
        if !controls.is_empty() {
            self.engineer.push(address.to_owned(), ());
        }
    }

    fn selected(&self) -> Option<String> {
        self.console.state().selected.clone()
    }

    /// A move on a channel strip of the open mix: to every input it stands for.
    pub fn edit_channel(&self, sides: &[usize], stereo: bool, field: MixField, value: EditValue) {
        if let Some(address) = self.selected() {
            self.edit(&address, channel_edits(sides, stereo, field, value));
        }
    }

    /// A move of the group at `index` (its position) in the open mix.
    pub fn edit_group(&self, index: usize, level: u8) {
        if let (Some(address), Ok(key)) = (
            self.selected(),
            EditKey::new(MixField::GroupLevels, Some(index)),
        ) {
            self.edit(&address, Edits::from([(key, EditValue::Level(level))]));
        }
    }

    /// A change of the open mix's master: its level, cap, limit or mute.
    pub fn edit_master(&self, field: MixField, value: EditValue) {
        if let (Some(address), Ok(key)) = (self.selected(), EditKey::new(field, None)) {
            self.edit(&address, Edits::from([(key, value)]));
        }
    }

    // ---- The limiter and the returns -------------------------------------------------------------

    /// Changes the limiter of the mix `address` to `settings`: shown at once, sent through the mix's
    /// lane; resolves, once that mix's changes settled, to the last answer in words.
    pub fn edit_mix_limiter(
        &self,
        address: &str,
        settings: LimiterSettings,
    ) -> impl Future<Output = Said> + 'static {
        let sent = clamp_limiter(&settings);
        self.console.dispatch(Action::LimiterEdit {
            address: address.to_owned(),
            settings: sent,
        });
        let stamp = self
            .console
            .state()
            .limiter_pending
            .get(address)
            .map_or(0, |waiting| waiting.stamp);
        self.limiters.push(address.to_owned(), (stamp, sent));
        let (lanes, local, address) = (
            self.limiters.clone(),
            Rc::clone(&self.local),
            address.to_owned(),
        );
        async move {
            lanes.idle(&address).await;
            local
                .borrow()
                .limiter_said
                .get(&address)
                .cloned()
                .unwrap_or_else(|| Said::refused("The limiter was not set."))
        }
    }

    /// A key went down on a limiter slider.
    pub fn limiter_key_down(&self, field: LimiterField, key: &str) {
        self.local.borrow_mut().keys.key_down(field, key);
    }

    pub fn limiter_key_up(&self) {
        self.local.borrow_mut().keys.key_up();
    }

    pub fn limiter_pointer_down(&self) {
        self.local.borrow_mut().keys.pointer_down();
    }

    /// A limiter control of the open mix changed: the whole limiter is worked out from the one shown
    /// (the change on its way first) and sent; only a refusal is said.
    pub fn limiter_edit(&self, control: LimiterControl) {
        let shown = {
            let state = self.console.state();
            state.selected.clone().and_then(|address| {
                master_limiter(&state, &address).map(|limiter| (address, limiter.settings))
            })
        };
        let Some((address, shown)) = shown else {
            return;
        };
        let key = match control {
            LimiterControl::Slider(field, _) => self.local.borrow().keys.keyed(field),
            LimiterControl::Enabled(_) => false,
        };
        let said = self.edit_mix_limiter(&address, limiter_change(&shown, control, key));
        self.say_refusal(said);
    }

    /// Changes the return of `effect` in the mix `address` to `level`: shown at once, sent through a
    /// lane per mix and effect; resolves once settled to the last answer in words.
    pub fn edit_mix_return(
        &self,
        address: &str,
        effect: Effect,
        level: u8,
    ) -> impl Future<Output = Said> + 'static {
        let held = level.min(100);
        self.console.dispatch(Action::ReturnEdit {
            address: address.to_owned(),
            effect,
            level: held,
        });
        let key = format!("{address}|{}", effect_name(effect));
        self.returns
            .push(key.clone(), (address.to_owned(), effect, held));
        let (lanes, local) = (self.returns.clone(), Rc::clone(&self.local));
        async move {
            lanes.idle(&key).await;
            local
                .borrow()
                .return_said
                .get(&key)
                .cloned()
                .unwrap_or_else(|| Said::refused("The return was not set."))
        }
    }

    /// A return slider of the open mix moved; only a refusal is said.
    pub fn return_edit(&self, effect: Effect, level: u8) {
        if let Some(address) = self.selected() {
            let said = self.edit_mix_return(&address, effect, level);
            self.say_refusal(said);
        }
    }

    /// Says `said` above the strips once it comes, when it is a refusal and the screen still shows.
    fn say_refusal(&self, said: impl Future<Output = Said> + 'static) {
        let local = Rc::clone(&self.local);
        self.console.spawn(async move {
            let said = said.await;
            if said.ok || local.borrow().away {
                return;
            }
            local.borrow_mut().outcome = Some(said);
            notify(&local);
        });
    }
}

// The screen's life beside the others.
impl Model {
    /// Another screen shows: answers that come meanwhile say nothing, and the tools start afresh
    /// when the screen shows again, as the web screen did when it was mounted again.
    pub fn leave(&self) {
        let mut local = self.local.borrow_mut();
        local.away = true;
        local.outcome = None;
        local.tools.reset();
        local.clears = Clears::default();
    }

    /// The screen shows again.
    pub fn enter(&self) {
        self.local.borrow_mut().away = false;
        refresh_tools(&self.console, &self.local);
    }
}

/// Sends `address`'s waiting mix edits as they are when the send goes out: a musician's with
/// `set_mix`, the local mix's with `set_local_mix`.
async fn send_mix(console: Console, address: String) {
    let prepared = {
        let state = console.state();
        if console.holding() {
            None
        } else {
            state.mix_of(&address).and_then(|mix| {
                let (sent, _) = split_edits(&state.pending_edits(&address));
                build_mix_request(mix, &sent).map(|request| (sent, request))
            })
        }
    };
    let Some((sent, request)) = prepared else {
        return;
    };
    let command = if address == LOCAL_MIX {
        Command::SetLocalMix { mix: request }
    } else {
        Command::SetMix {
            address: address.clone(),
            mix: request,
        }
    };
    let action = match console.call::<MixDto>(command).await {
        Ok(mix) => Action::MixApplied {
            address,
            sent,
            mix: Some(Mix::from_core(&mix)),
        },
        Err(error) => Action::MixFailed {
            address,
            sent,
            error,
        },
    };
    console.dispatch(action);
}

/// Sends `address`'s waiting edits of the engineer's controls, as [send_mix] sends the mix's, with
/// the row's phone so a desk with no engine keeps them for the phone.
async fn send_engineer(console: Console, address: String) {
    let prepared = {
        let state = console.state();
        if console.holding() {
            None
        } else {
            state.musician_by_address(&address).and_then(|musician| {
                let (_, sent) = split_edits(&state.pending_edits(&address));
                build_engineer_request(&musician.mix, &sent)
                    .map(|request| (sent, request, musician.client_id.clone()))
            })
        }
    };
    let Some((sent, controls, client_id)) = prepared else {
        return;
    };
    let action = match console
        .call::<EngineerControlsSetDto>(Command::SetEngineerControls {
            address: address.clone(),
            client_id,
            controls,
        })
        .await
    {
        Ok(outcome) => Action::EngineerApplied {
            address,
            sent,
            outcome: Box::new(outcome),
        },
        Err(error) => Action::MixFailed {
            address,
            sent,
            error,
        },
    };
    console.dispatch(action);
}

/// Who a mix is, for its sentences and its command: its name, and the desk's key for a musician's
/// (`None` for the local mix, or a musician no longer listed).
fn whose(console: &Console, address: &str) -> (String, Option<String>) {
    if address == LOCAL_MIX {
        return ("The local mix".to_owned(), None);
    }
    let state = console.state();
    match state.musician_by_address(address) {
        Some(musician) => (label_of(musician), Some(musician_key(musician))),
        None => (address.to_owned(), None),
    }
}

/// "The local mix's" or "Ana's".
fn possessive(name: &str) -> String {
    if name == "The local mix" {
        "The local mix's".to_owned()
    } else {
        format!("{name}'s")
    }
}

/// Sends the limiter change `sent` of the mix `address`, then reads the status after its answer.
/// Only the command decides whether it was set: a status that cannot be read after a set leaves the
/// set, and the strip shows what the desk answered until the next status. The change shown
/// meanwhile is settled on every path, so the strip never stays on a change no longer on its way.
async fn send_limiter(
    console: Console,
    local: Shared,
    address: String,
    sent: (u64, LimiterSettings),
) {
    let (stamp, settings) = sent;
    let (name, key) = whose(&console, &address);
    let refused = |reason: &str| {
        Said::refused(sentence_with_reason(
            &format!("{} limiter was not set", possessive(&name)),
            Some(reason),
        ))
    };
    let mut kept = None;
    let command = match (address == LOCAL_MIX, key) {
        (true, _) => Some(Command::SetLocalLimiter { settings }),
        (false, Some(musician_key)) => Some(Command::SetMusicianLimiter {
            musician_key,
            settings,
        }),
        (false, None) => None,
    };
    let said = match command {
        None => refused(&format!("{name} is not here")),
        Some(command) => match console.call::<LimiterSetDto>(command).await {
            Ok(answer) => {
                let said = format_limiter_set(Some(&answer), &name);
                let set = said.ok.then(|| clamp_limiter(&answer.settings));
                local
                    .borrow_mut()
                    .limiter_said
                    .insert(address.clone(), said.clone());
                if console.read_status().await.is_some() {
                    kept = set;
                }
                said
            }
            Err(error) => refused(&error),
        },
    };
    local
        .borrow_mut()
        .limiter_said
        .insert(address.clone(), said);
    console.dispatch(Action::LimiterSettled {
        address,
        sent: Some(stamp),
        kept,
    });
}

/// Sends the return `level` of `effect` in the mix `address` and keeps the mix's returns as
/// answered until the next status.
async fn send_return(console: Console, local: Shared, key: String, sent: (String, Effect, u8)) {
    let (address, effect, level) = sent;
    let (name, musician) = whose(&console, &address);
    let refused = |reason: &str| {
        Said::refused(sentence_with_reason(
            &format!(
                "{} {} return was not set",
                possessive(&name),
                effect_name(effect)
            ),
            Some(reason),
        ))
    };
    let command = match (address == LOCAL_MIX, musician) {
        (true, _) => Some(Command::SetLocalFxReturn { effect, level }),
        (false, Some(musician_key)) => Some(Command::SetMusicianFxReturn {
            musician_key,
            effect,
            level,
        }),
        (false, None) => None,
    };
    let (said, returns) = match command {
        None => (refused(&format!("{name} is not here")), None),
        Some(command) => match console.call::<FxReturnSetDto>(command).await {
            Ok(answer) => {
                let said = format_return_set(Some(&answer), &name);
                let returns = said.ok.then_some(EffectLevels {
                    reverb: answer.returns.reverb.min(100),
                    delay: answer.returns.delay.min(100),
                });
                (said, returns)
            }
            Err(error) => (refused(&error), None),
        },
    };
    local.borrow_mut().return_said.insert(key, said);
    console.dispatch(Action::ReturnSettled {
        address,
        effect,
        level,
        returns,
    });
}

mod commands;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod tests_tools;
