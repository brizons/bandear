//! The screen's own small controls, built here because the kit has none like them: a checkbox
//! with its word, a text link, a button whose focus the screen moves, and the outcome lines.

use bandear_desktop_kit::a11y::{seen, toggled};
use bandear_desktop_kit::gpui::{
    AnyElement, App, ClickEvent, ElementId, FocusHandle, InteractiveElement, IntoElement,
    ParentElement, Role, SharedString, StatefulInteractiveElement, Styled, Window, div,
    prelude::FluentBuilder, px,
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

/// How a [focus_button] looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    Secondary,
    Danger,
}

/// A button whose focus the screen moves (Edit, Done, Keep), as the kit's button looks; `disabled`
/// keeps it focusable and announced as such (`aria-disabled`) while a press does nothing.
pub fn focus_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    name: impl Into<SharedString>,
    look: Look,
    focus: &FocusHandle,
    disabled: bool,
    on_press: impl Fn(&mut Window, &mut App) + 'static,
) -> bandear_desktop_kit::gpui::Stateful<bandear_desktop_kit::gpui::Div> {
    let (background, border, foreground) = match look {
        Look::Secondary => (colors::SURFACE_RAISED, colors::BORDER, colors::TEXT),
        Look::Danger => (
            colors::DANGER_BG,
            colors::DANGER_BORDER,
            colors::DANGER_TEXT,
        ),
    };
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(name.into())
        .track_focus(focus)
        .flex()
        .items_center()
        .justify_center()
        .min_h(px(TARGET))
        .px(px(12.0))
        .rounded(px(radius::MD))
        .border_1()
        .border_color(color(border))
        .bg(color(background))
        .text_color(color(foreground))
        .font_family(FONT_SANS)
        .text_size(px(text::SM))
        .font_weight(weight::BOLD)
        .whitespace_nowrap()
        .focus_visible(|style| style.border_2().border_color(color(colors::FOCUS)))
        .when(disabled, |button| button.opacity(0.5))
        .when(!disabled, |button| button.cursor_pointer())
        .on_click(move |_: &ClickEvent, window, cx| {
            if !disabled {
                on_press(window, cx);
            }
        })
        .child(seen(label.into()))
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
