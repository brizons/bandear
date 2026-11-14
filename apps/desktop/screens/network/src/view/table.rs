//! The musicians table (`musicians-table.js`): a row per phone with its name, control channel,
//! link, Buffer and Redundancy cells, mix, counts and a guarded Remove; under it the form that adds
//! a phone by address and the hint. Rows are rebuilt only when who is listed changes, so a field
//! being typed in keeps its text and its focus (N5).

use bandear_desktop_kit::a11y::{seen, toggled};
use bandear_desktop_kit::buttons::{Button, ButtonKind};
use bandear_desktop_kit::forms::{Choice, ChoiceState, choice_state, text_input};
use bandear_desktop_kit::gpui::{
    AnyElement, AppContext as _, Context, Div, Entity, Focusable, InteractiveElement, IntoElement,
    KeyDownEvent, ParentElement, Role, SharedString, StatefulInteractiveElement, Styled,
    Subscription, Window, div, prelude::FluentBuilder, px,
};
use bandear_desktop_kit::gpui_component::input::{InputEvent, InputState};
use bandear_desktop_kit::gpui_component::select::SelectEvent;
use bandear_desktop_kit::status::LinkMark;
use bandear_desktop_kit::theme::{FONT_MONO, FONT_SANS, color, colors, radius, text, weight};
use bandear_desktop_kit::tooltip::tooltip;

use super::NetworkScreen;
use super::parts::{Option_, OptionState, Shown, Tone, line, mono, option_state, sync_select};
use crate::model::ShownRow;
use crate::model::musicians::{BufferMode, DEFAULT_FIXED_MS};

/// The hint under the add form.
const HINT: &str = "For a fixed rack or for debugging. Adding or removing a phone never restarts the engine: the capture, the control server and the discovery beacon keep running. Removing stops that phone's audio at once; its control channel stays open and its next mix is answered with \"no UDP target configured for client IP\".";

/// The columns: header, width, tooltip, right-aligned.
pub(super) const COLUMNS: [(&str, f32, Option<&str>, bool); 10] = [
    ("Musician", 250.0, None, false),
    ("Control", 112.0, None, false),
    (
        "Link",
        200.0,
        Some("What the musician's phone reports about the audio it receives"),
        false,
    ),
    (
        "Buffer",
        150.0,
        Some(
            "The phone's playback buffer: Auto lets the phone adapt it; Fixed keeps the length set here. Under it, the cushion the phone reports it plays",
        ),
        false,
    ),
    (
        "Redundancy",
        230.0,
        Some(
            "Redundancy: every packet sent twice, the copy right after it or a few blocks later; it doubles this musician's bandwidth, and a later copy adds that much latency. Under it, the copies sent and the packets the phone reports a delayed copy recovered",
        ),
        false,
    ),
    ("Mix", 96.0, None, false),
    ("Sent", 72.0, None, true),
    ("Discarded", 88.0, None, true),
    (
        "Loss",
        72.0,
        Some("Packets this computer discarded for the phone"),
        true,
    ),
    ("Actions", 200.0, None, true),
];

pub(super) const COPY_TITLE: &str = "Immediate: the copy goes right after its packet, with no added latency. Later: a Wi-Fi burst must last that long to take both, and the phone plays that much later";

/// One row's own fields.
pub struct RowFields {
    pub address: String,
    pub(super) name: Entity<InputState>,
    pub(super) ms: Entity<InputState>,
    pub(super) mode: Entity<ChoiceState>,
    mode_shown: Shown,
    pub(super) copy: Entity<OptionState>,
    copy_shown: Shown,
    /// The name the desk has, as last drawn.
    saved_name: String,
    /// The length in the field when it took the focus: leaving it unchanged sends nothing.
    ms_at_focus: String,
    _subscriptions: Vec<Subscription>,
}

/// The add form's field: Enter adds.
pub fn subscribe_address(
    address: &Entity<InputState>,
    window: &mut Window,
    cx: &mut Context<NetworkScreen>,
) -> Vec<Subscription> {
    vec![
        cx.subscribe_in(address, window, |this, state, event: &InputEvent, _, cx| {
            if let InputEvent::PressEnter { .. } = event {
                let typed = state.read(cx).value().to_string();
                this.model.add_phone(&typed);
            }
        }),
    ]
}

