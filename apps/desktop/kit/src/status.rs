//! Small things that say how something is: the engine's status pill (`status-pill.js`), the chips
//! above every screen (`rec-chip.js`), a notice banner, a group's chip (`group-chip.js`), a phone's
//! link mark (`link-state.js`) and the warning triangle.

use std::rc::Rc;
use std::time::Duration;

use bandear_desktop_console::chips::{Chip, ChipKind};
use bandear_desktop_console::format::LinkLevel;
use bandear_desktop_console::router::Section;
use bandear_desktop_console::state::Phase;
use bandear_desktop_console::values::GroupTone;
use gpui::{
    Animation, AnimationExt, App, ClickEvent, ElementId, InteractiveElement, IntoElement,
    ParentElement, RenderOnce, Role, SharedString, StatefulInteractiveElement, Styled, Window, div,
    prelude::FluentBuilder, px, svg,
};

use crate::assets::{PLAY, TRIANGLE, WARNING};
use crate::buttons::{Button, ButtonKind};
use crate::theme::{FONT_MONO, FONT_SANS, TARGET, color, colors, radius, text, weight};

/// The opacity of a pulse `delta` of the way through its cycle: from 1 down to `low` and back,
/// eased in and out, as the web console's `alternate` pulses.
fn pulse(delta: f32, low: f32) -> f32 {
    let there = if delta < 0.5 {
        delta * 2.0
    } else {
        2.0 - delta * 2.0
    };
    let eased = there * there * (3.0 - 2.0 * there);
    1.0 - (1.0 - low) * eased
}

/// A dot that pulses with a `period` long cycle (down and back up), or stays still with none.
fn pulsing(
    id: impl Into<ElementId>,
    dot: gpui::Div,
    period: Option<Duration>,
    low: f32,
) -> gpui::AnyElement {
    match period {
        Some(period) => dot
            .with_animation(id, Animation::new(period).repeat(), move |dot, delta| {
                dot.opacity(pulse(delta, low))
            })
            .into_any_element(),
        None => dot.into_any_element(),
    }
}

/// The status pill's word for a phase.
pub fn phase_word(phase: Option<Phase>) -> &'static str {
    match phase {
        Some(Phase::Idle) | None => "Not running",
        Some(Phase::Starting) => "Starting",
        Some(Phase::Running) => "Running",
        Some(Phase::Stopping) => "Stopping",
        Some(Phase::Stopped) => "Stopped",
        Some(Phase::Error) => "Error",
    }
}

/// The engine's status as a pill: its word and a dot (green when running, the accent pulsing while
/// starting or stopping, a red square on an error, a hollow ring otherwise). To assistive
/// technology a `Status`, so a change of phase is announced.
#[derive(IntoElement)]
pub struct StatusPill {
    id: ElementId,
    phase: Option<Phase>,
    label: Option<SharedString>,
}

impl StatusPill {
    pub fn new(id: impl Into<ElementId>, phase: Option<Phase>) -> Self {
        Self {
            id: id.into(),
            phase,
            label: None,
        }
    }

    /// A word of its own in place of the phase's.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl RenderOnce for StatusPill {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let word = self.label.unwrap_or_else(|| phase_word(self.phase).into());
        let (background, border) = match self.phase {
            Some(Phase::Running) => (colors::OK_BG, colors::OK_BORDER),
            Some(Phase::Starting | Phase::Stopping) => (colors::WARN_BG, colors::WARN_BORDER),
            Some(Phase::Error) => (colors::DANGER_BG, colors::DANGER_BORDER),
            _ => (colors::SURFACE_RAISED, colors::BORDER),
        };
        let dot = div().flex_none().size(px(8.0));
        let dot = match self.phase {
            Some(Phase::Running) => pulsing(
                "dot",
                dot.rounded_full().bg(color(colors::GREEN)),
                None,
                1.0,
            ),
            Some(Phase::Starting | Phase::Stopping) => pulsing(
                "dot",
                dot.rounded_full().bg(color(colors::ACCENT)),
                Some(Duration::from_secs(2)),
                0.3,
            ),
            Some(Phase::Error) => pulsing(
                "dot",
                dot.rounded(px(1.0)).bg(color(colors::RED)),
                None,
                1.0,
            ),
            _ => pulsing(
                "dot",
                dot.rounded_full()
                    .border_2()
                    .border_color(color(colors::TEXT_MUTED)),
                None,
                1.0,
            ),
        };
        div()
            .id(self.id)
            .role(Role::Status)
            .aria_label(word.clone())
            .flex()
            .items_center()
            .gap(px(8.0))
            .min_h(px(TARGET))
            .px(px(12.0))
            .rounded(px(radius::XL))
            .border_1()
            .border_color(color(border))
            .bg(color(background))
            .text_color(color(colors::TEXT))
            .font_family(FONT_SANS)
            .text_size(px(text::SM))
            .font_weight(weight::BOLD)
            .whitespace_nowrap()
            .child(dot)
            .child(crate::a11y::seen(word))
    }
}

