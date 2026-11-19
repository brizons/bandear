//! The transport card (`section[aria-label="Transport"]`): Back to start, Play/Pause, Stop and
//! Record, the position over the state words, the take's name, Add marker, what a take would
//! record, and below them the disk's warning or problem and the markers.

use bandear_desktop_console::format::SEPARATOR;
use bandear_desktop_console::state::ConsoleState;
use bandear_desktop_kit::a11y::{seen, toggled};
use bandear_desktop_kit::buttons::{Button, ButtonKind};
use bandear_desktop_kit::forms::text_input;
use bandear_desktop_kit::gpui::{
    AnyElement, ClickEvent, Context, InteractiveElement, IntoElement, ParentElement, Role,
    SharedString, StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
};
use bandear_desktop_kit::theme::{FONT_MONO, color, colors, radius, space, text, weight};
use bandear_desktop_kit::tooltip::tooltip;

use super::RecordScreen;
use super::atoms::{self, Icon, caps, icon_button};
use crate::record_view::{Transport, WarningLevel, disk_view, transport_view};
use crate::soundcheck::SOUNDCHECK_BLOCKS_RECORDING;
use crate::soundcheck_view::PlayAction;

impl RecordScreen {
    /// The Record button: a dot and its word, red while a take records. Its tooltip says why it
    /// cannot record (a soundcheck, or the state words).
    fn record_button(&self, view: &Transport, busy: bool, cx: &Context<Self>) -> impl IntoElement {
        let disabled = busy || !view.can_record;
        let recording = view.recording;
        let why: Option<SharedString> = (!view.can_record && !recording).then(|| {
            if view.soundcheck {
                SOUNDCHECK_BLOCKS_RECORDING.into()
            } else {
                view.state_word.clone().into()
            }
        });
        let model = self.model.clone();
        let field = self.take_name.clone();
        let screen = cx.entity().downgrade();
        let word = if recording { "Recording" } else { "Record" };
        div()
            .id("record-record")
            .role(Role::Button)
            .aria_label("Record")
            .aria_toggled(toggled(recording))
            .when_some(why.clone(), |button, why| button.aria_description(why))
            .when_some(why, |button, why| {
                button.tooltip(move |window, cx| tooltip(why.clone(), window, cx))
            })
            .focusable()
            .tab_stop(!disabled)
            .flex()
            .flex_none()
            .items_center()
            .gap(px(space::S2))
            .h(px(44.0))
            .px(px(space::S4))
            .rounded(px(radius::MD))
            .border_1()
            .text_size(px(text::BASE))
            .font_weight(weight::BOLD)
            .map(|button| {
                if recording {
                    button
                        .border_color(color(colors::RED))
                        .bg(color(colors::RED))
                        .text_color(color(colors::TEXT_ON_BRIGHT))
                } else {
                    button
                        .border_color(color(colors::DANGER_BORDER))
                        .bg(color(colors::DANGER_BG))
                        .text_color(color(colors::DANGER_TEXT))
                }
            })
            .focus_visible(|style| style.border_2().border_color(color(colors::FOCUS)))
            .when(disabled && !recording, |button| button.opacity(0.5))
            .when(!disabled, |button| {
                button
                    .cursor_pointer()
                    .hover(|style| style.border_color(color(colors::RED)))
            })
            .on_click(move |_: &ClickEvent, _, cx| {
                if disabled {
                    return;
                }
                let name = field.read(cx).value().to_string();
                model.spawn(model.record(name));
                drop(screen.update(cx, |_, cx| cx.notify()));
            })
            .child(atoms::dot(
                12.0,
                if recording {
                    colors::TEXT_ON_BRIGHT
                } else {
                    colors::RED
                },
            ))
            .child(seen(word))
    }