pub(super) fn cell(width: f32, right: bool) -> Div {
    div()
        .flex_none()
        .w(px(width))
        .px(px(8.0))
        .py(px(10.0))
        .flex()
        .flex_col()
        .gap(px(4.0))
        .when(right, |cell| cell.items_end())
}

pub(super) fn small(words: impl Into<SharedString>, tone: u32) -> Div {
    div()
        .font_family(FONT_SANS)
        .text_size(px(text::XS))
        .text_color(color(tone))
        .child(seen(words.into()))
}

impl NetworkScreen {
    /// Rebuilds the rows when who is listed changed, and brings each row's fields in line.
    pub(super) fn sync_rows(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rows = self.view.table.rows.clone();
        let same = rows.len() == self.rows.len()
            && rows
                .iter()
                .zip(&self.rows)
                .all(|(row, fields)| row.row.address == fields.address);
        if !same {
            self.rows = rows
                .iter()
                .map(|row| self.row_fields(&row.row.address, window, cx))
                .collect();
        }
        for (row, fields) in rows.iter().zip(self.rows.iter_mut()) {
            let editable = row.row.editable;
            let typing = fields.name.read(cx).focus_handle(cx).is_focused(window);
            if (!typing || !editable) && fields.name.read(cx).value().as_ref() != row.row.name {
                let name = row.row.name.clone();
                fields
                    .name
                    .update(cx, |state, cx| state.set_value(name, window, cx));
            }
            fields.saved_name = row.row.name.clone();
            fields
                .name
                .update(cx, |state, cx| state.set_disabled(!editable, cx));
            let mode = match row.buffer_mode {
                BufferMode::Auto => "auto",
                BufferMode::Fixed => "fixed",
            };
            sync_select(
                &fields.mode,
                &mut fields.mode_shown,
                vec![Choice::new("auto", "Auto"), Choice::new("fixed", "Fixed")],
                mode,
                window,
                cx,
            );
            let editing = fields.ms.read(cx).focus_handle(cx).is_focused(window);
            if !editing && !row.buffer_pending {
                let current = fields.ms.read(cx).value().to_string();
                let shown = match row.row.buffer.ms {
                    Some(ms) => ms.to_string(),
                    None if current.is_empty() => DEFAULT_FIXED_MS.to_string(),
                    None => current.clone(),
                };
                if shown != current {
                    fields
                        .ms
                        .update(cx, |state, cx| state.set_value(shown, window, cx));
                }
            }
            let delays = row
                .row
                .redundancy
                .delays
                .iter()
                .map(|delay| Option_ {
                    value: delay.value.to_string().into(),
                    label: delay.label.clone().into(),
                    disabled: delay.disabled,
                })
                .collect();
            sync_select(
                &fields.copy,
                &mut fields.copy_shown,
                delays,
                &row.redundancy_delay.to_string(),
                window,
                cx,
            );
        }
    }

