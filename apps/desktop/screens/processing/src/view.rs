//! The Processing screen's GPUI view (`processing-screen.js`): the header, the note while the
//! strips cannot be shown (with Read again), the outcome and failure lines, then the editor of one
//! strip: the action bar, the levels, the bypassed banner, the equaliser, the gate, the compressor
//! and the effect sends. The view draws the [Model]'s [EditorView] and hands every gesture back to
//! it; it keeps only its own controls' state (the selects, the preset name typed, the copy targets
//! picked, the handle being dragged).
//!
//! The screen is built once with the window: when the route comes to Processing it tells the
//! model it is shown (on the route's channel), when it leaves every bypass ends, and while it is
//! shown the route follows the strip open (as the web console kept its hash on it).

mod atoms;
mod bar;
mod eq;
mod parts;

use std::cell::Cell;
use std::collections::BTreeSet;
use std::rc::Rc;

use bandear_desktop_console::router::{Route, Section};
use bandear_desktop_kit::a11y::{heading, seen};
use bandear_desktop_kit::buttons::{Button, ButtonKind};
use bandear_desktop_kit::forms::{Choice, ChoiceState, choice_state, set_choices};
use bandear_desktop_kit::gpui::{
    AppContext as _, Bounds, Context, Entity, FocusHandle, InteractiveElement, IntoElement,
    ParentElement, Pixels, Render, Role, SharedString, StatefulInteractiveElement, Styled,
    Subscription, Task, Window, div, prelude::FluentBuilder, px,
};
use bandear_desktop_kit::gpui_component::input::{InputEvent, InputState};
use bandear_desktop_kit::gpui_component::select::SelectEvent;
use bandear_desktop_kit::hub::{Hub, Screen};
use bandear_desktop_kit::theme::{FONT_SANS, color, colors, radius, text};
use futures::StreamExt;

use crate::graph::Target;
use crate::model::{EditorView, Model, NOT_RUNNING, UNREAD};

/// The screen's lead paragraph.
const LEAD: &str =
    "Each input's processing is heard in every mix. Edit one channel, then copy it where it fits.";

/// The longest strip preset name the desk takes, in characters.
const MAX_PRESET_NAME: usize = 40;

/// The Processing screen.
pub struct ProcessingScreen {
    hub: Entity<Hub>,
    heading: FocusHandle,
    model: Model,
    channel: Entity<ChoiceState>,
    preset: Entity<ChoiceState>,
    save_name: Entity<InputState>,
    /// The choices the selects show now, to replace them only when they change.
    channel_choices: Vec<Choice>,
    preset_choices: Vec<Choice>,
    /// The preset picked in the select, if it is still listed.
    chosen_preset: Option<String>,
    copy_open: bool,
    copy_picked: BTreeSet<usize>,
    /// Whether Processing was the section shown at the last change of the route.
    on_screen: bool,
    /// Where the graph was last drawn, for a drag.
    graph: Rc<Cell<Bounds<Pixels>>>,
    dragging: Option<Target>,
    handles: Vec<FocusHandle>,
    _subscriptions: Vec<Subscription>,
    _woken: Task<()>,
}

impl ProcessingScreen {
    /// The screen's model.
    pub fn model(&self) -> &Model {
        &self.model
    }

    /// The hub the screen draws from.
    pub fn hub(&self) -> &Entity<Hub> {
        &self.hub
    }

    /// Follows the route: shown, left, or a channel named while shown; then keeps the route on
    /// the strip open and the selects in line.
    fn follow_route(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let route = self.hub.read(cx).route();
        let here = route.section == Section::Processing;
        if here && !self.on_screen {
            self.model.shown(route.channel);
            self.copy_open = false;
            self.copy_picked.clear();
        } else if here
            && let Some(channel) = route.channel
            && usize::from(channel) != self.model.open_channel()
        {
            self.opened(usize::from(channel), cx);
        } else if !here && self.on_screen {
            let leave = self.model.leave();
            self.model.console().spawn(leave);
        }
        self.on_screen = here;
        self.keep_route(cx);
        self.sync(window, cx);
    }

    /// Keeps the route on the strip open while the screen is shown (the web console's
    /// `history.replaceState`): it moves no focus.
    fn keep_route(&self, cx: &mut Context<Self>) {
        let route = self.hub.read(cx).route();
        let known = self.model.view().known;
        let open = u8::try_from(self.model.open_channel()).ok();
        if route.section == Section::Processing && known && route.channel != open {
            let canonical = Route {
                section: Section::Processing,
                channel: open,
            };
            self.hub.update(cx, |hub, cx| hub.navigate(canonical, cx));
        }
    }

