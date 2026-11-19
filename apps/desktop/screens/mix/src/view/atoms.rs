//! The screen's own small parts, for what the kit has none of: a message line above the strips,
//! the tools' buttons (they take a focus handle, since the tools move the focus, and say whether
//! their panel is open), a checkbox, and the strips' captions.

use std::rc::Rc;

use bandear_desktop_kit::a11y::{seen, toggled};
use bandear_desktop_kit::gpui::{
    App, ClickEvent, Div, ElementId, FocusHandle, InteractiveElement, IntoElement, ParentElement,
    Role, SharedString, Stateful, StatefulInteractiveElement, Styled, Window, div,
    prelude::FluentBuilder, px,
};
use bandear_desktop_kit::theme::{FONT_SANS, color, colors, radius, text, weight};

/// A sentence above the strips: a status, or an alert in the danger colours.
pub fn message_line(id: impl Into<ElementId>, alert: bool, said: String) -> Stateful<Div> {
    let (border, background, foreground, role) = if alert {
        (
            colors::DANGER_BORDER,
            colors::DANGER_BG,
            colors::DANGER_TEXT,
            Role::Alert,
        )
    } else {
        (
            colors::BORDER,
            colors::SURFACE,
            colors::TEXT_SECONDARY,
            Role::Status,
        )
    };
    let said = SharedString::from(said);
    div()
        .id(id)
        .role(role)
        .aria_label(said.clone())
        .py(px(8.0))
        .px(px(12.0))
        .rounded(px(radius::MD))
        .border_1()
        .border_color(color(border))
        .bg(color(background))
        .text_color(color(foreground))
        .text_size(px(text::MD))
        .child(seen(said))
}

/// How a tools button looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    Plain,
    /// The accent fill (Copy, Apply).
    Primary,
    /// The danger colours (Delete).
    Danger,
}

/// A button of the tools (`mix-tools.js`): 36 px high, a raised surface, an accent border while its
/// panel is open (`expanded`). An `inert` button stays focusable but does nothing, dimmed, as an
/// `aria-disabled` one did.
pub struct ToolButton {
    id: ElementId,
    label: SharedString,
    seen: SharedString,
    look: Look,
    focus: Option<FocusHandle>,
    expanded: Option<bool>,
    inert: bool,
    on_press: Option<Rc<dyn Fn(&mut Window, &mut App)>>,
}

impl ToolButton {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>, look: Look) -> Self {
        let label = label.into();
        Self {
            id: id.into(),
            seen: label.clone(),
            label,
            look,
            focus: None,
            expanded: None,
            inert: false,
            on_press: None,
        }
    }

    /// A name for assistive technology other than the words shown ("Apply Verse").
    pub fn named(mut self, label: impl Into<SharedString>) -> Self {
        self.label = label.into();
        self
    }

    pub fn focus(mut self, focus: &FocusHandle) -> Self {
        self.focus = Some(focus.clone());
        self
    }

    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = Some(expanded);
        self
    }

    pub fn inert(mut self, inert: bool) -> Self {
        self.inert = inert;
        self
    }

    pub fn on_press(mut self, on_press: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_press = Some(Rc::new(on_press));
        self
    }

    pub fn build(self) -> Stateful<Div> {
        let (background, border, foreground, bold) = match self.look {
            Look::Plain => (colors::SURFACE_RAISED, colors::BORDER, colors::TEXT, false),
            Look::Primary => (colors::ACCENT, colors::ACCENT, colors::TEXT_ON_BRIGHT, true),
            Look::Danger => (
                colors::DANGER_BG,
                colors::DANGER_BORDER,
                colors::DANGER_TEXT,
                false,
            ),
        };
        let border = if self.expanded == Some(true) {
            colors::ACCENT
        } else {
            border
        };
        let (inert, on_press) = (self.inert, self.on_press);
        let button = div()
            .id(self.id)
            .role(Role::Button)
            .aria_label(self.label)
            .when_some(self.expanded, |button, expanded| {
                button.aria_expanded(expanded)
            });
        let button = match &self.focus {
            Some(focus) => button.track_focus(focus),
            None => button.focusable(),
        };
        button
            .tab_stop(true)
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .min_h(px(36.0))
            .px(px(10.0))
            .rounded(px(radius::MD))
            .border_1()
            .border_color(color(border))
            .bg(color(background))
            .text_color(color(foreground))
            .font_family(FONT_SANS)
            .text_size(px(text::MD))
            .font_weight(if bold { weight::BOLD } else { weight::MEDIUM })
            .whitespace_nowrap()
            .focus_visible(|style| style.border_2().border_color(color(colors::FOCUS)))
            .when(inert, |button| button.opacity(0.55))
            .when(!inert, |button| {
                button
                    .cursor_pointer()
                    .hover(|style| style.border_color(color(colors::TEXT_MUTED)))
            })
            .on_click(move |_: &ClickEvent, window, cx| {
                if inert {
                    return;
                }
                if let Some(on_press) = &on_press {
                    on_press(window, cx);
                }
            })
            .child(seen(self.seen))
    }
}