    fn row_fields(&self, address: &str, window: &mut Window, cx: &mut Context<Self>) -> RowFields {
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("unnamed"));
        let ms = cx.new(|cx| InputState::new(window, cx));
        let mode = choice_state(Vec::new(), None, window, cx);
        let copy = option_state(Vec::new(), window, cx);
        let at = address.to_owned();
        let named = address.to_owned();
        let timed = address.to_owned();
        let moded = address.to_owned();
        let copied = address.to_owned();
        let subscriptions = vec![
            cx.subscribe_in(
                &name,
                window,
                move |this, _, event: &InputEvent, window, cx| {
                    match event {
                        // Enter commits by leaving the field, as the web console's did.
                        InputEvent::PressEnter { .. } => window.blur(cx),
                        InputEvent::Blur => this.commit_name(&named, cx),
                        _ => {}
                    }
                },
            ),
            cx.subscribe_in(
                &ms,
                window,
                move |this, state, event: &InputEvent, window, cx| match event {
                    InputEvent::PressEnter { .. } => window.blur(cx),
                    InputEvent::Focus => {
                        let value = state.read(cx).value().to_string();
                        if let Some(fields) = this.fields_mut(&timed) {
                            fields.ms_at_focus = value;
                        }
                    }
                    InputEvent::Blur => this.commit_ms(&timed, cx),
                    _ => {}
                },
            ),
            cx.subscribe_in(
                &mode,
                window,
                move |this, _, event: &SelectEvent<Vec<Choice>>, _, cx| {
                    let SelectEvent::Confirm(Some(mode)) = event else {
                        return;
                    };
                    let typed = this
                        .fields(&moded)
                        .map(|fields| fields.ms.read(cx).value().to_string())
                        .unwrap_or_default();
                    this.model.ask_buffer(&moded, mode, &typed);
                },
            ),
            cx.subscribe_in(
                &copy,
                window,
                move |this, _, event: &SelectEvent<Vec<Option_>>, _, _| {
                    let SelectEvent::Confirm(Some(delay)) = event else {
                        return;
                    };
                    if let Ok(delay) = delay.parse::<u8>() {
                        this.model.ask_redundancy(&copied, true, Some(delay));
                    }
                },
            ),
        ];
        RowFields {
            address: at,
            name,
            ms,
            mode,
            mode_shown: Shown::default(),
            copy,
            copy_shown: Shown::default(),
            saved_name: String::new(),
            ms_at_focus: String::new(),
            _subscriptions: subscriptions,
        }
    }

    fn fields(&self, address: &str) -> Option<&RowFields> {
        self.rows.iter().find(|fields| fields.address == address)
    }

    fn fields_mut(&mut self, address: &str) -> Option<&mut RowFields> {
        self.rows
            .iter_mut()
            .find(|fields| fields.address == address)
    }

    fn shown_row(&self, address: &str) -> Option<&ShownRow> {
        self.view
            .table
            .rows
            .iter()
            .find(|row| row.row.address == address)
    }

    /// Sends the name typed when it differs from the desk's; a field turned read-only meanwhile
    /// (the phone announced a name) commits nothing (N5).
    fn commit_name(&mut self, address: &str, cx: &mut Context<Self>) {
        let editable = self.shown_row(address).is_some_and(|row| row.row.editable);
        let Some(fields) = self.fields(address) else {
            return;
        };
        let typed = fields.name.read(cx).value().trim().to_owned();
        if editable && typed != fields.saved_name {
            self.model.rename(address, &typed);
        }
    }

    /// Sends the length typed, while the row shows Fixed and the length changed (N11).
    fn commit_ms(&mut self, address: &str, cx: &mut Context<Self>) {
        let fixed = self
            .shown_row(address)
            .is_some_and(|row| row.buffer_mode == BufferMode::Fixed);
        let Some(fields) = self.fields(address) else {
            return;
        };
        let typed = fields.ms.read(cx).value().to_string();
        if fixed && typed != fields.ms_at_focus {
            self.model.ask_buffer(address, "fixed", &typed);
        }
    }

    /// Escape in the name field: the saved name goes back and the field is left.
    fn revert_name(&mut self, address: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(fields) = self.fields(address) {
            let saved = fields.saved_name.clone();
            fields
                .name
                .update(cx, |state, cx| state.set_value(saved, window, cx));
        }
        window.blur(cx);
    }

    /// Escape in the length field: the last length reported (or 40) goes back.
    pub(super) fn revert_ms(&mut self, address: &str, window: &mut Window, cx: &mut Context<Self>) {
        let reported = self
            .shown_row(address)
            .and_then(|row| row.row.buffer.ms)
            .map_or(DEFAULT_FIXED_MS.to_string(), |ms| ms.to_string());
        if let Some(fields) = self.fields_mut(address) {
            fields.ms_at_focus = reported.clone();
            fields
                .ms
                .update(cx, |state, cx| state.set_value(reported, window, cx));
        }
        window.blur(cx);
    }

    /// A click on a row outside its fields and buttons opens the musician's link panel (N6).
    fn on_select(
        cx: &mut Context<Self>,
        address: String,
    ) -> impl Fn(
        &bandear_desktop_kit::gpui::ClickEvent,
        &mut Window,
        &mut bandear_desktop_kit::gpui::App,
    )
    + 'static
    + use<> {
        let screen = cx.entity().downgrade();
        move |_, _, cx| {
            if let Some(screen) = screen.upgrade() {
                screen.update(cx, |this, cx| {
                    this.model.select_musician(&address);
                    cx.notify();
                });
            }
        }
    }

    pub(super) fn render_table(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let table = &self.view.table;
        let header = div().id("table-header").flex().role(Role::Row).children(
            COLUMNS
                .iter()
                .enumerate()
                .map(|(index, (title, width, tip, right))| {
                    let tip = tip.map(SharedString::from);
                    cell(*width, *right)
                        .id(("column", index))
                        .role(Role::ColumnHeader)
                        .aria_label(*title)
                        .py(px(12.0))
                        .when(index == 0, |cell| cell.pl(px(14.0)))
                        .font_family(FONT_SANS)
                        .text_size(px(text::XS))
                        .font_weight(weight::MEDIUM)
                        .text_color(color(colors::TEXT_MUTED))
                        .when_some(tip, |cell, tip| {
                            cell.tooltip(move |window, cx| tooltip(tip.clone(), window, cx))
                        })
                        // The actions' header is read, not seen.
                        .when(index < COLUMNS.len() - 1, |cell| {
                            cell.child(seen(title.to_uppercase()))
                        })
                }),
        );
        let rows: Vec<AnyElement> = table
            .rows
            .iter()
            .zip(&self.rows)
            .map(|(row, fields)| self.render_row(row, fields, cx))
            .collect();
        let empty = rows.is_empty();
        let address_invalid = table.add_error.is_some();
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .when_some(table.table_error.clone(), |table, error| {
                table.child(line("table-error", Tone::Error, error))
            })
            .child(
                div()
                    .id("musicians-table")
                    .role(Role::Table)
                    .aria_label("Musicians")
                    .overflow_x_scroll()
                    .rounded(px(radius::XL))
                    .border_1()
                    .border_color(color(colors::BORDER))
                    .bg(color(colors::SURFACE))
                    .text_size(px(text::MD))
                    .child(div().flex().flex_col().min_w(px(1490.0)).child(header).children(rows))
                    .when(empty, |table| {
                        table.child(
                            div()
                                .px(px(14.0))
                                .py(px(12.0))
                                .text_color(color(colors::TEXT_MUTED))
                                .child(seen("No musicians yet. A phone that finds this computer appears here on its own.")),
                        )
                    }),
            )
            .child(
                div()
                    .id("add-form")
                    .role(Role::Form)
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.0))
                    .child(super::parts::label("Add a phone by address"))
                    .child(
                        div()
                            .w(px(220.0))
                            .font_family(FONT_MONO)
                            .when(address_invalid, |field| {
                                field.border_1().border_color(color(colors::DANGER_BORDER)).rounded(px(radius::SM))
                            })
                            .child(text_input(&self.address, "Add a phone by address")),
                    )
                    .child(
                        Button::new("add-phone", "Add phone", ButtonKind::Primary).on_press({
                            let address = self.address.clone();
                            let model = self.model.clone();
                            move |_, cx| {
                                let typed = address.read(cx).value().to_string();
                                model.add_phone(&typed);
                            }
                        }),
                    )
                    .when_some(table.add_error.clone(), |form, error| {
                        form.child(div().w_full().child(line("add-error", Tone::Error, error)))
                    }),
            )
            .child(line("add-hint", Tone::Hint, HINT))
    }

    fn render_row(&self, row: &ShownRow, fields: &RowFields, cx: &mut Context<Self>) -> AnyElement {
        let musician = &row.row;
        let address = musician.address.clone();
        let label = musician.label().to_owned();
        let select = |cx: &mut Context<Self>, address: String| Self::on_select(cx, address);
        let reverting = address.clone();
        let name_cell = cell(COLUMNS[0].1, false)
            .pl(px(14.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        div()
                            .id(SharedString::from(format!("name-{address}")))
                            .w(px(150.0))
                            .font_weight(weight::BOLD)
                            .tooltip({
                                let title = SharedString::from(musician.name_title());
                                move |window, cx| tooltip(title.clone(), window, cx)
                            })
                            .on_key_down(cx.listener(
                                move |this, event: &KeyDownEvent, window, cx| {
                                    if event.keystroke.key == "escape" {
                                        this.revert_name(&reverting, window, cx);
                                    }
                                },
                            ))
                            .map(|field| {
                                if musician.editable {
                                    field
                                        .child(text_input(&fields.name, format!("Name of {label}")))
                                } else {
                                    // The phone's own name: read-only, without a field's frame.
                                    field
                                        .role(Role::Label)
                                        .aria_label(format!("Name of {label}"))
                                        .px(px(8.0))
                                        .child(seen(musician.name.clone()))
                                }
                            }),
                    )
                    .when(musician.name_is_local, |line| {
                        line.child(
                            div()
                                .px(px(6.0))
                                .py(px(1.0))
                                .border_1()
                                .border_dashed()
                                .border_color(color(colors::TEXT_MUTED))
                                .rounded(px(radius::SM))
                                .text_size(px(text::XS))
                                .text_color(color(colors::TEXT_SECONDARY))
                                .whitespace_nowrap()
                                .child(seen("local label")),
                        )
                    }),
            )
            .child(mono(address.clone(), text::XS));
        let control = cell(COLUMNS[1].1, false)
            .id(SharedString::from(format!("control-{address}")))
            .cursor_pointer()
            .on_click(select(cx, address.clone()))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .whitespace_nowrap()
                    .child(div().size(px(8.0)).rounded_full().map(|dot| {
                        if musician.connected {
                            dot.bg(color(colors::GREEN))
                        } else {
                            dot.border_2().border_color(color(colors::TEXT_MUTED))
                        }
                    }))
                    .child(seen(if musician.connected {
                        "connected"
                    } else {
                        "offline"
                    })),
            );
        let history_address = address.clone();
        let link = cell(COLUMNS[2].1, false)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        LinkMark::new(
                            SharedString::from(format!("link-{address}")),
                            musician.link.level,
                        )
                        .label(musician.link.label.clone())
                        .detail(musician.link.detail.clone()),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("history-{address}")))
                            .role(Role::Button)
                            .aria_label(format!("Link history of {label}"))
                            .aria_toggled(toggled(row.selected))
                            .focusable()
                            .cursor_pointer()
                            .min_h(px(26.0))
                            .px(px(8.0))
                            .flex()
                            .items_center()
                            .rounded(px(radius::MD))
                            .border_1()
                            .border_color(color(if row.selected {
                                colors::ACCENT
                            } else {
                                colors::BORDER
                            }))
                            .bg(color(colors::SURFACE_RAISED))
                            .text_size(px(text::XS))
                            .font_weight(weight::BOLD)
                            .text_color(color(if row.selected {
                                colors::ACCENT
                            } else {
                                colors::TEXT
                            }))
                            .focus_visible(|style| style.border_color(color(colors::FOCUS)))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.model.select_musician(&history_address);
                                cx.notify();
                            }))
                            .child(seen("History")),
                    ),
            )
            .child(
                div()
                    .id(SharedString::from(format!("detail-{address}")))
                    .on_click(select(cx, address.clone()))
                    .child(mono(musician.link.detail.clone(), text::XS)),
            );
        let buffer = self.render_buffer(row, fields, &label, cx);
        let redundancy = self.render_redundancy(row, fields, &label, cx);
        let number = |index: usize, value: &str, warn: bool, on_click| {
            cell(COLUMNS[index].1, true)
                .id(SharedString::from(format!("{index}-{address}")))
                .cursor_pointer()
                .on_click(on_click)
                .font_family(FONT_MONO)
                .text_color(color(if warn { colors::ACCENT } else { colors::TEXT }))
                .whitespace_nowrap()
                .child(seen(value.to_owned()))
        };
        let mix = cell(COLUMNS[5].1, false)
            .id(SharedString::from(format!("mix-{address}")))
            .cursor_pointer()
            .on_click(select(cx, address.clone()))
            .whitespace_nowrap()
            .child(seen(musician.mix.clone()));
        let actions = self.render_actions(row, &label, cx);
        let on_sent = select(cx, address.clone());
        let on_discarded = select(cx, address.clone());
        let on_loss = select(cx, address.clone());
        let escape = address.clone();
        div()
            .id(SharedString::from(format!("row-{address}")))
            .role(Role::Row)
            .aria_label(label.clone())
            .aria_selected(row.selected)
            .flex()
            .items_center()
            .border_t_1()
            .border_color(color(colors::BORDER_SUBTLE))
            .when(row.selected, |row| row.bg(color(colors::SURFACE_RAISED)))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape"
                    && this
                        .shown_row(&escape)
                        .is_some_and(|row| row.removing.is_some())
                {
                    this.model.keep_remove();
                    cx.notify();
                }
            }))
            .child(name_cell)
            .child(control)
            .child(link)
            .child(buffer)
            .child(redundancy)
            .child(mix)
            .child(number(6, &musician.sent, false, on_sent))
            .child(number(7, &musician.discarded, false, on_discarded))
            .child(number(8, &musician.loss, musician.loss_high, on_loss))
            .child(actions)
            .into_any_element()
    }
}