/// A chip above the screens: one link, named by the chip's label (also its tooltip); its word,
/// time and detail are only seen. A press opens the chip's section.
#[derive(IntoElement)]
pub struct ChipLink {
    id: ElementId,
    chip: Chip,
    on_open: Option<Rc<dyn Fn(Section, &mut Window, &mut App)>>,
}

impl ChipLink {
    pub fn new(id: impl Into<ElementId>, chip: Chip) -> Self {
        Self {
            id: id.into(),
            chip,
            on_open: None,
        }
    }

    /// Hears a press, with the section the chip opens.
    pub fn on_open(mut self, on_open: impl Fn(Section, &mut Window, &mut App) + 'static) -> Self {
        self.on_open = Some(Rc::new(on_open));
        self
    }
}

impl RenderOnce for ChipLink {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let Chip {
            kind,
            word,
            time,
            detail,
            label,
            target,
        } = self.chip;
        let (border, background, hover) = match kind {
            ChipKind::Recording | ChipKind::Failed | ChipKind::SoundcheckFailed => {
                (colors::DANGER_BORDER, colors::DANGER_BG, colors::RED)
            }
            ChipKind::Stopping => (colors::WARN_BORDER, colors::WARN_BG, colors::RED),
            ChipKind::Soundcheck | ChipKind::SoundcheckPaused => {
                (colors::WARN_BORDER, colors::ACCENT_BG, colors::ACCENT)
            }
            ChipKind::Demo => (colors::BLUE, colors::SURFACE_RAISED, colors::TEXT_SECONDARY),
        };
        let dot: gpui::AnyElement = match kind {
            ChipKind::Recording => pulsing(
                "dot",
                div()
                    .flex_none()
                    .size(px(10.0))
                    .rounded_full()
                    .bg(color(colors::RED)),
                Some(Duration::from_millis(1200)),
                0.35,
            ),
            ChipKind::Stopping => div()
                .flex_none()
                .size(px(10.0))
                .rounded_full()
                .bg(color(colors::ACCENT))
                .into_any_element(),
            ChipKind::Failed => div()
                .flex_none()
                .size(px(10.0))
                .rounded(px(2.0))
                .bg(color(colors::RED))
                .into_any_element(),
            ChipKind::Demo => div()
                .flex_none()
                .size(px(10.0))
                .rounded(px(2.0))
                .bg(color(colors::BLUE))
                .into_any_element(),
            ChipKind::Soundcheck | ChipKind::SoundcheckPaused | ChipKind::SoundcheckFailed => {
                let tone = if kind == ChipKind::SoundcheckFailed {
                    colors::RED
                } else {
                    colors::ACCENT
                };
                let triangle = svg()
                    .path(PLAY)
                    .flex_none()
                    .w(px(10.0))
                    .h(px(12.0))
                    .text_color(color(tone));
                if kind == ChipKind::Soundcheck {
                    triangle
                        .with_animation(
                            "dot",
                            Animation::new(Duration::from_millis(1600)).repeat(),
                            |triangle, delta| triangle.opacity(pulse(delta, 0.35)),
                        )
                        .into_any_element()
                } else {
                    triangle.into_any_element()
                }
            }
        };
        let on_open = self.on_open;
        let tooltip = SharedString::from(label.clone());
        div()
            .id(self.id)
            .role(Role::Link)
            .aria_label(label)
            .focusable()
            .tab_stop(true)
            .tooltip(move |window, cx| crate::tooltip::tooltip(tooltip.clone(), window, cx))
            .flex()
            .items_center()
            .gap(px(8.0))
            .min_h(px(TARGET))
            .px(px(12.0))
            .rounded(px(radius::XL))
            .border_1()
            .border_color(color(border))
            .bg(color(background))
            .text_color(color(colors::TEXT))
            .font_family(FONT_SANS)
            .text_size(px(text::SM))
            .font_weight(weight::BOLD)
            .whitespace_nowrap()
            .cursor_pointer()
            .hover(|style| style.border_color(color(hover)))
            .focus_visible(|style| style.border_2().border_color(color(colors::FOCUS)))
            .on_click(move |_: &ClickEvent, window, cx| {
                if let Some(on_open) = &on_open {
                    on_open(target, window, cx);
                }
            })
            .child(dot)
            .child(crate::a11y::seen(word))
            .when(!time.is_empty(), |chip| {
                chip.child(div().font_family(FONT_MONO).child(crate::a11y::seen(time)))
            })
            .when(!detail.is_empty(), |chip| {
                chip.child(
                    div()
                        .font_weight(weight::MEDIUM)
                        .text_color(color(colors::TEXT_SECONDARY))
                        .child(crate::a11y::seen(detail)),
                )
            })
    }
}

