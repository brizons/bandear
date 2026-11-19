//! The Channels and groups screen's GPUI view (`channels-screen.js`): the header, the note while
//! no engine runs, the outcome and failure lines, then the channel table beside the groups (below
//! it when narrow). The view draws the [Model]'s [ScreenView] and hands every control's edit back
//! to it; the fields it keeps (names being typed, a group's editor open, a delete armed) are its
//! own.

mod atoms;
mod groups;
mod table;

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use bandear_desktop_console::router::{Route, Section};
use bandear_desktop_kit::a11y::{heading, seen};
use bandear_desktop_kit::forms::{Choice, ChoiceState, choice_state};
use bandear_desktop_kit::gpui::{
    AppContext as _, Context, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Render, Role, StatefulInteractiveElement, Styled, Subscription, Task, Window,
    div, prelude::FluentBuilder, px,
};
use bandear_desktop_kit::gpui_component::input::{InputEvent, InputState};
use bandear_desktop_kit::gpui_component::select::SelectEvent;
use bandear_desktop_kit::hub::{Hub, Screen};
use bandear_desktop_kit::theme::{FONT_SANS, color, colors, text};
use futures::StreamExt;

use crate::channels::MAX_CHANNEL_NAME_LENGTH;
use crate::model::{Edit, Model, ScreenView};
use crate::table::kind_options;

/// The screen's lead paragraph.
const LEAD: &str = "Names, instruments and groups reach every phone at once. A hidden input is \
                    shown nowhere and no one hears it.";

/// The name field and instrument picker of one input's row.
struct RowFields {
    name: Entity<InputState>,
    kind: Entity<ChoiceState>,
    /// The name and kind the desk has, as last drawn.
    saved_name: String,
    saved_kind: String,
    _subscriptions: Vec<Subscription>,
}

/// One group card's own fields.
struct CardFields {
    name: Entity<InputState>,
    saved_name: String,
    edit: FocusHandle,
    done: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

/// A group's delete asked for once, waiting for its confirmation.
struct Armed {
    id: String,
    at: Instant,
    generation: u64,
}

/// The Channels and groups screen.
pub struct ChannelsScreen {
    hub: Entity<Hub>,
    heading: FocusHandle,
    model: Model,
    rows: Vec<RowFields>,
    cards: BTreeMap<String, CardFields>,
    /// The cards whose editor is open, by group id: it survives redraws and other cards coming
    /// and going.
    open: BTreeSet<String>,
    armed: Option<Armed>,
    armings: u64,
    keep: FocusHandle,
    confirm: FocusHandle,
    _subscriptions: Vec<Subscription>,
    _woken: Task<()>,
}

impl ChannelsScreen {
    /// The screen's model.
    pub fn model(&self) -> &Model {
        &self.model
    }

    /// The hub the screen draws from.
    pub fn hub(&self) -> &Entity<Hub> {
        &self.hub
    }

    /// Runs one of the screen's edits; its outcome reaches the view through the model.
    fn edit(&self, edit: Edit) {
        let flow = self.model.handle(edit);
        self.model.console().spawn(flow);
    }