    /// Opens the strip holding `channel`, asks for its curve and keeps the route on it.
    fn opened(&mut self, channel: usize, cx: &mut Context<Self>) {
        self.model.open(channel);
        self.model.request_curve();
        self.copy_picked.clear();
        self.keep_route(cx);
        cx.notify();
    }

    /// Brings the selects in line with the model: the strips and the presets, the strip open and
    /// the preset picked.
    fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = self.model.view();
        let channels: Vec<Choice> = view
            .choices
            .iter()
            .map(|choice| Choice::new(choice.channel.to_string(), choice.label.clone()))
            .collect();
        let open = view.channel.to_string();
        if channels != self.channel_choices {
            set_choices(&self.channel, channels.clone(), Some(&open), window, cx);
            self.channel_choices = channels;
        } else {
            let selected = self.channel.read(cx).selected_value().cloned();
            if selected.as_ref().map(|value| value.as_ref()) != Some(open.as_str()) {
                let open = SharedString::from(open);
                self.channel
                    .update(cx, |state, cx| state.set_selected_value(&open, window, cx));
            }
        }
        let presets: Vec<Choice> = view
            .presets
            .iter()
            .map(|preset| Choice::new(preset.name.clone(), preset.label.clone()))
            .collect();
        if self
            .chosen_preset
            .as_ref()
            .is_some_and(|name| !view.presets.iter().any(|preset| &preset.name == name))
        {
            self.chosen_preset = None;
        }
        // A select always shows one of its options: the first, until another is picked.
        if self.chosen_preset.is_none() {
            self.chosen_preset = view.presets.first().map(|preset| preset.name.clone());
        }
        if presets != self.preset_choices {
            set_choices(
                &self.preset,
                presets.clone(),
                self.chosen_preset.as_deref(),
                window,
                cx,
            );
            self.preset_choices = presets;
        }
    }

    /// Runs one of the model's commands and, once it answered, `then` with its outcome in the
    /// view (closing the copy popover, picking a saved preset).
    fn run_then(
        &self,
        flow: impl std::future::Future<Output = crate::strip::Said> + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, &crate::strip::Said, &mut Window, &mut Context<Self>) + 'static,
    ) {
        let (answered, answer) = futures::channel::oneshot::channel();
        self.model.console().spawn(async move {
            let said = flow.await;
            drop(answered.send(said));
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(said) = answer.await {
                drop(this.update_in(cx, |this, window, cx| {
                    then(this, &said, window, cx);
                    cx.notify();
                }));
            }
        })
        .detach();
    }

    /// Saves the open strip under the name typed; once saved the select shows the new preset and
    /// the field is cleared.
    fn save_preset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let typed = self.save_name.read(cx).value().to_string();
        let name = typed.trim().to_owned();
        let save = self.model.save_preset(typed);
        self.run_then(save, window, cx, move |this, said, window, cx| {
            if said.ok {
                this.chosen_preset = Some(name);
                this.save_name
                    .update(cx, |state, cx| state.set_value("", window, cx));
                this.preset_choices.clear();
                this.sync(window, cx);
            }
        });
    }

    /// The note while no engine runs, or while the strips could not be read (with Read again).
    fn note(&self, view: &EditorView) -> impl IntoElement {
        let words = if view.known { UNREAD } else { NOT_RUNNING };
        let model = self.model.clone();
        div()
            .id("processing-note")
            .role(Role::Note)
            .aria_label(words)
            .flex()
            .flex_col()
            .gap(px(10.0))
            .p(px(16.0))
            .rounded(px(radius::LG))
            .border_1()
            .border_color(color(colors::BORDER))
            .bg(color(colors::SURFACE))
            .child(
                div()
                    .text_size(px(text::BASE))
                    .text_color(color(colors::TEXT_SECONDARY))
                    .child(seen(words)),
            )
            .when(view.known, |note| {
                note.child(div().flex().child(
                    Button::new("processing-read", "Read again", ButtonKind::Secondary).on_press(
                        move |_, _| {
                            let read = model.read_again();
                            model.console().spawn(read);
                        },
                    ),
                ))
            })
    }
}

