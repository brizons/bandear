//! The Network and system screen's GPUI view.

use bandear_desktop_console::router::Section;
use bandear_desktop_kit::a11y::heading;
use bandear_desktop_kit::gpui::{
    Context, Entity, FocusHandle, IntoElement, ParentElement, Render, Styled, Window, div, px,
};
use bandear_desktop_kit::hub::{Hub, Screen};

use crate::model::Model;

/// The Network and system screen.
pub struct NetworkScreen {
    hub: Entity<Hub>,
    heading: FocusHandle,
    model: Model,
}

impl NetworkScreen {
    /// The screen's model.
    pub fn model(&self) -> &Model {
        &self.model
    }

    /// The hub the screen draws from.
    pub fn hub(&self) -> &Entity<Hub> {
        &self.hub
    }
}

impl Screen for NetworkScreen {
    const SECTION: Section = Section::Network;

    fn new(hub: Entity<Hub>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Draw again whenever the console changes.
        cx.observe(&hub, |_, _, cx| cx.notify()).detach();
        let model = Model::new(hub.read(cx).console_for(Self::SECTION));
        Self {
            hub,
            heading: cx.focus_handle(),
            model,
        }
    }

    fn heading(&self) -> &FocusHandle {
        &self.heading
    }
}

impl Render for NetworkScreen {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().flex().flex_col().gap(px(12.0)).child(heading(
            "network-heading",
            Self::SECTION.title(),
            &self.heading,
            false,
        ))
    }
}
