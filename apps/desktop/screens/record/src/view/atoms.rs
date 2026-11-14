//! The Record screen's own small pieces (`record-screen.css`, `record-track.js`), which the kit
//! does not have: its card, caps label, hint, badge, an in-text link, the transport's icons and
//! the waveform lane.

use bandear_desktop_console::values::meter_fraction;
use bandear_desktop_kit::a11y::seen;
use bandear_desktop_kit::gpui::{
    AnyElement, App, Bounds, ClickEvent, Div, ElementId, Hsla, InteractiveElement, IntoElement,
    ParentElement, PathBuilder, Pixels, Role, SharedString, Stateful, StatefulInteractiveElement,
    Styled, Window, canvas, div, fill, point, prelude::FluentBuilder, px, size,
};
use bandear_desktop_kit::theme::{color, colors, radius, space, text, weight};
use bandear_desktop_kit::tooltip::tooltip;

use crate::record_view::{EndKind, PeakHistory};
use crate::soundcheck_view::BadgeKind;

/// A card (`.record-card`): bordered, rounded, on the surface.
pub fn card(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Stateful<Div> {
    div()
        .id(id)
        .role(Role::Group)
        .aria_label(label)
        .rounded(px(radius::XL))
        .border_1()
        .border_color(color(colors::BORDER))
        .bg(color(colors::SURFACE))
}

/// A card's title (`h2`), as the takes, soundcheck and settings cards have it.
pub fn title(id: impl Into<ElementId>, words: &'static str) -> impl IntoElement {
    div()
        .id(id)
        .role(Role::Heading)
        .aria_level(2)
        .aria_label(words)
        .text_size(px(text::LG))
        .font_weight(weight::BOLD)
        .child(seen(words))
}

/// A caps label (`.record-label`, a field's label): small, muted, spaced.
pub fn caps(words: &'static str) -> Div {
    div()
        .text_size(px(text::XS))
        .font_weight(weight::MEDIUM)
        .text_color(color(colors::TEXT_MUTED))
        .child(seen(words.to_uppercase()))
}

/// A hint (`.record-hint`): small muted text.
pub fn hint(words: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(text::SM))
        .text_color(color(colors::TEXT_MUTED))
        .child(seen(words.into()))
}

/// A line of the screen's messages (`.mix-message`): a status, or an alert in the danger colors.
pub fn message(id: &'static str, words: impl Into<SharedString>, alert: bool) -> impl IntoElement {
    let words = words.into();
    div()
        .id(id)
        .role(if alert { Role::Alert } else { Role::Status })
        .aria_label(words.clone())
        .py(px(space::S2))
        .px(px(space::S3))
        .rounded(px(radius::MD))
        .border_1()
        .text_size(px(text::MD))
        .map(|line| {
            if alert {
                line.border_color(color(colors::DANGER_BORDER))
                    .bg(color(colors::DANGER_BG))
                    .text_color(color(colors::DANGER_TEXT))
            } else {
                line.border_color(color(colors::BORDER))
                    .bg(color(colors::SURFACE))
                    .text_color(color(colors::TEXT_SECONDARY))
            }
        })
        .child(seen(words))
}

/// The tones of a badge (`.record-badge`): border, background, text.
fn badge_tones(kind: BadgeTone) -> (u32, Option<u32>, u32) {
    match kind {
        BadgeTone::Ok => (colors::OK_BORDER, Some(colors::OK_BG), colors::TEXT),
        BadgeTone::Warn => (colors::WARN_BORDER, Some(colors::WARN_BG), colors::TEXT),
        BadgeTone::Danger => (
            colors::DANGER_BORDER,
            Some(colors::DANGER_BG),
            colors::DANGER_TEXT,
        ),
        BadgeTone::Off => (colors::BORDER, None, colors::TEXT_SECONDARY),
    }
}

/// How a badge looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BadgeTone {
    Ok,
    Warn,
    Danger,
    Off,
}

impl From<EndKind> for BadgeTone {
    fn from(kind: EndKind) -> Self {
        match kind {
            EndKind::Ok => Self::Ok,
            EndKind::Warn => Self::Warn,
            EndKind::Error | EndKind::Live => Self::Danger,
        }
    }
}

impl From<BadgeKind> for BadgeTone {
    fn from(kind: BadgeKind) -> Self {
        match kind {
            BadgeKind::Ok => Self::Ok,
            BadgeKind::Warn => Self::Warn,
            BadgeKind::Error => Self::Danger,
            BadgeKind::Off => Self::Off,
        }
    }
}

/// A badge: a pill with a word; `detail`, when given, is its tooltip and is read after the word.
pub fn badge(
    id: impl Into<ElementId>,
    words: impl Into<SharedString>,
    tone: BadgeTone,
    detail: Option<SharedString>,
) -> impl IntoElement {
    let words = words.into();
    let (border, background, foreground) = badge_tones(tone);
    div()
        .id(id)
        .role(Role::Label)
        .aria_label(words.clone())
        .when_some(detail.clone(), |badge, detail| {
            badge.aria_description(detail)
        })
        .when_some(detail, |badge, detail| {
            badge.tooltip(move |window, cx| tooltip(detail.clone(), window, cx))
        })
        .flex_none()
        .px(px(8.0))
        .py(px(1.0))
        .rounded(px(radius::PILL))
        .border_1()
        .border_color(color(border))
        .when_some(background, |badge, background| badge.bg(color(background)))
        .text_color(color(foreground))
        .text_size(px(text::SM))
        .whitespace_nowrap()
        .child(seen(words))
}

