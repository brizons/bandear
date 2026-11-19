//! The Network and system screen's GPUI view (`network-screen.js` and its components): the header,
//! the note while no engine runs, the refused command and the audio word, then the musicians table
//! with the link panel below it, beside the audio input and the Session panel (below them in a
//! narrow window). The view draws the [Model]'s [ScreenView] and hands every control's action back
//! to it; the fields it keeps (text being typed, the selects' lists, the chart's readout) are its
//! own.

mod audio;
mod cells;
mod chart;
mod health;
mod parts;
mod session;
mod table;

use bandear_desktop_console::router::{Route, Section};
use bandear_desktop_kit::a11y::{heading, seen};
use bandear_desktop_kit::forms::{ChoiceState, choice_state};
use bandear_desktop_kit::gpui::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement, IntoElement, ParentElement,
    Render, Role, StatefulInteractiveElement, Styled, Subscription, Task, Window, div,
    prelude::FluentBuilder, px,
};
use bandear_desktop_kit::gpui_component::input::InputState;
use bandear_desktop_kit::hub::{Hub, Screen};
use bandear_desktop_kit::theme::{FONT_SANS, color, colors, space, text};
use futures::StreamExt;

use crate::model::sessions::MAX_SESSION_NAME_LENGTH;
use crate::model::{Cleared, Model, ScreenView};
use parts::{Shown, Tone, line};

/// The screen's lead paragraph.
const LEAD: &str =
    "Each phone's link and name, the audio input the mixes come from, and the saved sessions.";

/// The audio card's selects and what each shows.
struct AudioFields {
    host: Entity<ChoiceState>,
    device: Entity<ChoiceState>,
    buffer: Entity<ChoiceState>,
    output: Entity<ChoiceState>,
    shown: [Shown; 4],
    apply: FocusHandle,
}

/// The Session panel's fields and the places the focus goes.
struct SessionFields {
    save_name: Entity<InputState>,
    rename: Entity<InputState>,
    saved: Entity<ChoiceState>,
    saved_shown: Shown,
    load: FocusHandle,
    cancel_load: FocusHandle,
    keep_delete: FocusHandle,
    cancel_restore: FocusHandle,
}

/// The Network and system screen.
pub struct NetworkScreen {
    hub: Entity<Hub>,
    heading: FocusHandle,
    model: Model,
    /// What the model shows, worked out as the console or the model changed.
    view: ScreenView,
    /// True while the section is the one shown.
    shown: bool,
    rows: Vec<table::RowFields>,
    address: Entity<InputState>,
    keep_remove: FocusHandle,
    audio: AudioFields,
    session: SessionFields,
    cleared: Cleared,
    chart: chart::ChartState,
    _subscriptions: Vec<Subscription>,
    _woken: Task<()>,
}

impl NetworkScreen {
    /// The screen's model.
    pub fn model(&self) -> &Model {
        &self.model
    }

    /// The hub the screen draws from.
    pub fn hub(&self) -> &Entity<Hub> {
        &self.hub
    }

    /// Works the view out again and brings the fields in line with it: a field being typed in is
    /// never overwritten, and a select is rebuilt only when its options change.
    fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let section = self.hub.read(cx).route().section;
        let shown = section == Section::Network;
        if shown != self.shown {
            self.shown = shown;
            if shown {
                self.model.mount();
            } else {
                self.model.unmount();
            }
        }
        self.view = self.model.screen();
        self.sync_rows(window, cx);
        self.sync_audio(window, cx);
        self.sync_session(window, cx);
        let cleared = self.view.cleared;
        if cleared.address != self.cleared.address {
            self.address
                .update(cx, |state, cx| state.set_value("", window, cx));
        }
        if cleared.save_name != self.cleared.save_name {
            self.session
                .save_name
                .update(cx, |state, cx| state.set_value("", window, cx));
        }
        if cleared.rename != self.cleared.rename {
            self.session
                .rename
                .update(cx, |state, cx| state.set_value("", window, cx));
        }
        self.cleared = cleared;
        cx.notify();
    }
}

impl Screen for NetworkScreen {
    const SECTION: Section = Section::Network;

