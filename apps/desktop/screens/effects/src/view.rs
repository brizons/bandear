//! The Effects screen's GPUI view (`effects-screen.js`): the shared reverb and delay side by
//! side, their sliders as the Processing editor's, the delay's tempo form and the "Repeats
//! every …" line. Every change goes through the model (the whole effect, the latest winning);
//! the view only draws [effects_view] and the model's outcome.

use bandear_desktop_console::router::{Route, Section};
use bandear_desktop_console::types::{DelaySettings, ReverbSettings};
use bandear_desktop_kit::a11y::{heading, seen};
use bandear_desktop_kit::buttons::{Button, ButtonKind};
use bandear_desktop_kit::forms::{
    Choice, ChoiceState, InputEvent, SelectEvent, choice_state, select, text_input, text_state,
};
use bandear_desktop_kit::gpui::{
    AnyElement, ClickEvent, Context, Div, Entity, FocusHandle, Focusable, InteractiveElement,
    IntoElement, ParentElement, Render, Role, SharedString, Stateful, StatefulInteractiveElement,
    Styled, Window, div, prelude::FluentBuilder, px,
};
use bandear_desktop_kit::gpui_component::input::InputState;
use bandear_desktop_kit::gpui_component::{Disableable, checkbox::Checkbox};
use bandear_desktop_kit::hub::{Hub, Screen};
use bandear_desktop_kit::slider::parameter;
use bandear_desktop_kit::theme::{
    FONT_MONO, FONT_SANS, color, colors, radius, space, text, weight,
};
use bandear_desktop_kit::tooltip::tooltip;

use crate::effects::{DELAY_NOTES, EffectsView, effects_view, format_tempo, note_of};
use crate::model::Model;

/// Why the Time slider waits while a tempo sets the time.
const TIME_SYNCED: &str = "The tempo sets the time; clear the tempo to set it here";

/// A slider of an effect: its range, its step, its words and where its value lives.
struct Field<S> {
    id: &'static str,
    label: &'static str,
    /// `aria-label`: "<Effect> <label lowercased>".
    aria: &'static str,
    min: f32,
    max: f32,
    step: f32,
    get: fn(&S) -> f32,
    set: fn(&mut S, f32),
}

impl<S> Field<S> {
    fn positions(&self) -> i32 {
        ((self.max - self.min) / self.step).round() as i32
    }

    fn position(&self, value: f32) -> f64 {
        f64::from((value - self.min) / self.step)
    }
}

const REVERB_FIELDS: [Field<ReverbSettings>; 4] = [
    Field {
        id: "reverb-size",
        label: "Size",
        aria: "Reverb size",
        min: 0.0,
        max: 100.0,
        step: 1.0,
        get: |reverb| reverb.size_percent,
        set: |reverb, value| reverb.size_percent = value,
    },
    Field {
        id: "reverb-damping",
        label: "Damping",
        aria: "Reverb damping",
        min: 0.0,
        max: 100.0,
        step: 1.0,
        get: |reverb| reverb.damping_percent,
        set: |reverb, value| reverb.damping_percent = value,
    },
    Field {
        id: "reverb-pre-delay",
        label: "Pre-delay",
        aria: "Reverb pre-delay",
        min: 0.0,
        max: 200.0,
        step: 1.0,
        get: |reverb| reverb.pre_delay_ms,
        set: |reverb, value| reverb.pre_delay_ms = value,
    },
    Field {
        id: "reverb-width",
        label: "Width",
        aria: "Reverb width",
        min: 0.0,
        max: 100.0,
        step: 1.0,
        get: |reverb| reverb.width_percent,
        set: |reverb, value| reverb.width_percent = value,
    },
];

const DELAY_FIELDS: [Field<DelaySettings>; 3] = [
    Field {
        id: "delay-time",
        label: "Time",
        aria: "Delay time",
        min: 1.0,
        max: 2000.0,
        step: 1.0,
        get: |delay| delay.time_ms,
        set: |delay, value| delay.time_ms = value,
    },
    Field {
        id: "delay-feedback",
        label: "Feedback",
        aria: "Delay feedback",
        min: 0.0,
        max: 90.0,
        step: 1.0,
        get: |delay| delay.feedback_percent,
        set: |delay, value| delay.feedback_percent = value,
    },
    Field {
        id: "delay-high-cut",
        label: "High cut",
        aria: "Delay high cut",
        min: 1000.0,
        max: 20_000.0,
        step: 100.0,
        get: |delay| delay.high_cut_hz,
        set: |delay, value| delay.high_cut_hz = value,
    },
];

