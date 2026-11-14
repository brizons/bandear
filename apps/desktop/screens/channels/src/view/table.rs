//! The channel table (`channel-table.js`): one row per input of the device, the columns Input,
//! Name, Instrument, Level, Stereo, Group, Shown and Processing. It scrolls sideways in its frame.

use bandear_desktop_kit::a11y::{seen, toggled};
use bandear_desktop_kit::forms::{select, text_input};
use bandear_desktop_kit::gpui::{
    AnyElement, ClickEvent, Context, InteractiveElement, IntoElement, ParentElement, Role,
    SharedString, StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
};
use bandear_desktop_kit::gpui_component::input::Escape;
use bandear_desktop_kit::meters::LevelMeter;
use bandear_desktop_kit::status::GroupChip;
use bandear_desktop_kit::theme::{FONT_MONO, TARGET, color, colors, radius, text, weight};
use bandear_desktop_kit::tooltip::tooltip;

use super::ChannelsScreen;
use super::atoms::{checkbox, link};
use crate::model::{Edit, ScreenView};
use crate::table::{ChannelRow, row_processing_link};

/// The columns, their titles and widths in px.
const COLUMNS: [(&str, f32); 8] = [
    ("Input", 56.0),
    ("Name", 150.0),
    ("Instrument", 130.0),
    ("Level", 110.0),
    ("Stereo", 170.0),
    ("Group", 84.0),
    ("Shown", 90.0),
    ("Processing", 130.0),
];

const HIDDEN_TITLE: &str = "A hidden input is shown nowhere and no one hears it";
const NAME_TITLE: &str = "Shown on every phone and on the Mix. Enter saves, Escape reverts";
const PROCESSING_TITLE: &str = "Open this input's high-pass, equaliser, gate and compressor";

fn cell(width: f32) -> bandear_desktop_kit::gpui::Div {
    div()
        .flex_none()
        .w(px(width))
        .px(px(8.0))
        .py(px(6.0))
        .flex()
        .flex_col()
        .items_start()
        .justify_center()
        .gap(px(4.0))
}

impl ChannelsScreen {
    /// The table section, beside the groups.
    pub(super) fn table(
        &self,
        view: &ScreenView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let header = div()
            .flex()
            .border_b_1()
            .border_color(color(colors::BORDER))
            .children(COLUMNS.iter().map(|(title, width)| {
                let head = cell(*width)
                    .text_size(px(text::XS))
                    .font_weight(weight::BOLD)
                    .text_color(color(colors::TEXT_MUTED))
                    .child(seen(title.to_uppercase()));
                if *title == "Shown" {
                    let id = SharedString::from("channels-column-shown");
                    div()
                        .id(id)
                        .role(Role::ColumnHeader)
                        .aria_label("Shown")
                        .tooltip(|window, cx| tooltip(HIDDEN_TITLE.into(), window, cx))
                        .child(head)
                        .into_any_element()
                } else {
                    div()
                        .id(SharedString::from(format!("channels-column-{title}")))
                        .role(Role::ColumnHeader)
                        .aria_label(*title)
                        .child(head)
                        .into_any_element()
                }
            }));
        let rows: Vec<AnyElement> = view
            .rows
            .iter()
            .map(|row| self.row(row, window, cx))
            .collect();
        div()
            .id("channels-table-section")
            .role(Role::Region)
            .aria_label("Channels")
            .flex_grow(999.0)
            .flex_basis(px(640.0))
            .min_w_0()
            .rounded(px(radius::LG))
            .border_1()
            .border_color(color(colors::BORDER))
            .bg(color(colors::SURFACE))
            .child(
                div()
                    .id("channels-table")
                    .role(Role::Table)
                    .aria_label("Channels")
                    .overflow_x_scroll()
                    .child(div().flex().flex_col().child(header).children(rows)),
            )
            .into_any_element()
    }

