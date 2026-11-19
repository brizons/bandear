//! The Record screen's GPUI view (`record-screen.js`): the transport, one track per recorded
//! input with its arm button, meter and waveform lane, the takes in the recordings folder, the
//! virtual soundcheck card and the recording settings. It draws what [crate::record_view] and
//! [crate::soundcheck_view] work out and turns the controls into the model's flows; each outcome
//! is said in words above the transport.
//!
//! The view keeps no state of its own but its text fields and selects: the model keeps the
//! transport's guard, the lanes and the folder field's rule. Every press notifies the view, and
//! every flow ends on a change of the console's state, which draws it again.

mod atoms;
mod side;
mod takes;
mod tracks;
mod transport;

use bandear_desktop_console::router::{Route, Section};
use bandear_desktop_console::state::ConsoleState;
use bandear_desktop_kit::a11y::{heading, seen};
use bandear_desktop_kit::forms::{Choice, ChoiceState, InputEvent, SelectEvent, choice_state};
use bandear_desktop_kit::gpui::{
    App, AppContext as _, Context, Entity, FocusHandle, IntoElement, ParentElement, Render, Styled,
    Window, div, prelude::FluentBuilder, px,
};
use bandear_desktop_kit::gpui_component::input::InputState;
use bandear_desktop_kit::hub::{Hub, Screen};
use bandear_desktop_kit::theme::{FONT_SANS, color, colors, space, text};

use crate::model::{Flow, Model};
use crate::record_view::record_messages;
use crate::recording::SettingsChange;

/// The longest take name the field takes.
const TAKE_NAME_LENGTH: usize = 60;

/// The Record screen.
pub struct RecordScreen {
    hub: Entity<Hub>,
    heading: FocusHandle,
    model: Model,
    take_name: Entity<InputState>,
    folder: Entity<InputState>,
    format: Entity<ChoiceState>,
    /// The soundcheck card's take select.
    takes: Entity<ChoiceState>,
    /// The folders the take select offers now.
    takes_offered: Vec<String>,
    /// The screen is the one the window shows.
    shown: bool,
}

impl RecordScreen {
    /// The screen's model.
    pub fn model(&self) -> &Model {
        &self.model
    }

    /// The hub the screen draws from.
    pub fn hub(&self) -> &Entity<Hub> {
        &self.hub
    }

    /// What a press does: runs `run`'s flow on the console and draws again (the guard may have
    /// changed the buttons).
    fn press<T: 'static>(
        &self,
        cx: &Context<Self>,
        run: impl Fn(&Model) -> Flow<T> + 'static,
    ) -> impl Fn(&mut Window, &mut App) + 'static {
        let model = self.model.clone();
        let screen = cx.entity().downgrade();
        move |_, cx| {
            model.spawn(run(&model));
            drop(screen.update(cx, |_, cx| cx.notify()));
        }
    }

    /// What the web screen did each time it was mounted (`rs.js`): read the settings when they
    /// are unknown, list the takes and read the disk estimate. The screen lives as long as the
    /// window, so it does this each time the window turns to it.
    fn on_shown(model: &Model) {
        if model.console().state().recording_settings.is_none() {
            let load = model.console().load_recording_settings();
            model.spawn(load);
        }
        model.spawn(model.refresh_takes());
        let estimate = model.console().refresh_estimate();
        model.spawn(estimate);
    }

    /// Opens `section`, as the in-text links do.
    fn open(&self, section: Section) -> impl Fn(&mut Window, &mut App) + 'static {
        let hub = self.hub.clone();
        move |_, cx| hub.update(cx, |hub, cx| hub.navigate(Route::from(section), cx))
    }
}

impl Screen for RecordScreen {
    const SECTION: Section = Section::Record;

