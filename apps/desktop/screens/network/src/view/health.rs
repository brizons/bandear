//! The link health panel (`link-health.js`): the musician's name and range, Close, the range
//! buttons, the summary with its link mark, the error and empty lines, the chart, the notes, the
//! key, and the table of every second (B1-B6).

use bandear_desktop_kit::a11y::{seen, toggled};
use bandear_desktop_kit::buttons::{Button, ButtonKind};
use bandear_desktop_kit::gpui::{
    Context, InteractiveElement, IntoElement, ParentElement, Role, SharedString,
    StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
};
use bandear_desktop_kit::status::LinkMark;
use bandear_desktop_kit::theme::{FONT_MONO, FONT_SANS, color, colors, radius, text, weight};

use super::NetworkScreen;
use super::parts::{Tone, heading2, line};
use crate::model::link_view::{PanelView, TABLE_COLUMNS};

/// What the chart's marks mean.
const KEY: &str = "A line per measure, one point a second. Shaded: seconds with no report. A vertical line: the phone reconnected. A tick: an underrun or a timeout in that second.";

impl NetworkScreen {
    pub(super) fn render_health(
        &self,
        panel: &PanelView,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let shown_table = panel.table.is_some();
        let ranges = panel.ranges.iter().map(|range| {
            let ms = range.ms;
            div()
                .id(SharedString::from(format!("range-{ms}")))
                .role(Role::Button)
                .aria_label(format!("Last {}", range.label))
                .aria_toggled(toggled(range.pressed))
                .focusable()
                .cursor_pointer()
                .min_w(px(64.0))
                .min_h(px(28.0))
                .px(px(8.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(radius::SM))
                .border_1()
                .border_color(color(if range.pressed {
                    colors::ACCENT
                } else {
                    colors::BORDER
                }))
                .bg(color(if range.pressed {
                    colors::ACCENT
                } else {
                    colors::SURFACE_RAISED
                }))
                .text_color(color(if range.pressed {
                    colors::TEXT_ON_BRIGHT
                } else {
                    colors::TEXT
                }))
                .font_family(FONT_MONO)
                .text_size(px(text::SM))
                .font_weight(weight::BOLD)
                .focus_visible(|style| style.border_2().border_color(color(colors::FOCUS)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.model.set_range(ms);
                    cx.notify();
                }))
                .child(seen(range.label))
        });
        let model = self.model.clone();
        let toggler = self.model.clone();
        div()
            .id("link-health")
            .role(Role::Region)
            .aria_label(panel.title.clone())
            .flex()
            .flex_col()
            .gap(px(10.0))
            .py(px(14.0))
            .px(px(16.0))
            .rounded(px(radius::XL))
            .border_1()
            .border_color(color(colors::BORDER))
            .bg(color(colors::SURFACE))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(heading2(panel.title.clone()))
                    .child(div().flex_grow(1.0))
                    .child(
                        Button::new("link-close", "Close", ButtonKind::Secondary)
                            .description(format!("Close the link health of {}", panel.name))
                            .on_press(move |_, _| model.close_health()),
                    ),
            )
            .child(
                div()
                    .id("ranges")
                    .role(Role::Group)
                    .aria_label("Range")
                    .flex()
                    .gap(px(6.0))
                    .children(ranges),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.0))
                    .font_family(FONT_SANS)
                    .text_size(px(text::SM))
                    .text_color(color(colors::TEXT_MUTED))
                    .child(
                        LinkMark::new("summary-mark", panel.summary.level)
                            .label(panel.summary.label.clone()),
                    )
                    .child(seen(panel.summary.text.clone())),
            )
            .when_some(panel.message.clone(), |card, message| {
                card.child(line("link-error", Tone::Error, message))
            })
            .map(|card| match panel.empty {
                Some(empty) => card.child(line("link-empty", Tone::Hint, empty)),
                None => card.child(self.render_chart(panel, cx)),
            })
            .when_some(panel.extras_note.clone(), |card, note| {
                card.child(line("extras-note", Tone::Hint, note))
            })
            .when_some(panel.recovered_note, |card, note| {
                card.child(line("recovered-note", Tone::Hint, note))
            })
            .child(line("link-key", Tone::Hint, KEY))
            .child(
                div().flex().child(
                    div()
                        .id("table-toggle")
                        .role(Role::Button)
                        .aria_label(if shown_table {
                            "Hide the table"
                        } else {
                            "Show as table"
                        })
                        .aria_expanded(shown_table)
                        .focusable()
                        .cursor_pointer()
                        .min_h(px(32.0))
                        .px(px(12.0))
                        .flex()
                        .items_center()
                        .rounded(px(radius::MD))
                        .border_1()
                        .border_color(color(colors::BORDER))
                        .bg(color(colors::SURFACE_RAISED))
                        .font_weight(weight::BOLD)
                        .text_size(px(text::SM))
                        .focus_visible(|style| style.border_2().border_color(color(colors::FOCUS)))
                        .on_click(move |_, _, _| toggler.show_table(!shown_table))
                        .child(seen(if shown_table {
                            "Hide the table"
                        } else {
                            "Show as table"
                        })),
                ),
            )
            .when_some(panel.table.as_ref(), |card, rows| {
                let columns: Vec<usize> = TABLE_COLUMNS
                    .iter()
                    .enumerate()
                    .filter(|(_, (_, extra))| extra.is_none_or(|reported| reported(&panel.extras)))
                    .map(|(index, _)| index)
                    .collect();
                let width = |index: usize| if index == 0 || index == 9 { 90.0 } else { 84.0 };
                let right = |index: usize| !(index <= 1 || index == 9);
                let head =
                    div()
                        .id("history-head")
                        .flex()
                        .role(Role::Row)
                        .children(columns.iter().map(|index| {
                            div()
                                .id(("history-column", *index))
                                .role(Role::ColumnHeader)
                                .w(px(width(*index)))
                                .px(px(6.0))
                                .py(px(4.0))
                                .flex()
                                .when(right(*index), |cell| cell.justify_end())
                                .text_color(color(colors::TEXT_MUTED))
                                .child(seen(TABLE_COLUMNS[*index].0))
                        }));
                let body = rows.iter().map(|row| {
                    let cells = row.cells();
                    let key = row.key as usize;
                    div()
                        .id(("history-row", key))
                        .flex()
                        .role(Role::Row)
                        .border_t_1()
                        .border_color(color(colors::BORDER_SUBTLE))
                        .children(columns.iter().map(|index| {
                            div()
                                .id(SharedString::from(format!("history-cell-{key}-{index}")))
                                .role(Role::Cell)
                                .w(px(width(*index)))
                                .px(px(6.0))
                                .py(px(3.0))
                                .flex()
                                .when(right(*index), |cell| cell.justify_end())
                                .font_family(FONT_MONO)
                                .child(seen(cells[*index].to_owned()))
                        }))
                });
                card.child(
                    div()
                        .id("history-table")
                        .role(Role::Table)
                        .aria_label("Link history")
                        .max_h(px(320.0))
                        .overflow_y_scroll()
                        .text_size(px(text::XS))
                        .child(head)
                        .children(body),
                )
            })
    }
}