    fn row(&self, row: &ChannelRow, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let index = row.index;
        let level = self
            .model
            .console()
            .meters()
            .and_then(|meters| meters.channels.get(index).copied())
            .map_or(0.0, f64::from);
        let link_target = {
            let state = self.model.console().state();
            row_processing_link(&state, row)
        };
        let id = |part: &str| SharedString::from(format!("channels-row-{index}-{part}"));
        let name = self.rows.get(index).map(|fields| {
            div()
                .id(id("name"))
                .on_action(cx.listener(move |this, _: &Escape, window, cx| {
                    this.revert_name(index, window, cx);
                    cx.stop_propagation();
                }))
                .tooltip(|window, cx| tooltip(NAME_TITLE.into(), window, cx))
                .child(text_input(&fields.name, format!("Name of {}", row.input)))
        });
        let kind = self
            .rows
            .get(index)
            .map(|fields| select(&fields.kind, format!("Instrument of {}", row.label)));
        let shown_label = if row.stereo.linked {
            let partner = row.stereo.partner.unwrap_or(index) + 1;
            format!("Show {} and CH {partner}, its pair to musicians", row.input)
        } else {
            format!("Show {} to musicians", row.input)
        };
        let shown_title = if row.stereo.linked {
            "Both inputs of a pair are shown or hidden together"
        } else {
            HIDDEN_TITLE
        };
        let model = self.model.clone();
        let shown = checkbox(
            id("shown"),
            !row.hidden,
            shown_label,
            if row.hidden { "Hidden" } else { "Shown" },
            Some(shown_title.into()),
            move |shown, _, _| {
                let flow = model.handle(Edit::Shown { index, shown });
                model.console().spawn(flow);
            },
        );
        let hub = self.hub.clone();
        let route = link_target.route;
        let processing = div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .child(link(
                id("processing"),
                "Processing",
                link_target.aria_label.clone(),
                Some(PROCESSING_TITLE.into()),
                move |_, cx| hub.update(cx, |hub, cx| hub.navigate(route, cx)),
            ))
            .when(!link_target.word.is_empty(), |cell| {
                cell.child(
                    div()
                        .text_size(px(text::XS))
                        .text_color(color(colors::TEXT_MUTED))
                        .child(seen(link_target.word)),
                )
            });
        div()
            .id(id("row"))
            .role(Role::Row)
            .flex()
            .items_center()
            .border_b_1()
            .border_color(color(colors::BORDER_SUBTLE))
            .child(
                div()
                    .id(id("input"))
                    .role(Role::RowHeader)
                    .aria_label(row.input.clone())
                    .relative()
                    .child(
                        cell(COLUMNS[0].1)
                            .min_h(px(TARGET + 12.0))
                            .font_family(FONT_MONO)
                            .text_size(px(text::SM))
                            .text_color(color(colors::TEXT_SECONDARY))
                            .child(seen(row.input.clone())),
                    )
                    .when(row.hidden, |input| {
                        // A hidden input's row is marked down its left edge.
                        input.child(
                            div()
                                .absolute()
                                .left_0()
                                .top_0()
                                .bottom_0()
                                .w(px(3.0))
                                .bg(color(colors::TEXT_MUTED)),
                        )
                    }),
            )
            .child(cell(COLUMNS[1].1).child(div().w_full().children(name)))
            .child(cell(COLUMNS[2].1).child(div().w_full().children(kind)))
            .child(
                cell(COLUMNS[3].1).child(
                    div().w_full().child(
                        LevelMeter::new(id("level"), level, format!("{} input level", row.label))
                            .horizontal(),
                    ),
                ),
            )
            .child(cell(COLUMNS[4].1).child(self.stereo(row, window, cx)))
            .child(
                cell(COLUMNS[5].1).child(GroupChip::new(
                    id("group"),
                    row.group
                        .as_ref()
                        .map_or(bandear_desktop_console::values::GroupTone::None, |group| {
                            group.tone
                        }),
                    row.group.as_ref().map(|group| group.name.clone().into()),
                    row.group.as_ref().is_some_and(|group| group.invalid),
                )),
            )
            .child(cell(COLUMNS[6].1).child(shown))
            .child(cell(COLUMNS[7].1).child(processing))
            .into_any_element()
    }

    /// The stereo cell: the Stereo toggle, the words, and why it cannot pair now.
    fn stereo(&self, row: &ChannelRow, _: &mut Window, _: &mut Context<Self>) -> AnyElement {
        let index = row.index;
        let choice = &row.stereo;
        let unavailable = !choice.available;
        let title: SharedString = match (&choice.reason, choice.linked) {
            (Some(reason), _) => reason.clone().into(),
            (None, true) => "Undo the pair: two strips again".into(),
            (None, false) => "Pair with the next input: one strip whose pan is a balance".into(),
        };
        let linked = choice.linked;
        let model = self.model.clone();
        let mut button = div()
            .id(SharedString::from(format!("channels-row-{index}-stereo")))
            .role(Role::Button)
            .aria_label(choice.label.clone())
            .aria_toggled(toggled(linked))
            .focusable()
            .flex()
            .items_center()
            .gap(px(6.0))
            .min_h(px(TARGET))
            .px(px(10.0))
            .rounded(px(radius::MD))
            .border_1()
            .border_color(color(if linked {
                colors::ACCENT
            } else {
                colors::BORDER
            }))
            .bg(color(if linked {
                colors::ACCENT_BG
            } else {
                colors::SURFACE_RAISED
            }))
            .text_size(px(text::SM))
            .font_weight(weight::BOLD)
            .text_color(color(if linked { colors::ACCENT } else { colors::TEXT }))
            .focus_visible(|style| style.border_2().border_color(color(colors::FOCUS)))
            .tooltip(move |window, cx| tooltip(title.clone(), window, cx))
            .child(seen("\u{25ce} Stereo"))
            .on_click(move |_: &ClickEvent, _, _| {
                if unavailable {
                    return;
                }
                let flow = model.handle(Edit::Link {
                    index,
                    linked: !linked,
                });
                model.console().spawn(flow);
            });
        if let Some(reason) = &choice.reason {
            // Unavailable: dashed, still focusable, its reason as its description.
            button = button
                .border_dashed()
                .opacity(0.7)
                .aria_description(reason.clone());
        }
        div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(button)
                    .child(
                        div()
                            .text_size(px(text::SM))
                            .text_color(color(colors::TEXT_MUTED))
                            .child(seen(choice.text.clone())),
                    ),
            )
            .when_some(choice.reason.clone(), |cell, reason| {
                cell.child(
                    div()
                        .max_w(px(22.0 * text::XS))
                        .text_size(px(text::XS))
                        .text_color(color(colors::TEXT_MUTED))
                        .child(seen(reason)),
                )
            })
            .into_any_element()
    }
}