    fn new(hub: Entity<Hub>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Draw again whenever the console changes, the meters included (the track meters and
        // lanes follow them); and read what the screen shows when the window turns to it.
        cx.observe(&hub, |screen: &mut Self, hub, cx| {
            let shown = hub.read(cx).route().section == Self::SECTION;
            if shown && !screen.shown {
                Self::on_shown(&screen.model);
            }
            screen.shown = shown;
            cx.notify();
        })
        .detach();
        let model = Model::new(hub.read(cx).console_for(Self::SECTION));

        let take_name = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Take")
                .validate(|text, _| text.chars().count() <= TAKE_NAME_LENGTH)
        });
        let folder = cx.new(|cx| InputState::new(window, cx));
        // Enter in the folder field asks for the folder, as the form's submit did.
        cx.subscribe(&folder, |screen, field, event: &InputEvent, cx| {
            if let InputEvent::PressEnter { .. } = event {
                let text = field.read(cx).value().to_string();
                screen.model.spawn(screen.model.use_folder(text));
                cx.notify();
            }
        })
        .detach();

        let formats = crate::record_view::settings_view(&model.console().state())
            .formats
            .into_iter()
            .map(|(value, label)| Choice::new(value, label))
            .collect();
        let format = choice_state(formats, None, window, cx);
        cx.subscribe(
            &format,
            |screen, _, event: &SelectEvent<Vec<Choice>>, cx| {
                if let SelectEvent::Confirm(Some(value)) = event {
                    let change = SettingsChange {
                        format: Some(value.to_string()),
                        ..SettingsChange::default()
                    };
                    screen.model.spawn(screen.model.change_settings(change));
                    cx.notify();
                }
            },
        )
        .detach();

        let takes = choice_state(Vec::new(), None, window, cx);
        cx.subscribe(&takes, |screen, _, event: &SelectEvent<Vec<Choice>>, cx| {
            let SelectEvent::Confirm(value) = event;
            screen
                .model
                .choose_take(value.as_ref().map(|value| value.to_string()));
            cx.notify();
        })
        .detach();

        let shown = hub.read(cx).route().section == Self::SECTION;
        if shown {
            Self::on_shown(&model);
        }

        Self {
            hub,
            heading: cx.focus_handle(),
            model,
            take_name,
            folder,
            format,
            takes,
            takes_offered: Vec::new(),
            shown,
        }
    }

    fn heading(&self) -> &FocusHandle {
        &self.heading
    }
}

impl RecordScreen {
    /// The screen's messages above the transport.
    fn messages(&self, state: &ConsoleState) -> impl IntoElement {
        let messages = record_messages(state);
        div()
            .flex()
            .flex_col()
            .gap(px(space::S2))
            .when_some(messages.outcome, |lines, outcome| {
                lines.child(atoms::message(
                    if outcome.ok {
                        "record-outcome"
                    } else {
                        "record-outcome-error"
                    },
                    outcome.text,
                    !outcome.ok,
                ))
            })
            .when_some(messages.failure, |lines, failure| {
                lines.child(atoms::message("record-failure", failure, true))
            })
    }
}

impl Render for RecordScreen {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let console = self.model.console().clone();
        let state = console.state();
        let locked = record_messages(&state).locked;

        div()
            .flex()
            .flex_col()
            .gap(px(space::S4))
            .pt(px(space::S1))
            .font_family(FONT_SANS)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_baseline()
                    .gap(px(space::S3))
                    .child(heading(
                        "record-heading",
                        Self::SECTION.title(),
                        &self.heading,
                        false,
                    ))
                    .child(
                        div()
                            .text_size(px(text::BASE))
                            .text_color(color(colors::TEXT_SECONDARY))
                            .child(seen(
                                "Every input on its own track, written straight from the engine \
                                 while the show runs.",
                            )),
                    ),
            )
            .child(self.messages(&state))
            .child(self.transport(&state, window, cx))
            .when_some(locked, |screen, locked| screen.child(atoms::hint(locked)))
            .child(self.tracks(&state, cx))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_start()
                    .gap(px(space::S4))
                    .child(self.takes(&state, cx))
                    .child(self.side(&state, window, cx)),
            )
    }
}
