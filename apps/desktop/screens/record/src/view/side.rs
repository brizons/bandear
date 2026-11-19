//! The aside beside the takes (`aria-label="Soundcheck and recording settings"`): the virtual
//! soundcheck card and the recording settings card.

use bandear_desktop_console::state::ConsoleState;
use bandear_desktop_kit::a11y::seen;
use bandear_desktop_kit::buttons::{Button, ButtonKind};
use bandear_desktop_kit::forms::{Choice, select, set_choices, text_input};
use bandear_desktop_kit::gpui::{
    Context, Focusable, InteractiveElement, IntoElement, ParentElement, Role, SharedString,
    StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
};
use bandear_desktop_kit::gpui_component::{Disableable, checkbox::Checkbox};
use bandear_desktop_kit::slider::parameter;
use bandear_desktop_kit::theme::{FONT_MONO, color, colors, radius, space, text, weight};

use super::RecordScreen;
use super::atoms::{self, BadgeTone, badge, caps, hint};
use crate::record_view::settings_view;
use crate::recording::SettingsChange;
use crate::soundcheck_view::soundcheck_view;

/// The soundcheck card's explanation, always shown.
const SOUNDCHECK_HINT: &str = "Each track plays into the input it was recorded from, in place of \
                               the interface, so every mix, the local output and the phones can \
                               be set with nobody playing. The interface stays open: going back \
                               to the live inputs is instant.";

/// The settings card's explanation, always shown.
const SETTINGS_HINT: &str = "Tracks are written before any processing, and a linked pair stays \
                             one stereo file. A file that passes 4 GB switches to RF64 on its \
                             own.";

/// A card of the aside: a column 10 px apart.
fn side_card(
    id: &'static str,
    label: &'static str,
) -> bandear_desktop_kit::gpui::Stateful<bandear_desktop_kit::gpui::Div> {
    atoms::card(id, label)
        .flex()
        .flex_col()
        .gap(px(10.0))
        .py(px(14.0))
        .px(px(space::S4))
}

