//! The buttons: plain ones (primary, secondary, danger), the Mute and Solo toggles (`toggle.js`)
//! and the button that asks for a second press before it acts (a removal).

use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    App, ClickEvent, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce, Role,
    SharedString, StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
};

use crate::a11y::toggled;
use crate::theme::{FONT_SANS, TARGET, TARGET_PRIMARY, color, colors, radius, text, weight};

/// What a button does when pressed.
pub type OnPress = Rc<dyn Fn(&mut Window, &mut App)>;

/// How a button looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonKind {
    /// Accent fill (Start engine).
    Primary,
    /// Raised surface with a border.
    Secondary,
    /// Danger background and text (Stop engine, Remove).
    Danger,
}

/// A button: 32 px high (40 for a primary action), 10 px radius, bold; a `Button` to assistive
/// technology, pressed by a click, Enter or Space.
#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    label: SharedString,
    kind: ButtonKind,
    large: bool,
    disabled: bool,
    description: Option<SharedString>,
    on_press: Option<OnPress>,
}

impl Button {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>, kind: ButtonKind) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            kind,
            large: false,
            disabled: false,
            description: None,
            on_press: None,
        }
    }

    /// The primary action's height (40 px).
    pub fn large(mut self) -> Self {
        self.large = true;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// A longer description for assistive technology.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn on_press(mut self, on_press: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_press = Some(Rc::new(on_press));
        self
    }
}

impl RenderOnce for Button {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let (background, border, foreground, hover) = match self.kind {
            ButtonKind::Primary => (
                colors::ACCENT,
                colors::ACCENT,
                colors::TEXT_ON_BRIGHT,
                colors::ACCENT_HOVER,
            ),
            ButtonKind::Secondary => (
                colors::SURFACE_RAISED,
                colors::BORDER,
                colors::TEXT,
                colors::TEXT_MUTED,
            ),
            ButtonKind::Danger => (
                colors::DANGER_BG,
                colors::DANGER_BORDER,
                colors::DANGER_TEXT,
                colors::RED,
            ),
        };
        let disabled = self.disabled;
        let on_press = self.on_press;
        div()
            .id(self.id)
            .role(Role::Button)
            .aria_label(self.label.clone())
            .when_some(self.description, |button, description| {
                button.aria_description(description)
            })
            .focusable()
            .tab_stop(!disabled)
            .flex()
            .items_center()
            .justify_center()
            .gap(px(6.0))
            .min_h(px(if self.large { TARGET_PRIMARY } else { TARGET }))
            .px(px(if self.large { 16.0 } else { 12.0 }))
            .rounded(px(radius::MD))
            .border_1()
            .border_color(color(border))
            .bg(color(background))
            .text_color(color(foreground))
            .font_family(FONT_SANS)
            .text_size(px(if self.large { text::MD } else { text::SM }))
            .font_weight(weight::BOLD)
            .whitespace_nowrap()
            .focus_visible(|style| style.border_2().border_color(color(colors::FOCUS)))
            .when(disabled, |button| button.opacity(0.5))
            .when(!disabled, |button| {
                button
                    .cursor_pointer()
                    .hover(|style| style.border_color(color(hover)))
            })
            .on_click(move |_: &ClickEvent, window, cx| {
                if disabled {
                    return;
                }
                if let Some(on_press) = &on_press {
                    on_press(window, cx);
                }
            })
            .child(crate::a11y::seen(self.label))
    }
}

/// What a toggle turns on: its fill color when pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToggleKind {
    /// A mute: red when on.
    Mute,
    /// A solo: the accent when on.
    Solo,
    /// Anything else: the text color when on.
    Plain,
}

/// A toggle button (Mute, Solo): pressed it takes its fill and shows a lamp bar along its bottom,
/// so its state never rests on color alone; to assistive technology a `Button` with its pressed
/// state, named by `label` ("Mute Kick").
#[derive(IntoElement)]
pub struct ToggleButton {
    id: ElementId,
    text: SharedString,
    label: SharedString,
    kind: ToggleKind,
    pressed: bool,
    wide: bool,
    disabled: bool,
    on_toggle: Option<Rc<dyn Fn(bool, &mut Window, &mut App)>>,
}

impl ToggleButton {
    /// A toggle showing `text` ("M", "S"), named `label`, `pressed` or not.
    pub fn new(
        id: impl Into<ElementId>,
        text: impl Into<SharedString>,
        label: impl Into<SharedString>,
        kind: ToggleKind,
        pressed: bool,
    ) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
            label: label.into(),
            kind,
            pressed,
            wide: false,
            disabled: false,
            on_toggle: None,
        }
    }

    /// At least 64 px wide (a master's mute).
    pub fn wide(mut self) -> Self {
        self.wide = true;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Hears the state a press asks for.
    pub fn on_toggle(mut self, on_toggle: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_toggle = Some(Rc::new(on_toggle));
        self
    }
}

