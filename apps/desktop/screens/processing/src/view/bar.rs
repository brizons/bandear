//! The action bar (`processing-screen.js`): the channel picker with previous and next and the
//! strip's state, A/B, Reset and Copy to…, the presets, and Save as.

use bandear_desktop_kit::a11y::seen;
use bandear_desktop_kit::buttons::{Button, ButtonKind, ToggleButton, ToggleKind};
use bandear_desktop_kit::forms::{select, text_input};
use bandear_desktop_kit::gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement, Role, SharedString,
    StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
};
use bandear_desktop_kit::theme::{color, colors, radius, text, weight};
use bandear_desktop_kit::tooltip::tooltip;

use super::ProcessingScreen;
use super::atoms::checkbox;
use crate::model::EditorView;

/// `element` with `title` shown on hover, as a web control's title.
fn titled(
    id: impl Into<SharedString>,
    title: &'static str,
    element: impl IntoElement,
) -> AnyElement {
    div()
        .id(id.into())
        .tooltip(move |window, cx| tooltip(title.into(), window, cx))
        .child(element)
        .into_any_element()
}

fn group() -> bandear_desktop_kit::gpui::Div {
    div().flex().flex_wrap().items_center().gap(px(8.0))
}

fn caption(words: &'static str) -> impl IntoElement {
    div()
        .text_size(px(text::SM))
        .text_color(color(colors::TEXT_SECONDARY))
        .child(seen(words))
}

