//! Small pieces the screen's cards share: the card itself, the lines that say something (an
//! error, a status, a caution), a select whose options can be disabled, and the select helpers
//! that rebuild options only when they change (N56).

use bandear_desktop_kit::a11y::seen;
use bandear_desktop_kit::forms::Choice;
use bandear_desktop_kit::gpui::{
    App, AppContext as _, Context, Div, ElementId, Entity, InteractiveElement, IntoElement,
    ParentElement, Role, SharedString, Stateful, StatefulInteractiveElement, Styled, Window, div,
    prelude::FluentBuilder, px,
};
use bandear_desktop_kit::gpui_component::IndexPath;
use bandear_desktop_kit::gpui_component::select::{SelectItem, SelectState};
use bandear_desktop_kit::theme::{FONT_MONO, FONT_SANS, color, colors, radius, text, weight};

use crate::model::audio;
use crate::model::musicians::Said;

/// A card of the screen: raised from the ground, 14/16 px padding, its title an `h2`.
pub fn card(id: impl Into<ElementId>, title: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_col()
        .gap(px(10.0))
        .py(px(14.0))
        .px(px(16.0))
        .rounded(px(radius::XL))
        .border_1()
        .border_color(color(colors::BORDER))
        .bg(color(colors::SURFACE))
        .child(heading2(title))
}

/// A second-level heading.
pub fn heading2(title: impl Into<SharedString>) -> impl IntoElement {
    let title = title.into();
    div()
        .id(SharedString::from(format!("heading-{title}")))
        .role(Role::Heading)
        .aria_level(2)
        .aria_label(title.clone())
        .font_family(FONT_SANS)
        .text_size(px(text::LG))
        .font_weight(weight::BOLD)
        .text_color(color(colors::TEXT))
        .child(seen(title))
}

/// How a line says what it says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    /// An error: danger text, announced at once.
    Error,
    /// A word in passing: announced politely.
    Status,
    /// A caution in a warn box.
    Caution,
    /// A hint in the muted color.
    Hint,
}

/// A line of `words` in `tone`, read out as an alert (errors) or a status.
pub fn line(id: impl Into<ElementId>, tone: Tone, words: impl Into<SharedString>) -> Stateful<Div> {
    let words = words.into();
    let role = match tone {
        Tone::Error => Role::Alert,
        Tone::Status | Tone::Caution => Role::Status,
        Tone::Hint => Role::Paragraph,
    };
    div()
        .id(id)
        .role(role)
        .aria_label(words.clone())
        .font_family(FONT_SANS)
        .text_size(px(text::MD))
        .line_height(px(text::MD * text::LINE_HEIGHT))
        .map(|line| match tone {
            Tone::Error => line.text_color(color(colors::DANGER_TEXT)),
            Tone::Status => line.text_color(color(colors::TEXT)),
            Tone::Hint => line.text_color(color(colors::TEXT_MUTED)),
            Tone::Caution => line
                .py(px(8.0))
                .px(px(10.0))
                .rounded(px(radius::MD))
                .border_1()
                .border_color(color(colors::WARN_BORDER))
                .bg(color(colors::WARN_BG))
                .text_color(color(colors::TEXT)),
        })
        .child(seen(words))
}

/// An outcome line: a status when it worked, an error (red, an alert) when not.
pub fn said_line(id: impl Into<ElementId>, said: &Said) -> Stateful<Div> {
    line(
        id,
        if said.ok { Tone::Status } else { Tone::Error },
        said.message.clone(),
    )
}

/// Monospaced small text in the muted color (an address, a detail).
pub fn mono(words: impl Into<SharedString>, size: f32) -> Div {
    div()
        .font_family(FONT_MONO)
        .text_size(px(size))
        .text_color(color(colors::TEXT_MUTED))
        .child(seen(words.into()))
}

/// A label beside a control.
pub fn label(words: &'static str) -> Div {
    div()
        .font_family(FONT_SANS)
        .text_size(px(text::MD))
        .text_color(color(colors::TEXT_SECONDARY))
        .child(seen(words))
}

