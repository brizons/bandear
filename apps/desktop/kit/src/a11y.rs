//! Accessibility helpers: headings a navigation focuses, visually hidden text, and live statuses.
//!
//! The web console set ARIA attributes; GPUI exposes the same through AccessKit: `.role()`,
//! `.aria_label()`, `.aria_value()`, `.aria_toggled()` and the numeric values on an element with an
//! id (D8). gpui-pre 0.3.12 has no live-region setting, so a status is given the `Status` role,
//! which screen readers announce politely on platforms that map it.

use gpui::{
    Div, ElementId, FocusHandle, InteractiveElement, IntoElement, ParentElement, Role,
    SharedString, Stateful, StatefulInteractiveElement, Styled, Toggled, div, px,
};

use crate::theme::{FONT_SANS, color, colors, text, weight};

/// A screen's heading: level 1, focused (with no ring) when a navigation lands on the screen. A
/// visually hidden one (the Mix screen's) is still read and still takes the focus.
pub fn heading(
    id: impl Into<ElementId>,
    title: impl Into<SharedString>,
    focus: &FocusHandle,
    hidden: bool,
) -> Stateful<Div> {
    let title = title.into();
    let element = div()
        .id(id)
        .role(Role::Heading)
        .aria_level(1)
        .aria_label(title.clone())
        .track_focus(focus);
    if hidden {
        visually_hidden(element).child(title)
    } else {
        element
            .font_family(FONT_SANS)
            .text_size(px(text::XXL))
            .font_weight(weight::BOLD)
            .line_height(px(text::XXL * text::HEADING_LINE_HEIGHT))
            .text_color(color(colors::TEXT))
            .child(title)
    }
}

/// `element` kept for assistive technology but taking no room on screen (`.ml-visually-hidden`).
pub fn visually_hidden<E: Styled>(element: E) -> E {
    element
        .absolute()
        .w(px(1.0))
        .h(px(1.0))
        .overflow_hidden()
        .opacity(0.0)
}

/// A toggled state for a pressed button or a switch.
pub fn toggled(on: bool) -> Toggled {
    if on { Toggled::True } else { Toggled::False }
}

/// Text that is only seen: assistive technology reads its container's label instead.
pub fn seen(text: impl Into<SharedString>) -> impl IntoElement {
    gpui::Text::new_inaccessible(text.into())
}