/// A checkbox: its box and words, a reason beside a disabled one ("(follows Bass)"). To assistive
/// technology a `CheckBox` named by its words.
pub fn checkbox(
    id: impl Into<ElementId>,
    label: &str,
    note: Option<&str>,
    checked: bool,
    disabled: bool,
    focus: Option<&FocusHandle>,
    on_toggle: impl Fn(bool, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let name = match note {
        Some(note) => format!("{label} ({note})"),
        None => label.to_owned(),
    };
    let row = div()
        .id(id)
        .role(Role::CheckBox)
        .aria_label(SharedString::from(name))
        .aria_toggled(toggled(checked));
    let row = match focus {
        Some(focus) => row.track_focus(&focus.clone().tab_stop(!disabled)),
        None => row.focusable().tab_stop(!disabled),
    };
    row.flex()
        .items_center()
        .gap(px(8.0))
        .min_h(px(28.0))
        .px(px(2.0))
        .rounded(px(radius::XS))
        .text_color(color(colors::TEXT))
        .text_size(px(text::BASE))
        .focus_visible(|style| style.border_2().border_color(color(colors::FOCUS)))
        .when(disabled, |row| row.opacity(0.55))
        .when(!disabled, |row| {
            row.cursor_pointer()
                .on_click(move |_: &ClickEvent, window, cx| on_toggle(!checked, window, cx))
        })
        .child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .size(px(16.0))
                .rounded(px(radius::XS))
                .border_1()
                .border_color(color(if checked {
                    colors::ACCENT
                } else {
                    colors::BORDER
                }))
                .bg(color(if checked {
                    colors::ACCENT
                } else {
                    colors::GROUND
                }))
                .text_color(color(colors::TEXT_ON_BRIGHT))
                .text_size(px(text::XS))
                .font_weight(weight::BOLD)
                .when(checked, |mark| mark.child(seen("\u{2713}"))),
        )
        .child(seen(label.to_owned()))
        .when_some(note, |row, note| {
            row.child(
                div()
                    .text_color(color(colors::TEXT_MUTED))
                    .text_size(px(text::SM))
                    .child(seen(format!("({note})"))),
            )
        })
}

/// A strip's caption ("MASTER", "GROUP", "LEVEL"): only seen, as `aria-hidden` had it.
pub fn caption(words: &'static str) -> Div {
    div()
        .text_color(color(colors::TEXT_MUTED))
        .text_size(px(text::XXS))
        .font_weight(weight::BOLD)
        .child(seen(words))
}

/// A muted hint line of the tools' panels.
pub fn hint(words: impl Into<SharedString>) -> Div {
    div()
        .text_color(color(colors::TEXT_MUTED))
        .text_size(px(text::SM))
        .child(seen(words.into()))
}

/// A thing that renders: the tools' button as an element.
impl IntoElement for ToolButton {
    type Element = Stateful<Div>;

    fn into_element(self) -> Self::Element {
        self.build()
    }
}