impl RenderOnce for ToggleButton {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let fill = match self.kind {
            ToggleKind::Mute => colors::RED,
            ToggleKind::Solo => colors::ACCENT,
            ToggleKind::Plain => colors::TEXT,
        };
        let (pressed, disabled) = (self.pressed, self.disabled);
        let on_toggle = self.on_toggle;
        div()
            .id(self.id)
            .role(Role::Button)
            .aria_label(self.label)
            .aria_toggled(toggled(pressed))
            .focusable()
            .tab_stop(!disabled)
            .relative()
            .flex()
            .items_center()
            .justify_center()
            .min_w(px(if self.wide { 64.0 } else { TARGET }))
            .h(px(TARGET))
            .px(px(8.0))
            .rounded(px(radius::SM))
            .border_1()
            .font_family(FONT_SANS)
            .text_size(px(text::SM))
            .font_weight(weight::BOLD)
            .map(|button| {
                if pressed {
                    button
                        .border_color(color(fill))
                        .bg(color(fill))
                        .text_color(color(colors::TEXT_ON_BRIGHT))
                } else {
                    button
                        .border_color(color(colors::BORDER))
                        .bg(color(colors::SURFACE_RAISED))
                        .text_color(color(colors::TEXT))
                }
            })
            .focus_visible(|style| style.border_2().border_color(color(colors::FOCUS)))
            .when(disabled, |button| button.opacity(0.5))
            .when(!disabled, |button| {
                button
                    .cursor_pointer()
                    .hover(|style| style.border_color(color(colors::TEXT_MUTED)))
            })
            .on_click(move |_: &ClickEvent, window, cx| {
                if disabled {
                    return;
                }
                if let Some(on_toggle) = &on_toggle {
                    on_toggle(!pressed, window, cx);
                }
            })
            .child(crate::a11y::seen(self.text))
            .when(pressed, |button| {
                // The lamp: the state in a shape, not only a color.
                button.child(
                    div()
                        .absolute()
                        .left(px(6.0))
                        .right(px(6.0))
                        .bottom(px(3.0))
                        .h(px(2.0))
                        .rounded(px(1.0))
                        .bg(color(colors::TEXT_ON_BRIGHT)),
                )
            })
    }
}

/// How long a confirming press waits after the first one, so a double-click never confirms.
pub const CONFIRM_DELAY: Duration = Duration::from_millis(600);
/// How long a first press stays armed.
pub const CONFIRM_TIMEOUT: Duration = Duration::from_secs(10);

/// True once a press armed at `armed_at` has waited long enough to be confirmed at `now`
/// (`confirmReady`).
pub fn confirm_ready(armed_at: Option<Instant>, now: Instant) -> bool {
    armed_at.is_some_and(|armed| now.duration_since(armed) >= CONFIRM_DELAY)
}

struct Armed {
    at: Option<Instant>,
    generation: u64,
}

/// A button that acts only on a second press (Remove a phone): the first arms it and shows
/// `armed_label` ("Confirm remove"), a press at least 600 ms later acts, and it disarms by itself
/// after 10 s.
#[derive(IntoElement)]
pub struct ConfirmButton {
    id: ElementId,
    label: SharedString,
    armed_label: SharedString,
    disabled: bool,
    on_confirm: Option<OnPress>,
}

impl ConfirmButton {
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        armed_label: impl Into<SharedString>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            armed_label: armed_label.into(),
            disabled: false,
            on_confirm: None,
        }
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_confirm(mut self, on_confirm: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_confirm = Some(Rc::new(on_confirm));
        self
    }
}

impl RenderOnce for ConfirmButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let armed = window.use_keyed_state(self.id.clone(), cx, |_, _| Armed {
            at: None,
            generation: 0,
        });
        let is_armed = armed.read(cx).at.is_some();
        let on_confirm = self.on_confirm;
        let label = if is_armed {
            self.armed_label
        } else {
            self.label
        };
        let state = armed.clone();
        Button::new(self.id, label, ButtonKind::Danger)
            .disabled(self.disabled)
            .on_press(move |window, cx| {
                let now = Instant::now();
                let armed_at = state.read(cx).at;
                if armed_at.is_some() {
                    if confirm_ready(armed_at, now) {
                        state.update(cx, |armed, cx| {
                            armed.at = None;
                            cx.notify();
                        });
                        if let Some(on_confirm) = &on_confirm {
                            on_confirm(window, cx);
                        }
                    }
                    return;
                }
                let generation = state.update(cx, |armed, cx| {
                    armed.at = Some(now);
                    armed.generation += 1;
                    cx.notify();
                    armed.generation
                });
                let state = state.clone();
                cx.spawn(async move |cx| {
                    cx.background_executor().timer(CONFIRM_TIMEOUT).await;
                    state.update(cx, |armed, cx| {
                        if armed.generation == generation && armed.at.is_some() {
                            armed.at = None;
                            cx.notify();
                        }
                    });
                })
                .detach();
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_press_confirms_only_once_the_first_has_waited() {
        let armed = Instant::now();
        assert!(!confirm_ready(None, armed), "nothing armed");
        assert!(!confirm_ready(
            Some(armed),
            armed + Duration::from_millis(599)
        ));
        assert!(confirm_ready(Some(armed), armed + CONFIRM_DELAY));
    }
}
