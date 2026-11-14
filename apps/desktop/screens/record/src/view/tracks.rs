//! The tracks card (`record-track.js`): one row per recorded unit of the running device and the
//! local mix, each with its arm button, its name and source, a level meter and the waveform lane
//! (or a note saying what arming does). The rows keep 640 px and the card scrolls sideways in a
//! narrow window.

use bandear_desktop_console::state::ConsoleState;
use bandear_desktop_kit::a11y::{seen, toggled};
use bandear_desktop_kit::gpui::{
    ClickEvent, Context, InteractiveElement, IntoElement, ParentElement, Role, SharedString,
    StatefulInteractiveElement, Styled, div, prelude::FluentBuilder, px,
};
use bandear_desktop_kit::meters::LevelMeter;
use bandear_desktop_kit::theme::{FONT_MONO, TARGET, color, colors, space, text, weight};
use bandear_desktop_kit::tooltip::tooltip;

use super::RecordScreen;
use super::atoms::{self, caps};
use crate::model::Lane;
use crate::record_view::{TrackRow, shown_rows, track_level};

/// What a lane says when it does not draw: by lane and by row kind (an input, the local mix).
fn lane_note(lane: Lane, local: bool) -> &'static str {
    match (lane, local) {
        (Lane::Off, false) => "Not armed: this input is not being recorded",
        (Lane::Off, true) => "Not recorded: arm it to record the local mix as a stereo file",
        (Lane::Armed, false) => "Armed: records with the next take",
        (Lane::Armed, true) => "Armed: the local mix records with the next take",
        (Lane::Recording, _) => "",
    }
}

impl RecordScreen {
    /// One track row.
    fn track_row(
        &self,
        row: &TrackRow,
        level: f64,
        lane: Lane,
        locked: bool,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let key = row.key.clone();
        let label: SharedString = if row.local {
            "Record the local mix".into()
        } else {
            format!("Arm {} ({})", row.name, row.source).into()
        };
        let tip: SharedString = if locked {
            "Arming stays as it is while a take records".into()
        } else if row.armed {
            "Disarm".into()
        } else {
            "Arm".into()
        };
        let armed = row.armed;
        let arm = div()
            .id(SharedString::from(format!("record-arm-{key}")))
            .role(Role::Button)
            .aria_label(label)
            .aria_toggled(toggled(armed))
            .focusable()
            .tab_stop(!locked)
            .tooltip(move |window, cx| tooltip(tip.clone(), window, cx))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(TARGET))
            .rounded_full()
            .border_1()
            .font_family(FONT_MONO)
            .text_size(px(text::XS))
            .font_weight(weight::BOLD)
            .map(|button| {
                if armed {
                    button
                        .border_color(color(colors::RED))
                        .bg(color(colors::RED))
                        .text_color(color(colors::TEXT_ON_BRIGHT))
                } else {
                    button
                        .border_color(color(colors::BORDER))
                        .bg(color(colors::SURFACE_RAISED))
                        .text_color(color(colors::TEXT_MUTED))
                }
            })
            .focus_visible(|style| style.border_2().border_color(color(colors::FOCUS)))
            .when(locked, |button| button.opacity(0.7))
            .when(!locked, |button| button.cursor_pointer())
            .on_click({
                let press = self.press(cx, move |model| model.arm(&key));
                move |_: &ClickEvent, window, cx| {
                    if !locked {
                        press(window, cx);
                    }
                }
            })
            .child(seen("R"));
        let note = lane_note(lane, row.local);
        let tone = if row.local {
            colors::ACCENT
        } else {
            colors::GROUP_TEAL_TEXT
        };
        div()
            .flex()
            .min_h(px(48.0))
            .border_t_1()
            .border_color(color(colors::BORDER_SUBTLE))
            .when(row.local, |line| line.bg(color(colors::SURFACE_SUNKEN)))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .w(px(260.0))
                    .items_center()
                    .gap(px(10.0))
                    .py(px(6.0))
                    .px(px(14.0))
                    .border_r_1()
                    .border_color(color(colors::BORDER_SUBTLE))
                    .child(arm)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(text::MD))
                                    .font_weight(weight::BOLD)
                                    .truncate()
                                    .child(seen(row.name.clone())),
                            )
                            .child(
                                div()
                                    .font_family(FONT_MONO)
                                    .text_size(px(text::XS))
                                    .text_color(color(colors::TEXT_MUTED))
                                    .child(seen(row.source.clone())),
                            ),
                    )
                    .child(
                        div().flex_none().w(px(56.0)).child(
                            LevelMeter::new(
                                SharedString::from(format!("record-meter-{}", row.key)),
                                level,
                                format!("{} level", row.name),
                            )
                            .horizontal(),
                        ),
                    ),
            )
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .when(lane == Lane::Recording, |lane_box| {
                        lane_box.child(atoms::lane(self.model.history(&row.key), tone))
                    })
                    .when(!note.is_empty(), |lane_box| {
                        lane_box.child(
                            div()
                                .px(px(space::S3))
                                .text_size(px(text::SM))
                                .text_color(color(colors::TEXT_MUTED))
                                .child(seen(note)),
                        )
                    }),
            )
    }

    /// The tracks card.
    pub(super) fn tracks(&self, state: &ConsoleState, cx: &Context<Self>) -> impl IntoElement {
        let rows = shown_rows(state);
        let meters = self.model.console().meters();
        let locked = state.is_recording_now();
        let lines: Vec<_> = rows
            .iter()
            .map(|row| {
                let level = meters
                    .as_deref()
                    .map_or(0.0, |meters| track_level(row, meters));
                let lane = self.model.lane_of(row, state);
                self.track_row(row, level, lane, locked, cx)
                    .into_any_element()
            })
            .collect();
        let empty = lines.is_empty();
        atoms::card("record-tracks", "Tracks")
            .overflow_x_scroll()
            .child(div().py(px(10.0)).px(px(14.0)).child(caps("Tracks")))
            .when(empty, |tracks| {
                tracks.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .py(px(space::S3))
                        .px(px(space::S4))
                        .border_t_1()
                        .border_color(color(colors::BORDER_SUBTLE))
                        .text_size(px(text::MD))
                        .text_color(color(colors::TEXT_SECONDARY))
                        .child(seen(
                            "The tracks are the running device's inputs: start the engine from \
                             the\u{a0}",
                        ))
                        .child(atoms::link(
                            "record-mix-link",
                            "Mix",
                            self.open(bandear_desktop_console::router::Section::Mix),
                        ))
                        .child(seen("\u{a0}screen to see and arm them.")),
                )
            })
            .when(!empty, |tracks| {
                tracks.child(div().min_w(px(640.0)).children(lines))
            })
    }
}