impl RecordScreen {
    /// The virtual soundcheck card.
    fn soundcheck_card(
        &mut self,
        state: &ConsoleState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let local = self.model.local();
        let view = soundcheck_view(
            state,
            local.chosen_take.as_deref(),
            local.drag,
            local.loop_next,
        );
        let busy = local.busy.is_some();

        // The take select offers the playable takes and shows the one Play would start.
        let offered: Vec<String> = view
            .choices
            .iter()
            .map(|take| take.folder.clone())
            .collect();
        if offered != self.takes_offered {
            let choices = view
                .choices
                .iter()
                .map(|take| Choice::new(take.folder.clone(), take.label.clone()))
                .collect();
            set_choices(&self.takes, choices, view.chosen.as_deref(), window, cx);
            self.takes_offered = offered;
        } else if let Some(chosen) = view.chosen.as_deref()
            && self
                .takes
                .read(cx)
                .selected_value()
                .map(|value| value.as_ref())
                != Some(chosen)
        {
            let chosen = SharedString::from(chosen.to_owned());
            self.takes.update(cx, |takes, cx| {
                takes.set_selected_value(&chosen, window, cx)
            });
        }

        let start = {
            let chosen = view.chosen.clone();
            Button::new("record-sc-start", "Start soundcheck", ButtonKind::Secondary)
                .disabled(busy || !view.can_start)
                .on_press(self.press(cx, move |model| match chosen.clone() {
                    Some(folder) => model.soundcheck_take(folder),
                    None => Box::pin(async { None }),
                }))
        };
        let off = div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(text::SM))
                    .text_color(color(colors::TEXT_SECONDARY))
                    .child(seen("Play back as the live inputs")),
            )
            .child(
                select(&self.takes, "Play back as the live inputs")
                    .disabled(view.choices.is_empty()),
            )
            .child(div().flex().child(start))
            .when_some(view.start_hint, |off, why| off.child(hint(why)));

        let tenths = (view.slider.max * 10.0).round().max(1.0) as i32;
        let seek = parameter(
            "record-sc-seek",
            tenths,
            view.slider.value * 10.0,
            0,
            "Position in the take",
            format!("{} of {}", view.position, view.length),
        )
        .disabled(view.slider.disabled)
        .on_input({
            let model = self.model.clone();
            let screen = cx.entity().downgrade();
            move |tenths, _, cx| {
                model.drag(f64::from(tenths) / 10.0);
                drop(screen.update(cx, |_, cx| cx.notify()));
            }
        })
        .on_change({
            let model = self.model.clone();
            let screen = cx.entity().downgrade();
            move |tenths, _, cx| {
                model.spawn(model.seek_to(f64::from(tenths) / 10.0));
                drop(screen.update(cx, |_, cx| cx.notify()));
            }
        });
        let pause_word = if view.can_pause {
            "Pause"
        } else {
            view.resume_word
        };
        let on = div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .justify_between()
                    .gap(px(space::S2))
                    .text_size(px(text::MD))
                    .child(
                        div()
                            .min_w_0()
                            .font_weight(weight::BOLD)
                            .truncate()
                            .child(seen(view.take_name.clone().unwrap_or_default())),
                    )
                    .child(
                        div()
                            .font_family(FONT_MONO)
                            .child(seen(format!("{} / {}", view.position, view.length))),
                    ),
            )
            .child(seek)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(6.0))
                    .child(
                        Button::new("record-sc-pause", pause_word, ButtonKind::Secondary)
                            .disabled(busy || !(view.can_pause || view.can_resume))
                            .on_press(self.press(cx, |model| model.play())),
                    )
                    .child(
                        Button::new(
                            "record-sc-stop",
                            "Back to live inputs",
                            ButtonKind::Secondary,
                        )
                        .disabled(busy || !view.can_stop)
                        .on_press(self.press(cx, |model| model.stop())),
                    ),
            )
            .child(hint(view.clock))
            .child(hint(view.underruns.clone()))
            .when_some(view.failure.clone(), |on, failure| {
                on.child(atoms::message("record-sc-failure", failure, true))
            });

        let looping = {
            let model = self.model.clone();
            let screen = cx.entity().downgrade();
            Checkbox::new("record-sc-loop")
                .label("Loop the take")
                .checked(view.looping)
                .disabled(!view.can_loop)
                .on_click(move |checked, _, cx| {
                    model.spawn(model.set_loop(*checked));
                    drop(screen.update(cx, |_, cx| cx.notify()));
                })
        };

        side_card("record-soundcheck", "Virtual soundcheck")
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(space::S2))
                    .child(atoms::title(
                        "record-soundcheck-title",
                        "Virtual soundcheck",
                    ))
                    .child(badge(
                        "record-sc-badge",
                        view.badge.clone(),
                        BadgeTone::from(view.badge_kind),
                        None,
                    )),
            )
            .map(|card| {
                if view.on {
                    card.child(on)
                } else {
                    card.child(off)
                }
            })
            .child(looping)
            .child(hint(SOUNDCHECK_HINT))
    }

    /// The recording settings card.
    fn settings_card(
        &mut self,
        state: &ConsoleState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let view = settings_view(state);
        let off = !view.known || view.locked;

        // The field shows the desk's folder unless the engineer types in it; a refused folder
        // stays typed.
        let typing = self.folder.read(cx).focus_handle(cx).is_focused(window);
        if let Some(folder) = self.model.folder_to_show(state, typing) {
            self.folder
                .update(cx, |field, cx| field.set_value(folder, window, cx));
        }
        if self
            .format
            .read(cx)
            .selected_value()
            .map(|value| value.as_ref())
            != Some(view.format.as_str())
        {
            let format = SharedString::from(view.format.clone());
            self.format.update(cx, |select, cx| {
                select.set_selected_value(&format, window, cx)
            });
        }

        let use_folder = {
            let model = self.model.clone();
            let field = self.folder.clone();
            let screen = cx.entity().downgrade();
            Button::new(
                "record-folder-use",
                "Use this folder",
                ButtonKind::Secondary,
            )
            .disabled(off)
            .on_press(move |_, cx| {
                let text = field.read(cx).value().to_string();
                model.spawn(model.use_folder(text));
                drop(screen.update(cx, |_, cx| cx.notify()));
            })
        };
        let by_default = Button::new(
            "record-folder-default",
            "Default folder",
            ButtonKind::Secondary,
        )
        .disabled(off || view.is_default)
        .on_press(self.press(cx, |model| model.use_default_folder()));
        let include_local = {
            let model = self.model.clone();
            let screen = cx.entity().downgrade();
            Checkbox::new("record-include-local")
                .label("Also record the local mix (stereo)")
                .checked(view.include_local)
                .disabled(off)
                .on_click(move |checked, _, cx| {
                    let change = SettingsChange {
                        include_local: Some(*checked),
                        ..SettingsChange::default()
                    };
                    model.spawn(model.change_settings(change));
                    drop(screen.update(cx, |_, cx| cx.notify()));
                })
        };
        let note = if view.is_default {
            "This is the default folder.".to_owned()
        } else {
            format!("The default folder is {}.", view.default_folder)
        };

        side_card("record-settings", "Settings")
            .child(atoms::title("record-settings-title", "Settings"))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(caps("Folder"))
                    .child(
                        div()
                            .font_family(FONT_MONO)
                            .child(text_input(&self.folder, "Folder").disabled(off)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(6.0))
                            .child(use_folder)
                            .child(by_default),
                    ),
            )
            .child(hint(note))
            .when_some(view.notice.clone(), |card, notice| {
                card.child(
                    div()
                        .id("record-notice")
                        .role(Role::Status)
                        .aria_label(notice.clone())
                        .py(px(space::S2))
                        .px(px(10.0))
                        .rounded(px(radius::MD))
                        .border_1()
                        .border_color(color(colors::WARN_BORDER))
                        .bg(color(colors::WARN_BG))
                        .text_size(px(text::MD))
                        .child(seen(notice)),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(caps("Files"))
                    .child(select(&self.format, "Files").disabled(off)),
            )
            .child(include_local)
            .child(hint(SETTINGS_HINT))
    }

    /// The aside: the soundcheck card over the settings card; it drops under the takes in a
    /// narrow window (`flex: 1 1 300px`).
    pub(super) fn side(
        &mut self,
        state: &ConsoleState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .id("record-side")
            .role(Role::Complementary)
            .aria_label("Soundcheck and recording settings")
            .flex()
            .flex_col()
            .flex_grow(1.0)
            .flex_basis(px(300.0))
            .min_w_0()
            .gap(px(space::S4))
            .child(self.soundcheck_card(state, window, cx))
            .child(self.settings_card(state, window, cx))
    }
}