/// The kit's choices for a model's.
pub fn choices(options: &[audio::Choice]) -> Vec<Choice> {
    options
        .iter()
        .map(|option| Choice::new(option.value.clone(), option.label.clone()))
        .collect()
}

/// What a select shows now: its options and the value picked, so it is touched only when they
/// change and an open list is not torn down by a status (N56).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Shown {
    pub options: Vec<(String, String)>,
    pub value: String,
}

/// Brings `state` to `options` with `value` picked, rebuilding the options only when what they say
/// changed (a disabled option counts).
pub fn sync_select<T>(
    state: &Entity<SelectState<Vec<T>>>,
    shown: &mut Shown,
    options: Vec<T>,
    value: &str,
    window: &mut Window,
    cx: &mut App,
) where
    T: SelectItem<Value = SharedString> + 'static,
{
    let listed: Vec<(String, String)> = options
        .iter()
        .map(|option| {
            let mut title = option.title().to_string();
            if option.disabled() {
                title.push('\u{0}');
            }
            (option.value().to_string(), title)
        })
        .collect();
    if listed != shown.options {
        shown.options = listed;
        shown.value = String::from('\u{0}');
        state.update(cx, |state, cx| state.set_items(options, window, cx));
    }
    if shown.value != value {
        shown.value = value.to_owned();
        let row = shown
            .options
            .iter()
            .position(|(option, _)| option == value)
            .map(|row| IndexPath::default().row(row));
        state.update(cx, |state, cx| state.set_selected_index(row, window, cx));
    }
}

/// A Copy choice of the Redundancy cell: an option that can be disabled ("needs a newer app").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Option_ {
    pub value: SharedString,
    pub label: SharedString,
    pub disabled: bool,
}

impl SelectItem for Option_ {
    type Value = SharedString;

    fn title(&self) -> SharedString {
        self.label.clone()
    }

    fn value(&self) -> &SharedString {
        &self.value
    }

    fn disabled(&self) -> bool {
        self.disabled
    }
}

/// A select over options that can be disabled.
pub type OptionState = SelectState<Vec<Option_>>;

/// A select state over `options` with nothing picked yet.
pub fn option_state<T: 'static>(
    options: Vec<Option_>,
    window: &mut Window,
    cx: &mut Context<T>,
) -> Entity<OptionState> {
    cx.new(|cx| SelectState::new(options, None, window, cx))
}

/// A button the screen moves the focus to (Keep, Cancel, Load): the kit's [Button] look, with a
/// focus handle of the screen's, since the kit's takes none (a shared gap). Pressed by a click,
/// Enter or Space.
///
/// [Button]: bandear_desktop_kit::buttons::Button
pub fn focus_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    kind: bandear_desktop_kit::buttons::ButtonKind,
    focus: &bandear_desktop_kit::gpui::FocusHandle,
    disabled: bool,
    on_press: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    use bandear_desktop_kit::buttons::ButtonKind;
    use bandear_desktop_kit::theme::TARGET;
    let label = label.into();
    let (background, border, foreground) = match kind {
        ButtonKind::Primary => (colors::ACCENT, colors::ACCENT, colors::TEXT_ON_BRIGHT),
        ButtonKind::Secondary => (colors::SURFACE_RAISED, colors::BORDER, colors::TEXT),
        ButtonKind::Danger => (
            colors::DANGER_BG,
            colors::DANGER_BORDER,
            colors::DANGER_TEXT,
        ),
    };
    let press = std::rc::Rc::new(on_press);
    let clicked = std::rc::Rc::clone(&press);
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(label.clone())
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
        .on_click(move |_, window, cx| {
            if !disabled {
                clicked(window, cx);
            }
        })
        .on_key_down(move |event, window, cx| {
            let key = event.keystroke.key.as_str();
            if !disabled && (key == "enter" || key == "space") {
                cx.stop_propagation();
                press(window, cx);
            }
        })
        .child(seen(label))
}
