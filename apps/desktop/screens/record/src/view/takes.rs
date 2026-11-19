//! The takes card: the recordings folder and Refresh, the listing's error and what it left out,
//! the takes table (Take, Started, Length, Ended and each take's Soundcheck and Open folder) and
//! what to say when there is no take.

use bandear_desktop_console::state::ConsoleState;
use bandear_desktop_kit::a11y::{seen, visually_hidden};
use bandear_desktop_kit::buttons::{Button, ButtonKind};
use bandear_desktop_kit::gpui::{
    AnyElement, Context, Div, InteractiveElement, IntoElement, ParentElement, Role, SharedString,
    StatefulInteractiveElement, Styled, div, prelude::FluentBuilder, px,
};
use bandear_desktop_kit::theme::{FONT_MONO, color, colors, space, text, weight};

use super::RecordScreen;
use super::atoms::{self, BadgeTone, badge};
use crate::model::local_offset_minutes;
use crate::record_view::{TakeRow, record_messages, take_rows};
use crate::recording::format_unreadable;
use crate::soundcheck_view::RowSoundcheck;

/// The table's columns: Take grows, the others keep their width.
const STARTED: f32 = 140.0;
const LENGTH: f32 = 80.0;
const ENDED: f32 = 140.0;
const ACTIONS: f32 = 200.0;

fn cell(width: Option<f32>) -> Div {
    let cell = div().px(px(space::S2)).min_w_0();
    match width {
        Some(width) => cell.flex_none().w(px(width)),
        None => cell.flex_1(),
    }
}

impl RecordScreen {
    /// One row of the takes table.
    fn take_line(
        &self,
        index: usize,
        row: &TakeRow,
        soundcheck: &RowSoundcheck,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let folder = row.folder.clone();
        let check = {
            let folder = folder.clone();
            Button::new(
                SharedString::from(format!("record-take-soundcheck-{index}")),
                if soundcheck.playing {
                    "Playing"
                } else {
                    "Soundcheck"
                },
                ButtonKind::Secondary,
            )
            .description(soundcheck.title.clone())
            .disabled(soundcheck.disabled)
            .on_press(self.press(cx, move |model| model.soundcheck_take(folder.clone())))
        };
        let open = Button::new(
            SharedString::from(format!("record-take-open-{index}")),
            "Open folder",
            ButtonKind::Secondary,
        )
        .description(format!("Open the folder of {}", row.name))
        .on_press(self.press(cx, move |model| model.open_take_folder(folder.clone())));
        div()
            .id(SharedString::from(format!("record-take-{index}")))
            .role(Role::Row)
            .flex()
            .items_center()
            .py(px(10.0))
            .px(px(space::S2))
            .border_t_1()
            .border_color(color(colors::BORDER_SUBTLE))
            .text_size(px(text::MD))
            .child(
                cell(None)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(space::S2))
                            .font_weight(weight::BOLD)
                            .when(row.recording, |name| {
                                name.child(atoms::dot(8.0, colors::RED))
                            })
                            .child(seen(row.name.clone())),
                    )
                    .child(
                        div()
                            .text_size(px(text::XS))
                            .text_color(color(colors::TEXT_MUTED))
                            .child(seen(row.detail())),
                    ),
            )
            .child(cell(Some(STARTED)).child(seen(row.started.clone())))
            .child(
                cell(Some(LENGTH))
                    .font_family(FONT_MONO)
                    .child(seen(row.length.clone())),
            )
            .child(cell(Some(ENDED)).flex().child(badge(
                SharedString::from(format!("record-take-end-{index}")),
                row.end.text,
                BadgeTone::from(row.end.kind),
                Some(row.end.detail.clone().into()),
            )))
            .child(
                cell(Some(ACTIONS))
                    .flex()
                    .justify_end()
                    .gap(px(6.0))
                    .child(check)
                    .child(open),
            )
    }

    /// The takes card.
    pub(super) fn takes(&self, state: &ConsoleState, cx: &Context<Self>) -> impl IntoElement {
        let takes = &state.takes;
        let rows = take_rows(state, local_offset_minutes());
        let empty: Option<&'static str> = match &takes.list {
            None if takes.error.is_none() => Some("Listing the takes\u{2026}"),
            None => None,
            Some(_) if !rows.is_empty() => None,
            Some(_) if takes.missing => {
                Some("The recordings folder does not exist yet: the first take makes it.")
            }
            Some(_) => Some("No takes in this folder yet."),
        };
        let header = |words: &'static str, width: Option<f32>| -> AnyElement {
            cell(width)
                .text_size(px(text::XS))
                .font_weight(weight::MEDIUM)
                .text_color(color(colors::TEXT_MUTED))
                .child(seen(words.to_uppercase()))
                .into_any_element()
        };
        let lines: Vec<_> = rows
            .iter()
            .enumerate()
            .map(|(index, (row, soundcheck))| {
                self.take_line(index, row, soundcheck, cx)
                    .into_any_element()
            })
            .collect();
        // It takes the room the aside leaves (`flex: 999 1 560px`).
        atoms::card("record-takes", "Takes")
            .flex_grow(999.0)
            .flex_basis(px(560.0))
            .min_w_0()
            .pb(px(space::S2))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(space::S2))
                    .pt(px(space::S3))
                    .px(px(space::S4))
                    .pb(px(space::S1))
                    .child(atoms::title("record-takes-title", "Takes"))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_family(FONT_MONO)
                            .text_size(px(text::SM))
                            .text_color(color(colors::TEXT_MUTED))
                            .truncate()
                            .child(seen(takes.folder.clone().unwrap_or_default())),
                    )
                    .child(
                        Button::new("record-refresh", "Refresh", ButtonKind::Secondary)
                            .on_press(self.press(cx, |model| model.refresh_takes())),
                    ),
            )
            .when_some(record_messages(state).takes, |card, error| {
                card.child(
                    div()
                        .mx(px(space::S4))
                        .my(px(space::S1))
                        .child(atoms::message("record-takes-error", error, true)),
                )
            })
            .when_some(format_unreadable(takes.unreadable), |card, note| {
                card.child(atoms::hint(note).mx(px(space::S4)).my(px(space::S1)))
            })
            .child(
                div()
                    .id("record-takes-table")
                    .role(Role::Table)
                    .aria_label("Takes")
                    .child(
                        div()
                            .flex()
                            .py(px(space::S2))
                            .px(px(space::S2))
                            .child(header("Take", None))
                            .child(header("Started", Some(STARTED)))
                            .child(header("Length", Some(LENGTH)))
                            .child(header("Ended", Some(ENDED)))
                            .child(
                                cell(Some(ACTIONS))
                                    .relative()
                                    .child(visually_hidden(div()).child(seen("Actions"))),
                            ),
                    )
                    .children(lines),
            )
            .when_some(empty, |card, empty| {
                card.child(
                    div()
                        .py(px(space::S3))
                        .px(px(space::S4))
                        .text_size(px(text::MD))
                        .text_color(color(colors::TEXT_SECONDARY))
                        .child(seen(empty)),
                )
            })
    }
}
