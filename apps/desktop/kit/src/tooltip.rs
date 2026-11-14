//! The tooltip of a link or a rail icon: its accessible name, shown on hover.

use gpui::{
    AnyView, App, AppContext as _, Context, IntoElement, ParentElement, Render, SharedString,
    Styled, Window, div, px,
};

use crate::theme::{FONT_SANS, color, colors, radius, text};

/// A tooltip's view.
pub struct Tooltip {
    text: SharedString,
}

impl Render for Tooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .max_w(px(360.0))
            .px(px(8.0))
            .py(px(4.0))
            .rounded(px(radius::SM))
            .border_1()
            .border_color(color(colors::BORDER))
            .bg(color(colors::SURFACE_RAISED))
            .text_color(color(colors::TEXT))
            .font_family(FONT_SANS)
            .text_size(px(text::SM))
            .child(crate::a11y::seen(self.text.clone()))
    }
}

/// A tooltip saying `text`, for `.tooltip(...)`.
pub fn tooltip(text: SharedString, _: &mut Window, cx: &mut App) -> AnyView {
    cx.new(|_| Tooltip { text }).into()
}
