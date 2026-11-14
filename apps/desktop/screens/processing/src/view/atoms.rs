//! The screen's own small controls, built here because the kit has none like them: a checkbox
//! (the parts' On switches, the copy targets), a text link, and the outcome lines.

use bandear_desktop_kit::a11y::{seen, toggled};
use bandear_desktop_kit::gpui::{
    AnyElement, App, ClickEvent, ElementId, InteractiveElement, IntoElement, ParentElement, Role,
    SharedString, StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
};
use bandear_desktop_kit::theme::{FONT_SANS, TARGET, color, colors, radius, text, weight};
use bandear_desktop_kit::tooltip::tooltip;

/// A checkbox: a 16 px box with its word beside it, named `label`, its title on hover.
pub fn checkbox(
    id: impl Into<ElementId>,
    checked: bool,
    label: impl Into<SharedString>,
    word: impl Into<SharedString>,
    title: Option<SharedString>,
    on_toggle: impl Fn(bool, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let word = word.into();
    div()
        .id(id)
        .role(Role::CheckBox)
        .aria_label(label.into())
        .aria_toggled(toggled(checked))
        .focusable()
        .flex()
        .items_center()
        .gap(px(6.0))
        .min_h(px(TARGET))
        .px(px(2.0))
        .rounded(px(radius::XS))
        .cursor_pointer()
        .font_family(FONT_SANS)
        .text_size(px(text::SM))
        .text_color(color(colors::TEXT_SECONDARY))
        .focus_visible(|style| style.border_2().border_color(color(colors::FOCUS)))
        .when_some(title, |element, title| {
            element.tooltip(move |window, cx| tooltip(title.clone(), window, cx))
        })
        .on_click(move |_: &ClickEvent, window, cx| on_toggle(!checked, window, cx))
        .child(
            div()
                .flex_none()
                .size(px(16.0))
                .rounded(px(4.0))
                .border_1()
                .border_color(color(if checked {
                    colors::ACCENT
                } else {
                    colors::BORDER
                }))
                .bg(color(if checked {
                    colors::ACCENT
                } else {
                    colors::SURFACE_SUNKEN
                }))
                .flex()
                .items_center()
                .justify_center()
                .when(checked, |check| {
                    check.child(
                        div()
                            .text_size(px(11.0))
                            .font_weight(weight::BOLD)
                            .text_color(color(colors::TEXT_ON_BRIGHT))
                            .child(seen("\u{2713}")),
                    )
                }),
        )
        .when(!word.is_empty(), |element| element.child(seen(word)))
        .into_any_element()
}

/// A text link, named `label` (its visible text, or longer), its title on hover.
pub fn link(
    id: impl Into<ElementId>,
    visible: impl Into<SharedString>,
    label: impl Into<SharedString>,
    title: Option<SharedString>,
    on_open: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .role(Role::Link)
        .aria_label(label.into())
        .focusable()
        .cursor_pointer()
        .text_color(color(colors::BLUE))
        .underline()
        .rounded(px(radius::XS))
        .focus_visible(|style| style.border_2().border_color(color(colors::FOCUS)))
        .when_some(title, |element, title| {
            element.tooltip(move |window, cx| tooltip(title.clone(), window, cx))
        })
        .on_click(move |_: &ClickEvent, window, cx| on_open(window, cx))
        .child(seen(visible.into()))
        .into_any_element()
}

/// The outcome line (a status, announced politely) or the failure line (an alert): they stay in
/// place, empty, so a new outcome is announced; an empty line takes no room.
pub fn message_line(id: &'static str, alert: bool, message: &str) -> AnyElement {
    let message = SharedString::from(message.to_owned());
    div()
        .id(id)
        .role(if alert { Role::Alert } else { Role::Status })
        .aria_label(message.clone())
        .font_family(FONT_SANS)
        .text_size(px(text::BASE))
        .text_color(color(if alert {
            colors::DANGER_TEXT
        } else {
            colors::GREEN
        }))
        .when(!message.is_empty(), |line| line.child(seen(message)))
        .into_any_element()
}