    /// Brings the fields in line with what the model shows: a field the engineer is typing in is
    /// never overwritten.
    fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = self.model.view();
        for row in &view.rows {
            if self.rows.len() <= row.index {
                let fields = self.row_fields(row.index, window, cx);
                self.rows.push(fields);
            }
            let fields = &mut self.rows[row.index];
            let typing = fields.name.read(cx).focus_handle(cx).is_focused(window);
            if !typing && fields.name.read(cx).value().as_ref() != row.name {
                let name = row.name.clone();
                fields
                    .name
                    .update(cx, |state, cx| state.set_value(name, window, cx));
            }
            fields.saved_name = row.name.clone();
            if fields.saved_kind != row.kind {
                fields.saved_kind = row.kind.clone();
                let kind = bandear_desktop_kit::gpui::SharedString::from(row.kind.clone());
                fields
                    .kind
                    .update(cx, |state, cx| state.set_selected_value(&kind, window, cx));
            }
        }
        let ids: BTreeSet<String> = view.cards.iter().map(|card| card.id.clone()).collect();
        self.cards.retain(|id, _| ids.contains(id));
        self.open.retain(|id| ids.contains(id));
        if self
            .armed
            .as_ref()
            .is_some_and(|armed| !ids.contains(&armed.id))
        {
            self.armed = None;
        }
        for card in &view.cards {
            if !self.cards.contains_key(&card.id) {
                let fields = self.card_fields(&card.id, window, cx);
                self.cards.insert(card.id.clone(), fields);
            }
            let Some(fields) = self.cards.get_mut(&card.id) else {
                continue;
            };
            let typing = fields.name.read(cx).focus_handle(cx).is_focused(window);
            if !typing && fields.name.read(cx).value().as_ref() != card.name {
                let name = card.name.clone();
                fields
                    .name
                    .update(cx, |state, cx| state.set_value(name, window, cx));
            }
            fields.saved_name = card.name.clone();
        }
    }

    fn row_fields(&self, index: usize, window: &mut Window, cx: &mut Context<Self>) -> RowFields {
        let number = index + 1;
        let name = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(format!("CH {number}"))
                .validate(|text, _| text.chars().count() <= MAX_CHANNEL_NAME_LENGTH)
        });
        let choices: Vec<Choice> = kind_options()
            .iter()
            .map(|option| Choice::new(option.value, option.label))
            .collect();
        let kind = choice_state(choices, Some("other"), window, cx);
        let subscriptions = vec![
            cx.subscribe_in(
                &name,
                window,
                move |this, _, event: &InputEvent, window, cx| {
                    match event {
                        // Enter commits by leaving the field, as the web console's did.
                        InputEvent::PressEnter { .. } => window.blur(cx),
                        InputEvent::Blur => this.commit_name(index, cx),
                        _ => {}
                    }
                },
            ),
            cx.subscribe_in(
                &kind,
                window,
                move |this, _, event: &SelectEvent<Vec<Choice>>, _, _| {
                    let SelectEvent::Confirm(Some(kind)) = event else {
                        return;
                    };
                    let changed = this
                        .rows
                        .get(index)
                        .is_some_and(|fields| fields.saved_kind != kind.as_ref());
                    if changed {
                        this.edit(Edit::Kind {
                            index,
                            kind: kind.to_string(),
                        });
                    }
                },
            ),
        ];
        RowFields {
            name,
            kind,
            saved_name: String::new(),
            saved_kind: "other".to_owned(),
            _subscriptions: subscriptions,
        }
    }

    fn card_fields(&self, id: &str, window: &mut Window, cx: &mut Context<Self>) -> CardFields {
        let name = cx.new(|cx| InputState::new(window, cx));
        let card = id.to_owned();
        let subscriptions = vec![cx.subscribe_in(
            &name,
            window,
            move |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } => window.blur(cx),
                InputEvent::Blur => this.commit_group_name(&card, cx),
                _ => {}
            },
        )];
        CardFields {
            name,
            saved_name: String::new(),
            edit: cx.focus_handle(),
            done: cx.focus_handle(),
            _subscriptions: subscriptions,
        }
    }

    /// Sends the name typed for input `index` when it differs from the desk's, trimmed.
    fn commit_name(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(fields) = self.rows.get(index) else {
            return;
        };
        let typed = fields.name.read(cx).value().trim().to_owned();
        if typed != fields.saved_name.trim() {
            self.edit(Edit::Rename { index, name: typed });
        }
    }

    /// Puts the desk's name back in input `index`'s field and leaves it.
    fn revert_name(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(fields) = self.rows.get(index) {
            let saved = fields.saved_name.clone();
            fields
                .name
                .update(cx, |state, cx| state.set_value(saved, window, cx));
        }
        window.blur(cx);
    }

    fn commit_group_name(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(fields) = self.cards.get(id) else {
            return;
        };
        let typed = fields.name.read(cx).value().trim().to_owned();
        if typed != fields.saved_name.trim() {
            self.edit(Edit::GroupRename {
                id: id.to_owned(),
                name: typed,
            });
        }
    }

    fn revert_group_name(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(fields) = self.cards.get(id) {
            let saved = fields.saved_name.clone();
            fields
                .name
                .update(cx, |state, cx| state.set_value(saved, window, cx));
        }
        window.blur(cx);
    }

    /// The note while no engine runs; "Mix" goes to the Mix screen.
    fn note(&self) -> impl IntoElement {
        let hub = self.hub.clone();
        div()
            .id("channels-note")
            .role(Role::Note)
            .aria_label(
                "The engine is not running, so this desk does not know its inputs and groups \
                 yet. Start it from the Mix screen.",
            )
            .flex()
            .flex_wrap()
            .gap(px(4.0))
            .p(px(16.0))
            .rounded(px(12.0))
            .border_1()
            .border_color(color(colors::BORDER))
            .bg(color(colors::SURFACE))
            .text_color(color(colors::TEXT_SECONDARY))
            .child(seen(
                "The engine is not running, so this desk does not know its inputs and groups \
                 yet. Start it from the",
            ))
            .child(atoms::link("channels-note-mix", "Mix", "Mix", None, {
                let hub = hub.clone();
                move |_, cx| {
                    hub.update(cx, |hub, cx| hub.navigate(Route::from(Section::Mix), cx));
                }
            }))
            .child(seen("screen."))
    }

    fn view_model(&self) -> ScreenView {
        self.model.view()
    }
}