/// The Effects screen.
pub struct EffectsScreen {
    hub: Entity<Hub>,
    heading: FocusHandle,
    model: Model,
    tempo: Entity<InputState>,
    note: Entity<ChoiceState>,
    /// The tempo field was typed in since its last submit: the view leaves it as typed.
    tempo_touched: bool,
}

impl EffectsScreen {
    /// The screen's model.
    pub fn model(&self) -> &Model {
        &self.model
    }

    /// The hub the screen draws from.
    pub fn hub(&self) -> &Entity<Hub> {
        &self.hub
    }

    /// Set tempo (the button, or Enter in the field): a tempo that is not one is said and nothing
    /// is sent.
    fn submit_tempo(&mut self, cx: &mut Context<Self>) {
        let text = self.tempo.read(cx).value().to_string();
        let note = self
            .note
            .read(cx)
            .selected_value()
            .map_or_else(|| self.view().delay.settings.note, |value| note_of(value));
        if let Some(flow) = self.model.set_tempo(&text, note) {
            self.tempo_touched = false;
            self.model.console().spawn(flow);
        }
        cx.notify();
    }

    fn view(&self) -> EffectsView {
        effects_view(&self.model.console().state())
    }
}

impl Screen for EffectsScreen {
    const SECTION: Section = Section::Effects;

    fn new(hub: Entity<Hub>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Draw again whenever the console changes.
        cx.observe(&hub, |_, _, cx| cx.notify()).detach();
        let model = Model::new(hub.read(cx).console_for(Self::SECTION));
        let tempo = text_state("bpm", window, cx);
        cx.subscribe(&tempo, |screen, _, event: &InputEvent, cx| match event {
            InputEvent::Change => screen.tempo_touched = true,
            InputEvent::PressEnter { .. } => screen.submit_tempo(cx),
            _ => {}
        })
        .detach();
        let choices = DELAY_NOTES
            .iter()
            .map(|choice| Choice::new(choice.value, choice.label))
            .collect();
        let shown = effects_view(&model.console().state());
        let note = choice_state(
            choices,
            Some(crate::effects::note_choice(shown.delay.settings.note).value),
            window,
            cx,
        );
        cx.subscribe(&note, |screen, _, event: &SelectEvent<Vec<Choice>>, cx| {
            let SelectEvent::Confirm(Some(value)) = event else {
                return;
            };
            let note = note_of(value);
            screen
                .model
                .console()
                .spawn(screen.model.edit_delay(move |delay| delay.note = note));
            cx.notify();
        })
        .detach();
        Self {
            hub,
            heading: cx.focus_handle(),
            model,
            tempo,
            note,
            tempo_touched: false,
        }
    }

    fn heading(&self) -> &FocusHandle {
        &self.heading
    }
}

/// A card of the panel (`.processing-card`).
fn card(id: &'static str, label: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .role(Role::Group)
        .aria_label(label)
        .flex()
        .flex_col()
        .gap(px(10.0))
        .flex_1()
        .min_w(px(320.0))
        .py(px(14.0))
        .px(px(space::S4))
        .rounded(px(radius::XL))
        .border_1()
        .border_color(color(colors::BORDER))
        .bg(color(colors::SURFACE))
}

/// A card's title (`h2`).
fn card_title(title: &'static str) -> impl IntoElement {
    div()
        .id(title)
        .role(Role::Heading)
        .aria_level(2)
        .aria_label(title)
        .text_size(px(text::LG))
        .font_weight(weight::BOLD)
        .child(seen(title))
}