/// How urgent a notice is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoticeKind {
    /// Something the engineer should know (a status, announced politely).
    Status,
    /// Something went wrong (an alert, announced at once).
    Alert,
}

/// A notice over a screen: a title, a text, and at most one action.
#[derive(IntoElement)]
pub struct Notice {
    id: ElementId,
    kind: NoticeKind,
    title: SharedString,
    text: Option<SharedString>,
    action: Option<(SharedString, Rc<dyn Fn(&mut Window, &mut App)>)>,
    extra: Option<gpui::AnyElement>,
}

impl Notice {
    pub fn new(id: impl Into<ElementId>, kind: NoticeKind, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            kind,
            title: title.into(),
            text: None,
            action: None,
            extra: None,
        }
    }

    pub fn text(mut self, text: impl Into<SharedString>) -> Self {
        self.text = Some(text.into());
        self
    }

    /// A button under the text (`Start engine`, `Try again`).
    pub fn action(
        mut self,
        label: impl Into<SharedString>,
        on_press: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.action = Some((label.into(), Rc::new(on_press)));
        self
    }

    /// Anything else under the text (a link in the sentence, say).
    pub fn child(mut self, extra: impl IntoElement) -> Self {
        self.extra = Some(extra.into_any_element());
        self
    }
}

impl RenderOnce for Notice {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let (background, border, role) = match self.kind {
            NoticeKind::Status => (colors::SURFACE, colors::BORDER, Role::Status),
            NoticeKind::Alert => (colors::DANGER_BG, colors::DANGER_BORDER, Role::Alert),
        };
        let label = match &self.text {
            Some(text) => SharedString::from(format!("{} {text}", self.title)),
            None => self.title.clone(),
        };
        div()
            .id(self.id)
            .role(role)
            .aria_label(label)
            .flex()
            .flex_col()
            .gap(px(8.0))
            .p(px(16.0))
            .rounded(px(radius::LG))
            .border_1()
            .border_color(color(border))
            .bg(color(background))
            .font_family(FONT_SANS)
            .child(
                div()
                    .text_size(px(text::LG))
                    .font_weight(weight::BOLD)
                    .text_color(color(colors::TEXT))
                    .child(crate::a11y::seen(self.title)),
            )
            .when_some(self.text, |notice, body| {
                notice.child(
                    div()
                        .text_size(px(text::BASE))
                        .text_color(color(colors::TEXT_SECONDARY))
                        .child(crate::a11y::seen(body)),
                )
            })
            .when_some(self.extra, |notice, extra| notice.child(extra))
            .when_some(self.action, |notice, (label, on_press)| {
                notice.child(
                    div().flex().child(
                        Button::new("action", label, ButtonKind::Primary)
                            .large()
                            .on_press(move |window, cx| on_press(window, cx)),
                    ),
                )
            })
    }
}

/// A group's chip: its name in its tone; dashed with a `!` when the group names an input the
/// device lacks ("No group" with no name).
#[derive(IntoElement)]
pub struct GroupChip {
    id: ElementId,
    tone: GroupTone,
    name: Option<SharedString>,
    invalid: bool,
}

impl GroupChip {
    pub fn new(
        id: impl Into<ElementId>,
        tone: GroupTone,
        name: Option<SharedString>,
        invalid: bool,
    ) -> Self {
        Self {
            id: id.into(),
            tone,
            name,
            invalid,
        }
    }
}