impl Screen for ChannelsScreen {
    const SECTION: Section = Section::Channels;

    fn new(hub: Entity<Hub>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let model = Model::new(hub.read(cx).console_for(Self::SECTION));
        // The model's own changes (a box held, an outcome) draw the screen again.
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
        // Draw again whenever the console changes, the fields in line first.
        let subscriptions = vec![cx.observe_in(&hub, window, |this, _, window, cx| {
            this.sync(window, cx);
            cx.notify();
        })];
        let mut screen = Self {
            hub,
            heading: cx.focus_handle(),
            model,
            rows: Vec::new(),
            cards: BTreeMap::new(),
            open: BTreeSet::new(),
            armed: None,
            armings: 0,
            keep: cx.focus_handle(),
            confirm: cx.focus_handle(),
            _subscriptions: subscriptions,
            _woken: woken,
        };
        screen.sync(window, cx);
        screen
    }

    fn heading(&self) -> &FocusHandle {
        &self.heading
    }
}

impl Render for ChannelsScreen {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = self.view_model();
        let (status, alert) = match &view.outcome {
            Some(outcome) if outcome.ok => (outcome.message.clone(), String::new()),
            Some(outcome) => (String::new(), outcome.message.clone()),
            None => (String::new(), String::new()),
        };
        let known = view.known;
        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .font_family(FONT_SANS)
            .child(heading(
                "channels-heading",
                Self::SECTION.title(),
                &self.heading,
                false,
            ))
            .child(
                div()
                    .max_w(px(720.0))
                    .text_size(px(text::BASE))
                    .text_color(color(colors::TEXT_SECONDARY))
                    .child(LEAD),
            )
            .when(!known, |screen| screen.child(self.note()))
            .child(atoms::message_line("channels-outcome", false, &status))
            .child(atoms::message_line("channels-failure", true, &alert))
            .when(known, |screen| {
                screen.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_start()
                        .gap(px(16.0))
                        .child(self.table(&view, window, cx))
                        .child(self.groups(&view, window, cx)),
                )
            })
    }
}