impl ProcessingScreen {
    /// The wrapping action bar, and the copy popover under it while open.
    pub(super) fn bar(
        &self,
        view: &EditorView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let at = view
            .choices
            .iter()
            .position(|choice| choice.channel == view.channel);
        let first = at.is_none_or(|at| at == 0);
        let last = at.is_none_or(|at| at + 1 == view.choices.len());
        let screen = cx.entity().downgrade();
        let step = |delta: i32| {
            let screen = screen.clone();
            move |_: &mut Window, cx: &mut bandear_desktop_kit::gpui::App| {
                drop(screen.update(cx, |this, cx| {
                    let next = this.model.step(delta);
                    this.opened(next, cx);
                }));
            }
        };
        let (state_word, background, border, foreground) = if view.bypassed {
            (
                "Bypassed",
                colors::WARN_BG,
                colors::WARN_BORDER,
                colors::ACCENT,
            )
        } else if view.active {
            (
                "Processing on",
                colors::OK_BG,
                colors::OK_BORDER,
                colors::GREEN,
            )
        } else {
            (
                "No processing",
                colors::SURFACE_RAISED,
                colors::BORDER,
                colors::TEXT_MUTED,
            )
        };
        let picker = group()
            .child(titled(
                "processing-prev-title",
                "Previous channel",
                Button::new("processing-prev", "\u{2039}", ButtonKind::Secondary)
                    .description("Previous channel")
                    .disabled(first)
                    .on_press(step(-1)),
            ))
            .child(caption("Channel"))
            .child(div().w(px(200.0)).child(select(&self.channel, "Channel")))
            .child(titled(
                "processing-next-title",
                "Next channel",
                Button::new("processing-next", "\u{203a}", ButtonKind::Secondary)
                    .description("Next channel")
                    .disabled(last)
                    .on_press(step(1)),
            ))
            .child(
                div()
                    .px(px(10.0))
                    .py(px(4.0))
                    .rounded(px(radius::PILL))
                    .border_1()
                    .border_color(color(border))
                    .bg(color(background))
                    .text_size(px(text::XS))
                    .font_weight(weight::BOLD)
                    .text_color(color(foreground))
                    .child(state_word),
            );

        let model = self.model.clone();
        let bypass = ToggleButton::new(
            "processing-bypass",
            "Bypass (A/B)",
            "Bypass (A/B)",
            ToggleKind::Plain,
            view.bypassed,
        )
        .wide()
        .disabled(!view.can_bypass)
        .on_toggle(move |_, _, _| {
            let flow = model.toggle_bypass();
            model.console().spawn(flow);
        });
        let model = self.model.clone();
        let reset = Button::new("processing-reset", "Reset", ButtonKind::Secondary)
            .disabled(view.bypassed)
            .on_press(move |_, _| {
                let flow = model.reset();
                model.console().spawn(flow);
                model.request_curve();
            });
        let copy_open = self.copy_open;
        let toggle_copy = {
            let screen = screen.clone();
            move |_: &mut Window, cx: &mut bandear_desktop_kit::gpui::App| {
                drop(screen.update(cx, |this, cx| {
                    this.copy_open = !this.copy_open;
                    cx.notify();
                }));
            }
        };
        let actions = group()
            .child(titled(
                "processing-bypass-title",
                "Hear the channel with no processing; press again to put it back",
                bypass,
            ))
            .child(titled(
                "processing-reset-title",
                "Take every part of this strip off",
                reset,
            ))
            .child(
                div()
                    .id("processing-copy-toggle")
                    .aria_expanded(copy_open)
                    .child(
                        Button::new(
                            "processing-copy-open",
                            "Copy to\u{2026}",
                            ButtonKind::Secondary,
                        )
                        .on_press(toggle_copy),
                    ),
            );

        let chosen = self
            .chosen_preset
            .as_ref()
            .and_then(|name| view.presets.iter().find(|preset| &preset.name == name))
            .cloned();
        let model = self.model.clone();
        let apply_name = chosen.as_ref().map(|preset| preset.name.clone());
        let apply = Button::new("processing-apply", "Apply", ButtonKind::Secondary)
            .disabled(view.bypassed || chosen.is_none())
            .on_press(move |_, _| {
                if let Some(name) = &apply_name {
                    let flow = model.apply_preset(name.clone());
                    model.console().spawn(flow);
                    model.request_curve();
                }
            });
        let model = self.model.clone();
        let factory = chosen.as_ref().is_some_and(|preset| preset.factory);
        let delete_name = chosen.as_ref().map(|preset| preset.name.clone());
        let delete = Button::new("processing-delete", "Delete", ButtonKind::Secondary)
            .disabled(chosen.is_none() || factory)
            .on_press(move |_, _| {
                if let Some(name) = &delete_name {
                    let flow = model.delete_preset(name.clone());
                    model.console().spawn(flow);
                }
            });
        let presets = group()
            .child(caption("Preset"))
            .child(div().w(px(200.0)).child(select(&self.preset, "Preset")))
            .child(apply)
            .child(titled(
                "processing-delete-title",
                if factory {
                    "Built-in presets cannot be deleted"
                } else {
                    "Delete this preset of yours"
                },
                delete,
            ));

        let save = {
            let screen = screen.clone();
            Button::new("processing-save", "Save", ButtonKind::Secondary).on_press(
                move |window, cx| {
                    drop(screen.update(cx, |this, cx| this.save_preset(window, cx)));
                },
            )
        };
        let save_as = group()
            .child(caption("Save as"))
            .child(
                div()
                    .w(px(180.0))
                    .child(text_input(&self.save_name, "Save as")),
            )
            .child(save);

        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .id("processing-bar")
                    .role(Role::Toolbar)
                    .aria_label("Strip actions")
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(16.0))
                    .p(px(12.0))
                    .rounded(px(radius::LG))
                    .border_1()
                    .border_color(color(colors::BORDER))
                    .bg(color(colors::SURFACE))
                    .child(picker)
                    .child(actions)
                    .child(presets)
                    .child(save_as),
            )
            .when(copy_open, |bar| {
                bar.child(self.copy_popover(view, window, cx))
            })
            .into_any_element()
    }

    /// Copy to…: a box per other strip, and Copy; on success it closes and its boxes clear.
    fn copy_popover(
        &self,
        view: &EditorView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let _ = window;
        let screen = cx.entity().downgrade();
        let targets: Vec<AnyElement> = view
            .choices
            .iter()
            .filter(|choice| choice.channel != view.channel)
            .map(|choice| {
                let picked = self.copy_picked.contains(&choice.channel);
                let channel = choice.channel;
                let screen = screen.clone();
                checkbox(
                    SharedString::from(format!("processing-copy-{channel}")),
                    picked,
                    choice.label.clone(),
                    choice.label.clone(),
                    None,
                    move |on, _, cx| {
                        drop(screen.update(cx, |this, cx| {
                            if on {
                                this.copy_picked.insert(channel);
                            } else {
                                this.copy_picked.remove(&channel);
                            }
                            cx.notify();
                        }));
                    },
                )
            })
            .collect();
        let legend = format!("Copy {} to", view.label());
        let copy = Button::new("processing-copy", "Copy", ButtonKind::Primary)
            .disabled(view.bypassed)
            .on_press(move |window, cx| {
                drop(screen.update(cx, |this, cx| {
                    let picked: Vec<usize> = this.copy_picked.iter().copied().collect();
                    let copy = this.model.copy_to(picked);
                    this.run_then(copy, window, cx, |this, said, _, _| {
                        if said.ok {
                            this.copy_open = false;
                            this.copy_picked.clear();
                        }
                    });
                }));
            });
        div()
            .id("processing-copy-popover")
            .role(Role::Group)
            .aria_label(legend.clone())
            .flex()
            .flex_col()
            .gap(px(8.0))
            .max_w(px(420.0))
            .p(px(12.0))
            .rounded(px(radius::MD))
            .border_1()
            .border_color(color(colors::BORDER))
            .bg(color(colors::SURFACE_RAISED))
            .child(
                div()
                    .text_size(px(text::SM))
                    .font_weight(weight::BOLD)
                    .child(seen(legend)),
            )
            .child(div().flex().flex_col().gap(px(2.0)).children(targets))
            .child(div().flex().child(copy))
            .into_any_element()
    }
}