impl RenderOnce for GroupChip {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let (background, foreground) = match self.tone {
            GroupTone::Teal => (colors::GROUP_TEAL_BG, colors::GROUP_TEAL_TEXT),
            GroupTone::Blue => (colors::GROUP_BLUE_BG, colors::GROUP_BLUE_TEXT),
            GroupTone::Violet => (colors::GROUP_VIOLET_BG, colors::GROUP_VIOLET_TEXT),
            GroupTone::Orange => (colors::GROUP_ORANGE_BG, colors::GROUP_ORANGE_TEXT),
            GroupTone::None => (colors::GROUP_NONE_BG, colors::GROUP_NONE_TEXT),
        };
        let name = self.name.unwrap_or_else(|| "No group".into());
        let label = if self.invalid {
            SharedString::from(format!("{name}, input missing"))
        } else {
            name.clone()
        };
        div()
            .id(self.id)
            .role(Role::Label)
            .aria_label(label)
            .flex()
            .items_center()
            .gap(px(4.0))
            .max_w_full()
            .px(px(7.0))
            .py(px(2.0))
            .rounded(px(radius::SM))
            .border_1()
            .bg(color(background))
            .text_color(color(foreground))
            .font_family(FONT_SANS)
            .text_size(px(text::XXS))
            .font_weight(weight::MEDIUM)
            .line_height(px(14.0))
            .whitespace_nowrap()
            .map(|chip| {
                if self.invalid {
                    chip.border_color(color(foreground)).border_dashed()
                } else {
                    chip.border_color(gpui::transparent_black())
                }
            })
            .when(self.invalid, |chip| {
                chip.child(
                    div()
                        .font_weight(weight::BOLD)
                        .child(crate::a11y::seen("!")),
                )
            })
            .child(
                div()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(crate::a11y::seen(name)),
            )
    }
}

/// A phone's link in a word and a mark: a green dot when good, an accent triangle on a warning, a
/// red square when bad, a hollow ring when unknown.
#[derive(IntoElement)]
pub struct LinkMark {
    id: ElementId,
    level: LinkLevel,
    label: Option<SharedString>,
    detail: Option<SharedString>,
}

impl LinkMark {
    pub fn new(id: impl Into<ElementId>, level: LinkLevel) -> Self {
        Self {
            id: id.into(),
            level,
            label: None,
            detail: None,
        }
    }

    /// The word in place of the level's (`Link lost`, `No audio`).
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// The numbers behind it, read after the word.
    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

impl RenderOnce for LinkMark {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let word: SharedString = self.label.unwrap_or_else(|| {
            match self.level {
                LinkLevel::Good => "Good",
                LinkLevel::Warning => "Warning",
                LinkLevel::Bad => "Poor",
                LinkLevel::Unknown => "Unknown",
            }
            .into()
        });
        let mark: gpui::AnyElement = match self.level {
            LinkLevel::Good => div()
                .flex_none()
                .size(px(8.0))
                .rounded_full()
                .bg(color(colors::GREEN))
                .into_any_element(),
            LinkLevel::Warning => svg()
                .path(TRIANGLE)
                .flex_none()
                .w(px(10.0))
                .h(px(9.0))
                .text_color(color(colors::ACCENT))
                .into_any_element(),
            LinkLevel::Bad => div()
                .flex_none()
                .size(px(8.0))
                .rounded(px(1.0))
                .bg(color(colors::RED))
                .into_any_element(),
            LinkLevel::Unknown => div()
                .flex_none()
                .size(px(8.0))
                .rounded_full()
                .border_2()
                .border_color(color(colors::TEXT_MUTED))
                .into_any_element(),
        };
        div()
            .id(self.id)
            .role(Role::Label)
            .aria_label(word.clone())
            .when_some(self.detail, |link, detail| link.aria_description(detail))
            .flex()
            .items_center()
            .gap(px(6.0))
            .font_weight(weight::BOLD)
            .whitespace_nowrap()
            .child(mark)
            .child(crate::a11y::seen(word))
    }
}

/// The warning triangle, `size` px, in the accent (decorative: what it warns of is said beside it).
pub fn warning_icon(size: f32) -> impl IntoElement {
    svg()
        .path(WARNING)
        .flex_none()
        .size(px(size))
        .text_color(color(colors::ACCENT))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pill_says_each_phase_in_words() {
        let words: Vec<_> = [
            None,
            Some(Phase::Idle),
            Some(Phase::Starting),
            Some(Phase::Running),
        ]
        .into_iter()
        .chain([
            Some(Phase::Stopping),
            Some(Phase::Stopped),
            Some(Phase::Error),
        ])
        .map(phase_word)
        .collect();
        assert_eq!(
            words,
            [
                "Not running",
                "Not running",
                "Starting",
                "Running",
                "Stopping",
                "Stopped",
                "Error"
            ]
        );
    }

    #[test]
    fn a_pulse_goes_down_and_back_up() {
        assert_eq!(pulse(0.0, 0.3), 1.0);
        assert!((pulse(0.5, 0.3) - 0.3).abs() < 1e-6);
        assert!((pulse(1.0, 0.3) - 1.0).abs() < 1e-6);
    }
}
