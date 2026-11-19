//! How a screen reaches the console from GPUI, and how the app composes the screens.

use bandear_desktop_console::command::Owner;
use bandear_desktop_console::console::Console;
use bandear_desktop_console::router::{Route, Section};
use gpui::{Context, FocusHandle, Render, Window};

/// The console, as a GPUI entity the app keeps for the whole window. The app notifies it whenever
/// the console's state or meters changed, so every view that observes it draws again; the app also
/// runs every effect the console queues, so a screen only calls into the console.
pub struct Hub {
    console: Console,
    route: Route,
}

impl Hub {
    pub fn new(console: Console) -> Self {
        Self {
            console,
            route: Route::from(Section::DEFAULT),
        }
    }

    /// The console.
    pub fn console(&self) -> &Console {
        &self.console
    }

    /// The console, sending its commands for `section`'s screen.
    pub fn console_for(&self, section: Section) -> Console {
        self.console.for_owner(Owner::Screen(section))
    }

    /// Where the window is: the section shown, and for Processing the channel it opens on.
    pub fn route(&self) -> Route {
        self.route
    }

    /// Goes to `route`; the app moves the focus to the screen's heading (R6) and draws it.
    pub fn navigate(&mut self, route: Route, cx: &mut Context<Self>) {
        if self.route != route {
            self.route = route;
            cx.notify();
        }
    }
}

/// A screen of the console: one per section, in its own crate. The app builds each once, as the
/// window opens, and keeps it for the window's life (its flows and lanes run whether it is shown or
/// not); it draws only the section shown.
pub trait Screen: Render + Sized + 'static {
    /// The section the screen draws.
    const SECTION: Section;

    /// Builds the screen over the console `hub` holds. A screen observes `hub` to draw again when
    /// the console changes.
    fn new(hub: gpui::Entity<Hub>, window: &mut Window, cx: &mut Context<Self>) -> Self;

    /// The focus handle of the screen's heading: after a navigation (not the first paint) the
    /// focus moves there, so a screen reader starts there (R6).
    fn heading(&self) -> &FocusHandle;
}