/// One slider row: its label, the slider and its value in words.
fn slider_row(
    id: &'static str,
    label: &'static str,
    slider: impl IntoElement,
    output: SharedString,
    title: Option<&'static str>,
) -> AnyElement {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(10.0))
        .when_some(title, |row, title| {
            row.tooltip(move |window, cx| tooltip(title.into(), window, cx))
        })
        .child(
            div()
                .w(px(82.0))
                .flex_none()
                .text_size(px(text::SM))
                .text_color(color(colors::TEXT_SECONDARY))
                .child(seen(label)),
        )
        .child(div().flex_1().min_w_0().child(slider))
        .child(
            div()
                .w(px(72.0))
                .flex_none()
                .flex()
                .justify_end()
                .font_family(FONT_MONO)
                .text_size(px(text::SM))
                .whitespace_nowrap()
                .child(seen(output)),
        )
        .into_any_element()
}

impl Render for EffectsScreen {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = self.view();
        let running = view.running;
        let outcome = self.model.outcome();

        // The tempo field shows the tempo only while it sets the time, never over typing.
        let typing = self.tempo.read(cx).focus_handle(cx).is_focused(window);
        if !self.tempo_touched && !typing {
            let shown = if view.delay.synced {
                format_tempo(view.delay.settings.tempo_bpm)
            } else {
                String::new()
            };
            if self.tempo.read(cx).value().as_ref() != shown {
                self.tempo
                    .update(cx, |state, cx| state.set_value(shown, window, cx));
            }
        }
        let note_value = crate::effects::note_choice(view.delay.settings.note).value;
        let note_open = self.note.read(cx).focus_handle(cx).is_focused(window);
        if !note_open
            && self
                .note
                .read(cx)
                .selected_value()
                .map(|value| value.as_ref())
                != Some(note_value)
        {
            self.note.update(cx, |state, cx| {
                state.set_selected_value(&SharedString::from(note_value), window, cx)
            });
        }

        let reverb = view.reverb.settings;
        let reverb_words = [
            view.reverb.size.clone(),
            view.reverb.damping.clone(),
            view.reverb.pre_delay.clone(),
            view.reverb.width.clone(),
        ];
        let reverb_rows = REVERB_FIELDS
            .iter()
            .zip(reverb_words)
            .map(|(field, words)| {
                let model = self.model.clone();
                let set = field.set;
                let (min, step) = (field.min, field.step);
                let slider = parameter(
                    field.id,
                    field.positions(),
                    field.position((field.get)(&reverb)),
                    field.position((field.get)(&ReverbSettings::default())) as i32,
                    field.aria,
                    words.clone(),
                )
                .disabled(!running)
                .on_input(move |position, _, _| {
                    let value = min + position as f32 * step;
                    model
                        .console()
                        .spawn(model.edit_reverb(move |reverb| set(reverb, value)));
                });
                slider_row(field.id, field.label, slider, words.into(), None)
            });

        let delay = view.delay.settings;
        let delay_words = [
            view.delay.time.clone(),
            view.delay.feedback.clone(),
            view.delay.high_cut.clone(),
        ];
        let synced = view.delay.synced;
        let delay_rows = DELAY_FIELDS.iter().zip(delay_words).map(|(field, words)| {
            let model = self.model.clone();
            let set = field.set;
            let (min, step) = (field.min, field.step);
            let is_time = field.id == "delay-time";
            let slider = parameter(
                field.id,
                field.positions(),
                field.position((field.get)(&delay)),
                field.position((field.get)(&DelaySettings::default())) as i32,
                field.aria,
                words.clone(),
            )
            .disabled(!running || (is_time && synced))
            .on_input(move |position, _, _| {
                let value = min + position as f32 * step;
                model
                    .console()
                    .spawn(model.edit_delay(move |delay| set(delay, value)));
            });
            slider_row(
                field.id,
                field.label,
                slider,
                words.into(),
                (is_time && synced).then_some(TIME_SYNCED),
            )
        });

        let ping_pong = {
            let model = self.model.clone();
            Checkbox::new("delay-ping-pong")
                .label("Ping-pong")
                .accessibility_label("Delay ping-pong")
                .checked(delay.ping_pong)
                .disabled(!running)
                .on_click(move |checked, _, _| {
                    let on = *checked;
                    model
                        .console()
                        .spawn(model.edit_delay(move |delay| delay.ping_pong = on));
                })
        };

