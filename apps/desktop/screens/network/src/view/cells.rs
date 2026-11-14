//! The musicians table's cells that act (`musicians-table.js`): the Buffer cell (Auto or Fixed,
//! its length, the cushion the phone plays and the last outcome), the Redundancy cell (the switch,
//! the Copy choice, what it costs, the counts and the last outcome) and the guarded Remove.

use bandear_desktop_kit::a11y::{seen, toggled};
use bandear_desktop_kit::buttons::{Button, ButtonKind};
use bandear_desktop_kit::forms::{select, text_input};
use bandear_desktop_kit::gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, KeyDownEvent, ParentElement, Role,
    SharedString, StatefulInteractiveElement, Styled, div, prelude::FluentBuilder, px,
};
use bandear_desktop_kit::gpui_component::select::Select;
use bandear_desktop_kit::theme::{FONT_MONO, color, colors, text};
use bandear_desktop_kit::tooltip::tooltip;

use super::NetworkScreen;
use super::parts::{focus_button, mono, said_line};
use super::table::{COLUMNS, COPY_TITLE, RowFields, cell, small};
use crate::model::ShownRow;
use crate::model::musicians::BufferMode;

impl NetworkScreen {
    pub(super) fn render_buffer(
        &self,
        row: &ShownRow,
        fields: &RowFields,
        label: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let buffer = &row.row.buffer;
        let address = row.row.address.clone();
        let detail = if buffer.supported {
            buffer.playing.clone().unwrap_or_default()
        } else {
            buffer.note.unwrap_or_default().to_owned()
        };
        let detail_title = SharedString::from(if buffer.supported {
            "The cushion the phone reports it plays"
        } else {
            "This phone's app does not take its playback buffer from the desk; update BandEar on it"
        });
        cell(COLUMNS[3].1, false)
            .when(buffer.supported, |cell| {
                cell.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(
                            div()
                                .w(px(76.0))
                                .child(select(&fields.mode, format!("Playback buffer of {label}"))),
                        )
                        .when(row.buffer_mode == BufferMode::Fixed, |line| {
                            line.child(
                                div()
                                    .id(SharedString::from(format!("ms-{address}")))
                                    .w(px(52.0))
                                    .font_family(FONT_MONO)
                                    .on_key_down(cx.listener({
                                        let address = address.clone();
                                        move |this, event: &KeyDownEvent, window, cx| {
                                            if event.keystroke.key == "escape" {
                                                this.revert_ms(&address, window, cx);
                                            }
                                        }
                                    }))
                                    .child(text_input(
                                        &fields.ms,
                                        format!(
                                            "Fixed playback buffer of {label}, in milliseconds"
                                        ),
                                    )),
                            )
                            .child(small("ms", colors::TEXT_MUTED))
                        }),
                )
            })
            .child(
                div()
                    .id(SharedString::from(format!("buffer-detail-{address}")))
                    .tooltip(move |window, cx| tooltip(detail_title.clone(), window, cx))
                    .child(mono(detail, text::XS)),
            )
            .when_some(row.buffer_said.as_ref(), |cell, said| {
                cell.child(
                    div()
                        .max_w(px(140.0))
                        .text_size(px(text::XS))
                        .child(said_line(
                            SharedString::from(format!("buffer-said-{address}")),
                            said,
                        )),
                )
            })
            .into_any_element()
    }

    pub(super) fn render_redundancy(
        &self,
        row: &ShownRow,
        fields: &RowFields,
        label: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let cell_view = &row.row.redundancy;
        let address = row.row.address.clone();
        let on = row.redundancy_on;
        let switched = address.clone();
        let switch = div()
            .id(SharedString::from(format!("redundancy-{address}")))
            .role(Role::Switch)
            .aria_label(format!("Send {label}'s audio twice (redundancy)"))
            .aria_toggled(toggled(on))
            .focusable()
            .cursor_pointer()
            .flex()
            .items_center()
            .gap(px(6.0))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.model.ask_redundancy(&switched, !on, None);
                cx.notify();
            }))
            .child(
                div()
                    .w(px(28.0))
                    .h(px(16.0))
                    .rounded_full()
                    .p(px(2.0))
                    .flex()
                    .when(on, |track| track.justify_end().bg(color(colors::BLUE)))
                    .when(!on, |track| track.bg(color(colors::TRACK)))
                    .child(div().size(px(12.0)).rounded_full().bg(color(colors::THUMB))),
            )
            .child(seen(if on { "On" } else { "Off" }));
        let counts = cell_view
            .duplicates
            .iter()
            .chain(cell_view.recovered.iter())
            .cloned()
            .collect::<Vec<_>>()
            .join(" \u{b7} ");
        cell(COLUMNS[4].1, false)
            .text_size(px(text::SM))
            .child(switch)
            .when(on, |cell| {
                cell.child(
                    div()
                        .id(SharedString::from(format!("copy-{address}")))
                        .w(px(210.0))
                        .tooltip(|window, cx| tooltip(COPY_TITLE.into(), window, cx))
                        .child(
                            Select::new(&fields.copy)
                                .accessibility_label(format!("When {label}'s copy is sent")),
                        ),
                )
            })
            .child(
                div()
                    .id(SharedString::from(format!("cost-{address}")))
                    .tooltip(|window, cx| {
                        tooltip(
                            "Every packet is sent twice, so a lost one can still arrive as its copy".into(),
                            window,
                            cx,
                        )
                    })
                    .child(mono(cell_view.bandwidth.clone(), text::XS)),
            )
            .when_some(cell_view.hold_note.clone().filter(|_| on), |cell, note| {
                cell.child(
                    div()
                        .id(SharedString::from(format!("hold-{address}")))
                        .tooltip(|window, cx| {
                            tooltip(
                                "This phone's app cannot hold packets back for a delayed copy; update BandEar on it".into(),
                                window,
                                cx,
                            )
                        })
                        .child(mono(note, text::XS)),
                )
            })
            .when(cell_view.on, |cell| cell.child(mono(counts, text::XS)))
            .when_some(row.redundancy_said.as_ref(), |cell, said| {
                cell.child(
                    div()
                        .max_w(px(200.0))
                        .text_size(px(text::XS))
                        .child(said_line(SharedString::from(format!("redundancy-said-{address}")), said)),
                )
            })
            .into_any_element()
    }

    pub(super) fn render_actions(
        &self,
        row: &ShownRow,
        label: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let address = row.row.address.clone();
        let cell = cell(COLUMNS[9].1, true).pr(px(14.0));
        match row.removing {
            None => {
                let arming = address.clone();
                let keep = self.keep_remove.clone();
                let screen = cx.entity().downgrade();
                cell.child(
                    Button::new(
                        SharedString::from(format!("remove-{address}")),
                        "Remove",
                        ButtonKind::Danger,
                    )
                    .description(format!("Remove {label}"))
                    .on_press(move |window, cx| {
                        if let Some(screen) = screen.upgrade() {
                            screen.update(cx, |this, cx| {
                                this.model.arm_remove(&arming);
                                cx.notify();
                            });
                        }
                        // Keep takes the focus, where the second click would land.
                        keep.focus(window, cx);
                    }),
                )
                .into_any_element()
            }
            Some(ready) => {
                let confirming = address.clone();
                let model = self.model.clone();
                let keeper = self.model.clone();
                cell.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .whitespace_nowrap()
                        .child(seen(format!("Remove {label}?")))
                        .child(
                            Button::new(
                                SharedString::from(format!("confirm-{address}")),
                                "Remove",
                                ButtonKind::Danger,
                            )
                            .disabled(!ready)
                            .on_press(move |_, _| model.confirm_remove(&confirming)),
                        )
                        // Keep sits last, where Remove was.
                        .child(focus_button(
                            SharedString::from(format!("keep-{address}")),
                            "Keep",
                            ButtonKind::Secondary,
                            &self.keep_remove,
                            false,
                            move |_, _| keeper.keep_remove(),
                        )),
                )
                .into_any_element()
            }
        }
    }
}
