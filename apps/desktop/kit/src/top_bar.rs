//! The Mix screen's top bar (`top-bar.js`): the engine's status pill, the device and its format,
//! the phones that need attention, and the button that starts or stops the engine. The Mix screen
//! works out what it says (`topBarView`) and draws it with this.

use std::rc::Rc;

use bandear_desktop_console::state::Phase;
use gpui::{
    App, ClickEvent, InteractiveElement, IntoElement, ParentElement, RenderOnce, Role,
    SharedString, StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
};

use crate::buttons::{Button, ButtonKind};
use crate::status::{StatusPill, warning_icon};
use crate::theme::{FONT_MONO, FONT_SANS, TARGET, color, colors, radius, text, weight};

/// What the engine button does now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineAction {
    Start,
    Stop,
}

/// What the top bar says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TopBarData {
    pub phase: Option<Phase>,
    /// The input device, or `None` to hide the readout.
    pub device: Option<SharedString>,
    /// The capture format (`48 kHz · 24-bit · 256`).
    pub format: SharedString,
    /// The attention link's words (`1 phone needs attention`), or `None` to hide it.
    pub attention: Option<SharedString>,
    /// What the engine button does, or `None` while the engine starts or stops (busy).
    pub action: Option<EngineAction>,
}

/// The engine button's words while it is busy.
fn busy_words(phase: Option<Phase>) -> &'static str {
    if phase == Some(Phase::Stopping) {
        "Stopping\u{2026}"
    } else {
        "Starting\u{2026}"
    }
}

/// The top bar: left to right, wrapping, 10 px apart.
#[derive(IntoElement)]
pub struct TopBar {
    data: TopBarData,
    on_attention: Option<Rc<dyn Fn(&mut Window, &mut App)>>,
    on_engine: Option<Rc<dyn Fn(EngineAction, &mut Window, &mut App)>>,
}

impl TopBar {
    pub fn new(data: TopBarData) -> Self {
        Self {
            data,
            on_attention: None,
            on_engine: None,
        }
    }

    /// Hears a press on the attention link (it opens Network and system).
    pub fn on_attention(mut self, listener: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_attention = Some(Rc::new(listener));
        self
    }

    /// Hears a press on the engine button, with what it does.
    pub fn on_engine(
        mut self,
        listener: impl Fn(EngineAction, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_engine = Some(Rc::new(listener));
        self
    }
}

impl RenderOnce for TopBar {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let TopBarData {
            phase,
            device,
            format,
            attention,
            action,
        } = self.data;
        let on_attention = self.on_attention;
        let on_engine = self.on_engine;
        let button = match action {
            Some(EngineAction::Start) => Button::new("engine", "Start engine", ButtonKind::Primary),
            Some(EngineAction::Stop) => Button::new("engine", "Stop engine", ButtonKind::Danger),
            None => Button::new("engine", busy_words(phase), ButtonKind::Secondary).disabled(true),
        }
        .large()
        .on_press(move |window, cx| {
            if let (Some(action), Some(on_engine)) = (action, &on_engine) {
                on_engine(action, window, cx);
            }
        });
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(10.0))
            .font_family(FONT_SANS)
            .child(StatusPill::new("status", phase))
            .when_some(device, |bar, device| {
                let words = SharedString::from(format!("{device} \u{b7} {format}"));
                bar.child(
                    div()
                        .id("readout")
                        .role(Role::Label)
                        .aria_label(words)
                        .flex()
                        .text_size(px(text::SM))
                        .text_color(color(colors::TEXT_SECONDARY))
                        .child(
                            div()
                                .font_family(FONT_MONO)
                                .text_color(color(colors::TEXT))
                                .child(crate::a11y::seen(device)),
                        )
                        .child(crate::a11y::seen(" \u{b7} "))
                        .child(
                            div()
                                .font_family(FONT_MONO)
                                .child(crate::a11y::seen(format)),
                        ),
                )
            })
            .child(div().flex_1())
            .when_some(attention, |bar, attention| {
                bar.child(
                    div()
                        .id("attention")
                        .role(Role::Link)
                        .aria_label(attention.clone())
                        .focusable()
                        .tab_stop(true)
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .min_h(px(TARGET))
                        .px(px(12.0))
                        .rounded(px(radius::XL))
                        .border_1()
                        .border_color(color(colors::WARN_BORDER))
                        .bg(color(colors::WARN_BG))
                        .text_color(color(colors::TEXT))
                        .text_size(px(text::SM))
                        .font_weight(weight::BOLD)
                        .cursor_pointer()
                        .hover(|style| style.border_color(color(colors::ACCENT)))
                        .focus_visible(|style| style.border_2().border_color(color(colors::FOCUS)))
                        .on_click(move |_: &ClickEvent, window, cx| {
                            if let Some(on_attention) = &on_attention {
                                on_attention(window, cx);
                            }
                        })
                        .child(warning_icon(14.0))
                        .child(crate::a11y::seen(attention)),
                )
            })
            .child(button)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_busy_engine_button_says_what_it_waits_for() {
        assert_eq!(busy_words(Some(Phase::Starting)), "Starting\u{2026}");
        assert_eq!(busy_words(Some(Phase::Stopping)), "Stopping\u{2026}");
    }
}