    /// The disk's warning, its problem and the markers: rows of the card's full width.
    fn transport_rows(&self, state: &ConsoleState, view: &Transport) -> Vec<AnyElement> {
        let disk = disk_view(state);
        let mut rows = Vec::new();
        if let Some(warning) = disk.warning {
            let critical = warning.level == WarningLevel::Critical;
            rows.push(
                div()
                    .id("record-disk-warning")
                    .role(Role::Alert)
                    .aria_label(warning.text.clone())
                    .w_full()
                    .py(px(space::S2))
                    .px(px(space::S3))
                    .rounded(px(radius::MD))
                    .border_1()
                    .text_size(px(text::MD))
                    .map(|line| {
                        if critical {
                            line.border_color(color(colors::DANGER_BORDER))
                                .bg(color(colors::DANGER_BG))
                                .text_color(color(colors::DANGER_TEXT))
                        } else {
                            line.border_color(color(colors::WARN_BORDER))
                                .bg(color(colors::WARN_BG))
                        }
                    })
                    .child(seen(warning.text))
                    .into_any_element(),
            );
        }
        if let Some(problem) = disk.problem {
            rows.push(atoms::hint(problem).w_full().into_any_element());
        }
        if !view.markers.is_empty() {
            let title = match &view.markers_of {
                Some(take) => format!("Markers{SEPARATOR}{take}"),
                None => "Markers".to_owned(),
            };
            let markers = view.markers.iter().map(|marker| {
                div()
                    .id(SharedString::from(format!("record-marker-{}", marker.key)))
                    .role(Role::ListItem)
                    .aria_label(format!("{} {}", marker.time, marker.name))
                    .flex()
                    .gap(px(4.0))
                    .px(px(8.0))
                    .py(px(1.0))
                    .border_l_2()
                    .border_color(color(colors::BLUE))
                    .rounded_r(px(radius::XS))
                    .bg(color(colors::MARKER_BG))
                    .text_color(color(colors::MARKER_TEXT))
                    .text_size(px(text::SM))
                    .font_weight(weight::BOLD)
                    .child(
                        div()
                            .font_family(FONT_MONO)
                            .child(seen(marker.time.clone())),
                    )
                    .child(seen(marker.name.clone()))
            });
            rows.push(
                div()
                    .w_full()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(space::S3))
                    .child(
                        div()
                            .id("record-markers-title")
                            .role(Role::Heading)
                            .aria_level(2)
                            .aria_label(title.clone())
                            .text_size(px(text::XS))
                            .font_weight(weight::MEDIUM)
                            .text_color(color(colors::TEXT_MUTED))
                            .child(seen(title.to_uppercase())),
                    )
                    .child(
                        div()
                            .id("record-markers")
                            .role(Role::List)
                            .flex()
                            .flex_wrap()
                            .gap(px(6.0))
                            .children(markers),
                    )
                    .into_any_element(),
            );
        }
        rows
    }

    /// The transport card.
    pub(super) fn transport(
        &self,
        state: &ConsoleState,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let view = transport_view(state, &self.model.transport_local());
        let busy = self.model.local().busy.is_some();
        let disk = disk_view(state);

        let rewind = icon_button(
            "record-rewind",
            Icon::Rewind,
            "Back to start",
            !view.can_rewind,
            self.press(cx, |model| model.rewind()),
        );
        let play = icon_button(
            "record-play",
            if view.play.action == Some(PlayAction::Pause) {
                Icon::Pause
            } else {
                Icon::Play
            },
            view.play.label.clone(),
            busy || !view.play.can,
            self.press(cx, |model| model.play()),
        );
        let stop = icon_button(
            "record-stop",
            Icon::Stop,
            view.stop_label,
            busy || !view.can_stop,
            self.press(cx, |model| model.stop()),
        );
        let state_tone = match view.state_kind.as_str() {
            "recording" | "failed" => colors::DANGER_TEXT,
            "stopping" => colors::ACCENT,
            _ => colors::TEXT_MUTED,
        };
        let position = div()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .min_w(px(110.0))
            .child(
                div()
                    .id("record-position")
                    .aria_label(format!("Position {}", view.position))
                    .font_family(FONT_MONO)
                    .text_size(px(28.0))
                    .font_weight(weight::BOLD)
                    .line_height(px(31.0))
                    .child(seen(view.position.clone())),
            )
            .child(
                div()
                    .id("record-state")
                    .role(Role::Status)
                    .aria_label(view.state_word.clone())
                    .text_size(px(text::SM))
                    .font_weight(weight::BOLD)
                    .text_color(color(state_tone))
                    .child(seen(view.state_word.clone())),
            );
        let take_field = div()
            .flex()
            .flex_col()
            .gap(px(space::S1))
            .child(caps("Take"))
            .child(
                div()
                    .w(px(200.0))
                    .child(text_input(&self.take_name, "Take").disabled(view.recording)),
            );
        let mark = Button::new("record-mark", "Add marker", ButtonKind::Secondary)
            .disabled(busy || !view.can_mark)
            .on_press(self.press(cx, |model| model.mark()));
        let left = match &disk.left {
            Some(left) => format!("{SEPARATOR}{left}"),
            None => String::new(),
        };
        let summary_row = |term: &'static str, value: AnyElement| {
            div()
                .flex()
                .gap(px(space::S3))
                .child(
                    div()
                        .w(px(46.0))
                        .text_color(color(colors::TEXT_MUTED))
                        .child(seen(term)),
                )
                .child(div().font_weight(weight::MEDIUM).child(value))
        };
        let summary = div()
            .id("record-summary")
            .role(Role::Group)
            .aria_label(format!(
                "Armed {}. Format {}. Disk {}{}",
                view.armed, view.format, disk.free, left
            ))
            .flex()
            .flex_col()
            .gap(px(2.0))
            .text_size(px(text::SM))
            .child(summary_row(
                "Armed",
                seen(view.armed.clone()).into_any_element(),
            ))
            .child(summary_row(
                "Format",
                seen(view.format.clone()).into_any_element(),
            ))
            .child(summary_row(
                "Disk",
                div()
                    .flex()
                    .child(div().font_family(FONT_MONO).child(seen(disk.free.clone())))
                    .child(seen(left))
                    .into_any_element(),
            ));

        atoms::card("record-transport", "Transport")
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(14.0))
            .py(px(space::S3))
            .px(px(space::S4))
            .child(
                div()
                    .flex()
                    .gap(px(6.0))
                    .child(rewind)
                    .child(play)
                    .child(stop)
                    .child(self.record_button(&view, busy, cx)),
            )
            .child(position)
            .child(take_field)
            .child(div().self_end().child(mark))
            .child(div().flex_grow(1.0))
            .child(summary)
            .children(self.transport_rows(state, &view))
    }
}