    fn new(hub: Entity<Hub>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let model = Model::new(hub.read(cx).console_for(Self::SECTION));
        // The model's own changes (an outcome, a choice settled, a timer) draw the screen again;
        // they come from inside the console's flows, where no GPUI context is at hand.
        let (wake, mut woken) = futures::channel::mpsc::unbounded::<()>();
        model.on_change(move || drop(wake.unbounded_send(())));
        let woken = cx.spawn_in(window, async move |this, cx| {
            while woken.next().await.is_some() {
                while woken.try_recv().is_ok() {}
                if this
                    .update_in(cx, |this, window, cx| this.refresh(window, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        let address = cx.new(|cx| InputState::new(window, cx).placeholder("192.168.1.50:50000"));
        let save_name = cx.new(|cx| {
            InputState::new(window, cx)
                .validate(|text, _| text.chars().count() <= MAX_SESSION_NAME_LENGTH)
        });
        let rename = cx.new(|cx| {
            InputState::new(window, cx)
                .validate(|text, _| text.chars().count() <= MAX_SESSION_NAME_LENGTH)
        });
        let audio = AudioFields {
            host: choice_state(Vec::new(), None, window, cx),
            device: choice_state(Vec::new(), None, window, cx),
            buffer: choice_state(Vec::new(), None, window, cx),
            output: choice_state(Vec::new(), None, window, cx),
            shown: Default::default(),
            apply: cx.focus_handle(),
        };
        let session = SessionFields {
            save_name,
            rename,
            saved: choice_state(Vec::new(), None, window, cx),
            saved_shown: Shown::default(),
            load: cx.focus_handle(),
            cancel_load: cx.focus_handle(),
            keep_delete: cx.focus_handle(),
            cancel_restore: cx.focus_handle(),
        };
        let mut subscriptions = vec![cx.observe_in(&hub, window, |this, _, window, cx| {
            this.refresh(window, cx);
        })];
        subscriptions.extend(Self::subscribe_fields(
            &address, &audio, &session, window, cx,
        ));
        let chart = chart::ChartState::new(cx);
        subscriptions.push(cx.on_focus(&chart.focus, window, |this, _, cx| this.chart_focused(cx)));
        subscriptions.push(cx.on_blur(&chart.focus, window, |this, _, cx| this.chart_blurred(cx)));
        let mut screen = Self {
            view: model.screen(),
            hub,
            heading: cx.focus_handle(),
            model,
            shown: false,
            rows: Vec::new(),
            address,
            keep_remove: cx.focus_handle(),
            audio,
            session,
            cleared: Cleared::default(),
            chart,
            _subscriptions: subscriptions,
            _woken: woken,
        };
        screen.refresh(window, cx);
        screen
    }

    fn heading(&self) -> &FocusHandle {
        &self.heading
    }
}

impl NetworkScreen {
    fn subscribe_fields(
        address: &Entity<InputState>,
        audio: &AudioFields,
        session: &SessionFields,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Subscription> {
        let mut subscriptions = table::subscribe_address(address, window, cx);
        subscriptions.extend(audio::subscribe(audio, window, cx));
        subscriptions.extend(session::subscribe(session, window, cx));
        subscriptions
    }

    /// The note while no engine runs, with its way to the Mix screen.
    fn note(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let hub = self.hub.clone();
        div()
            .id("note")
            .role(Role::Paragraph)
            .flex()
            .flex_wrap()
            .gap(px(4.0))
            .font_family(FONT_SANS)
            .text_size(px(text::BASE))
            .text_color(color(colors::TEXT_SECONDARY))
            .child(seen(
                "The engine is not running, so no phone is listed. Start it from the",
            ))
            .child(
                div()
                    .id("note-mix")
                    .role(Role::Link)
                    .aria_label("Mix")
                    .focusable()
                    .cursor_pointer()
                    .underline()
                    .text_color(color(colors::BLUE))
                    .focus_visible(|style| style.border_b_2().border_color(color(colors::FOCUS)))
                    .on_click(cx.listener(move |_, _, _, cx| {
                        hub.update(cx, |hub, cx| hub.navigate(Route::from(Section::Mix), cx));
                    }))
                    .child(seen("Mix")),
            )
            .child(seen("screen, or choose a driver below."))
    }
}

impl Render for NetworkScreen {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = &self.view;
        let header = div()
            .flex()
            .flex_wrap()
            .items_baseline()
            .gap(px(space::S3))
            .child(heading(
                "network-heading",
                Self::SECTION.title(),
                &self.heading,
                false,
            ))
            .child(
                div()
                    .id("lead")
                    .role(Role::Paragraph)
                    .font_family(FONT_SANS)
                    .text_size(px(text::BASE))
                    .text_color(color(colors::TEXT_SECONDARY))
                    .child(seen(LEAD)),
            );
        let musicians = div()
            .id("musicians")
            .role(Role::Region)
            .aria_label("Musicians")
            .flex()
            .flex_col()
            .gap(px(space::S4))
            .min_w_0()
            .flex_basis(px(560.0))
            .flex_grow(999.0)
            .flex_shrink(1.0)
            .child(self.render_table(cx))
            .when_some(view.health.clone(), |section, panel| {
                section.child(self.render_health(&panel, window, cx))
            });
        let side = div()
            .id("side")
            .role(Role::Complementary)
            .aria_label("Audio input and session")
            .flex()
            .flex_col()
            .gap(px(space::S4))
            .min_w_0()
            .flex_basis(px(300.0))
            .flex_grow(1.0)
            .flex_shrink(1.0)
            .child(self.render_audio(cx))
            .child(self.render_session(cx));
        div()
            .flex()
            .flex_col()
            .gap(px(space::S4))
            .pt(px(space::S1))
            .child(header)
            .when(!view.engine_running, |screen| screen.child(self.note(cx)))
            .when_some(view.command.clone(), |screen, command| {
                screen.child(line("command", Tone::Error, command))
            })
            .when_some(view.audio_message.clone(), |screen, notice| {
                screen.child(line("audio-notice", Tone::Status, notice))
            })
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_start()
                    .gap(px(space::S4))
                    .child(musicians)
                    .child(side),
            )
    }
}