/// A link inside a sentence ("Mix" in the empty tracks text).
pub fn link(
    id: &'static str,
    words: &'static str,
    on_open: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .role(Role::Link)
        .aria_label(words)
        .focusable()
        .tab_stop(true)
        .text_color(color(colors::ACCENT))
        .underline()
        .cursor_pointer()
        .focus_visible(|style| style.border_1().border_color(color(colors::FOCUS)))
        .on_click(move |_: &ClickEvent, window, cx| on_open(window, cx))
        .child(seen(words))
}

/// The transport's icons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    /// A bar and a triangle pointing left.
    Rewind,
    Play,
    Pause,
    /// A rounded square.
    Stop,
}

/// A filled triangle in `bounds`, pointing right, or left with `left`.
fn paint_triangle(bounds: Bounds<Pixels>, left: bool, tone: Hsla, window: &mut Window) {
    let (x, y) = (bounds.origin.x, bounds.origin.y);
    let (width, height) = (bounds.size.width, bounds.size.height);
    let mut path = PathBuilder::fill();
    if left {
        path.move_to(point(x + width, y));
        path.line_to(point(x + width, y + height));
        path.line_to(point(x, y + height / 2.0));
    } else {
        path.move_to(point(x, y));
        path.line_to(point(x, y + height));
        path.line_to(point(x + width, y + height / 2.0));
    }
    path.close();
    if let Ok(path) = path.build() {
        window.paint_path(path, tone);
    }
}

/// A transport icon, 18 px, in `tone` (decorative: its button says what it does).
pub fn icon(kind: Icon, tone: Hsla) -> AnyElement {
    match kind {
        Icon::Stop => div()
            .size(px(14.0))
            .rounded(px(2.0))
            .bg(tone)
            .into_any_element(),
        Icon::Pause => div()
            .flex()
            .gap(px(4.0))
            .child(div().w(px(4.0)).h(px(16.0)).bg(tone))
            .child(div().w(px(4.0)).h(px(16.0)).bg(tone))
            .into_any_element(),
        Icon::Play => canvas(
            |_, _, _| {},
            move |bounds, (), window, _| paint_triangle(bounds, false, tone, window),
        )
        .w(px(13.0))
        .h(px(16.0))
        .into_any_element(),
        Icon::Rewind => div()
            .flex()
            .items_center()
            .gap(px(1.0))
            .child(div().w(px(2.0)).h(px(14.0)).bg(tone))
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, (), window, _| paint_triangle(bounds, true, tone, window),
                )
                .w(px(11.0))
                .h(px(14.0)),
            )
            .into_any_element(),
    }
}

/// A 44 px icon button of the transport (`.record-icon`), named `label` (also its tooltip).
pub fn icon_button(
    id: &'static str,
    kind: Icon,
    label: impl Into<SharedString>,
    disabled: bool,
    on_press: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let label = label.into();
    let tip = label.clone();
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(label)
        .focusable()
        .tab_stop(!disabled)
        .tooltip(move |window, cx| tooltip(tip.clone(), window, cx))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(44.0))
        .rounded(px(radius::MD))
        .border_1()
        .border_color(color(colors::BORDER))
        .bg(color(colors::SURFACE_RAISED))
        .focus_visible(|style| style.border_2().border_color(color(colors::FOCUS)))
        .when(disabled, |button| button.opacity(0.5))
        .when(!disabled, |button| {
            button
                .cursor_pointer()
                .hover(|style| style.border_color(color(colors::TEXT_MUTED)))
        })
        .on_click(move |_: &ClickEvent, window, cx| {
            if !disabled {
                on_press(window, cx);
            }
        })
        .child(icon(kind, color(colors::TEXT)))
}

/// A recording dot (`.record-dot`), `diameter` px.
pub fn dot(diameter: f32, tone: u32) -> impl IntoElement {
    div()
        .flex_none()
        .size(px(diameter))
        .rounded_full()
        .bg(color(tone))
}

/// The waveform lane of a track while a take records: mirrored bars, newest at the right, one
/// slot per level the history can keep, in `tone` at 80 %.
pub fn lane(history: Option<PeakHistory>, tone: u32) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, (), window, _| {
            let Some(history) = history else {
                return;
            };
            let width = f32::from(bounds.size.width);
            let height = f32::from(bounds.size.height);
            if width <= 0.0 || height <= 0.0 {
                return;
            }
            let middle = height / 2.0;
            let step = width / history.capacity() as f32;
            let start = width - history.len() as f32 * step;
            let tone = color(tone).opacity(0.8);
            for (index, level) in history.iter().enumerate() {
                let half = (meter_fraction(f64::from(level)) as f32 * (middle - 1.0)).max(0.5);
                let origin = point(
                    bounds.origin.x + px(start + index as f32 * step),
                    bounds.origin.y + px(middle - half),
                );
                window.paint_quad(fill(
                    Bounds::new(origin, size(px(step.max(1.0)), px(half * 2.0))),
                    tone,
                ));
            }
        },
    )
    .absolute()
    .left_0()
    .right_0()
    .top(px(4.0))
    .bottom(px(4.0))
}
