//! The select and the text input, gpui-component's, in the console's theme and named for
//! assistive technology.

use gpui::{App, AppContext as _, Context, Entity, SharedString, Window};
use gpui_component::IndexPath;
use gpui_component::input::{Input, InputState};
use gpui_component::select::{Select, SelectItem, SelectState};

pub use gpui_component::input::InputEvent;
pub use gpui_component::select::SelectEvent;

/// One choice of a select: the value it stands for and the words it shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub value: SharedString,
    pub label: SharedString,
}

impl Choice {
    pub fn new(value: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
        }
    }
}

impl SelectItem for Choice {
    type Value = SharedString;

    fn title(&self) -> SharedString {
        self.label.clone()
    }

    fn value(&self) -> &SharedString {
        &self.value
    }
}

/// A select's state: its choices and the one picked. Subscribe to it for [SelectEvent]s.
pub type ChoiceState = SelectState<Vec<Choice>>;

/// The row of `value` among `choices`.
fn row_of(choices: &[Choice], value: Option<&str>) -> Option<IndexPath> {
    let value = value?;
    choices
        .iter()
        .position(|choice| choice.value.as_ref() == value)
        .map(|row| IndexPath::default().row(row))
}

/// A select's state over `choices` with `selected` picked.
pub fn choice_state<T: 'static>(
    choices: Vec<Choice>,
    selected: Option<&str>,
    window: &mut Window,
    cx: &mut Context<T>,
) -> Entity<ChoiceState> {
    let row = row_of(&choices, selected);
    cx.new(|cx| SelectState::new(choices, row, window, cx))
}

/// Replaces the choices of `state` and picks `selected` among them.
pub fn set_choices(
    state: &Entity<ChoiceState>,
    choices: Vec<Choice>,
    selected: Option<&str>,
    window: &mut Window,
    cx: &mut App,
) {
    let row = row_of(&choices, selected);
    state.update(cx, |state, cx| {
        state.set_items(choices, window, cx);
        if let Some(row) = row {
            state.set_selected_index(Some(row), window, cx);
        }
    });
}

/// The select of `state`, named `label`.
pub fn select(state: &Entity<ChoiceState>, label: impl Into<SharedString>) -> Select<Vec<Choice>> {
    Select::new(state).accessibility_label(label)
}

/// A text input's state, showing `placeholder` while empty. Subscribe to it for [InputEvent]s.
pub fn text_state<T: 'static>(
    placeholder: impl Into<SharedString>,
    window: &mut Window,
    cx: &mut Context<T>,
) -> Entity<InputState> {
    let placeholder = placeholder.into();
    cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
}

/// The text input of `state`, named `label`.
pub fn text_input(state: &Entity<InputState>, label: impl Into<SharedString>) -> Input {
    Input::new(state).aria_label(label)
}