impl Screen for ProcessingScreen {
    const SECTION: Section = Section::Processing;

    fn new(hub: Entity<Hub>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let model = Model::new(hub.read(cx).console_for(Self::SECTION));
        // The model's own changes (an outcome, the desk's curve, a bypass) draw the screen again.
        let (wake, mut woken) = futures::channel::mpsc::unbounded::<()>();
        model.on_change(move || drop(wake.unbounded_send(())));
        let woken = cx.spawn(async move |this, cx| {
            while woken.next().await.is_some() {
                while woken.try_recv().is_ok() {}
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        });
        let channel = choice_state(Vec::new(), None, window, cx);
        let preset = choice_state(Vec::new(), None, window, cx);
        let save_name = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Preset name")
                .validate(|text, _| text.chars().count() <= MAX_PRESET_NAME)
        });
        let subscriptions = vec![
            cx.observe_in(&hub, window, |this, _, window, cx| {
                this.follow_route(window, cx);
                cx.notify();
            }),
            cx.subscribe_in(
                &channel,
                window,
                |this, _, event: &SelectEvent<Vec<Choice>>, _, cx| {
                    let SelectEvent::Confirm(Some(value)) = event else {
                        return;
                    };
                    if let Ok(channel) = value.parse::<usize>() {
                        this.opened(channel, cx);
                    }
                },
            ),
            cx.subscribe_in(
                &preset,
                window,
                |this, _, event: &SelectEvent<Vec<Choice>>, _, cx| {
                    let SelectEvent::Confirm(value) = event;
                    this.chosen_preset = value.as_ref().map(|value| value.to_string());
                    cx.notify();
                },
            ),
            cx.subscribe_in(
                &save_name,
                window,
                |this, _, event: &InputEvent, window, cx| {
                    if let InputEvent::PressEnter { .. } = event {
                        this.save_preset(window, cx);
                    }
                },
            ),
        ];
        let mut screen = Self {
            hub,
            heading: cx.focus_handle(),
            model,
            channel,
            preset,
            save_name,
            channel_choices: Vec::new(),
            preset_choices: Vec::new(),
            chosen_preset: None,
            copy_open: false,
            copy_picked: BTreeSet::new(),
            on_screen: false,
            graph: Rc::new(Cell::new(Bounds::default())),
            dragging: None,
            handles: (0..Target::ALL.len()).map(|_| cx.focus_handle()).collect(),
            _subscriptions: subscriptions,
            _woken: woken,
        };
        screen.follow_route(window, cx);
        screen
    }

    fn heading(&self) -> &FocusHandle {
        &self.heading
    }
}

impl Render for ProcessingScreen {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = self.model.view();
        let blocked = !view.known || view.unread;
        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .font_family(FONT_SANS)
            .child(
                // The header: the title, and its paragraph beside it on the baseline.
                div()
                    .flex()
                    .flex_wrap()
                    .items_baseline()
                    .gap_x(px(12.0))
                    .child(heading(
                        "processing-heading",
                        Self::SECTION.title(),
                        &self.heading,
                        false,
                    ))
                    .child(
                        div()
                            .text_size(px(text::MD))
                            .text_color(color(colors::TEXT_MUTED))
                            .child(LEAD),
                    ),
            )
            .when(blocked, |screen| screen.child(self.note(&view)))
            .child(atoms::message_line(
                "processing-outcome",
                false,
                &view.status_line(),
            ))
            .child(atoms::message_line(
                "processing-failure",
                true,
                &view.failure,
            ))
            .when(!blocked, |screen| {
                screen
                    .child(self.bar(&view, window, cx))
                    .child(parts::levels(&view, self.model.console()))
                    .when(view.bypassed, |screen| {
                        screen.child(
                            div()
                                .id("processing-bypassed")
                                .role(Role::Status)
                                .p(px(12.0))
                                .rounded(px(radius::MD))
                                .border_1()
                                .border_color(color(colors::WARN_BORDER))
                                .bg(color(colors::WARN_BG))
                                .text_color(color(colors::ACCENT))
                                .child(format!(
                                    "You hear {} with no processing. Its settings are kept below; \
                                     press Bypass (A/B) again to put them back. Leaving this \
                                     screen puts them back too.",
                                    view.label()
                                )),
                        )
                    })
                    .child(self.sections(&view, window, cx))
                    .child(self.sends(&view, cx))
            })
    }
}