        let hub = self.hub.clone();
        let processing_link = div()
            .id("effects-processing-link")
            .role(Role::Link)
            .aria_label("Processing")
            .focusable()
            .tab_stop(true)
            .text_color(color(colors::ACCENT))
            .underline()
            .cursor_pointer()
            .focus_visible(|style| style.border_1().border_color(color(colors::FOCUS)))
            .on_click(move |_: &ClickEvent, _, cx| {
                hub.update(cx, |hub, cx| {
                    hub.navigate(Route::from(Section::Processing), cx)
                });
            })
            .child(seen("Processing"));

        let disabled_look = |card: Stateful<Div>| {
            if running { card } else { card.opacity(0.7) }
        };

        div()
            .flex()
            .flex_col()
            .gap(px(space::S3))
            .font_family(FONT_SANS)
            // No visible heading, as the web screen had none: a hidden one takes the focus a
            // navigation moves here and names the screen.
            .child(heading(
                "effects-heading",
                Self::SECTION.title(),
                &self.heading,
                true,
            ))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .text_size(px(text::SM))
                    .text_color(color(colors::TEXT_MUTED))
                    .child(seen(
                        "One reverb and one delay, shared by every mix. Send inputs to them from \
                         the\u{a0}",
                    ))
                    .child(processing_link)
                    .child(seen(
                        "\u{a0}editor; each mix returns them on its master strip. They add no \
                         latency to the dry mix.",
                    )),
            )
            .when(!running, |screen| {
                screen.child(
                    div()
                        .text_size(px(text::SM))
                        .text_color(color(colors::TEXT_MUTED))
                        .child(
                            "The engine is not running. Start it from the Mix screen to change \
                             the effects.",
                        ),
                )
            })
            .child(
                div()
                    .id("effects-outcome")
                    .role(Role::Status)
                    .aria_label(
                        outcome
                            .as_ref()
                            .map(|said| SharedString::from(said.message.clone()))
                            .unwrap_or_default(),
                    )
                    .min_h(px(text::SM * text::LINE_HEIGHT))
                    .text_size(px(text::BASE))
                    .text_color(color(if outcome.as_ref().is_some_and(|said| !said.ok) {
                        colors::RED
                    } else {
                        colors::TEXT
                    }))
                    .child(seen(outcome.map(|said| said.message).unwrap_or_default())),
            )
            .child(
                div()
                    .id("effects-cards")
                    .role(Role::Group)
                    .aria_label("Effects")
                    .flex()
                    .flex_wrap()
                    .gap(px(space::S3))
                    .child(disabled_look(
                        card("effects-reverb", "Reverb")
                            .child(card_title("Reverb"))
                            .children(reverb_rows),
                    ))
                    .child(disabled_look(
                        card("effects-delay", "Delay")
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .items_center()
                                    .gap(px(space::S2))
                                    .child(card_title("Delay"))
                                    .child(div().flex_1())
                                    .child(ping_pong),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .items_center()
                                    .gap(px(space::S2))
                                    .child(seen("Tempo"))
                                    .child(
                                        div().w(px(84.0)).child(
                                            text_input(&self.tempo, "Tempo").disabled(!running),
                                        ),
                                    )
                                    .child(seen("Note"))
                                    .child(
                                        div()
                                            .w(px(130.0))
                                            .child(select(&self.note, "Note").disabled(!running)),
                                    )
                                    .child(
                                        Button::new(
                                            "effects-set-tempo",
                                            "Set tempo",
                                            ButtonKind::Secondary,
                                        )
                                        .disabled(!running)
                                        .on_press({
                                            let screen = cx.entity().downgrade();
                                            move |_, cx| {
                                                drop(screen.update(cx, |screen, cx| {
                                                    screen.submit_tempo(cx)
                                                }));
                                            }
                                        }),
                                    ),
                            )
                            .children(delay_rows)
                            .child(
                                div()
                                    .text_size(px(text::SM))
                                    .text_color(color(colors::TEXT_MUTED))
                                    .child(format!("Repeats every {}.", view.delay.time_in_force)),
                            ),
                    )),
            )
    }
}
