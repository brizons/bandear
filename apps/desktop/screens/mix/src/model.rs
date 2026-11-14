//! The Mix screen's model: its own state and flows, over the console. Pure: no window, tested
//! with plain `#[test]`s (the console's `testing` feature has a simulated desk to drive it).

use bandear_desktop_console::console::Console;

/// The screen's model, built once as the window opens and kept for its life, so its flows and
/// lanes go on while another screen is shown.
pub struct Model {
    console: Console,
}

impl Model {
    /// The model over `console` (a handle that sends its commands for this screen).
    pub fn new(console: Console) -> Self {
        Self { console }
    }

    /// The console this screen talks to the desk through.
    pub fn console(&self) -> &Console {
        &self.console
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bandear_desktop_console::command::Owner;
    use bandear_desktop_console::console::Options;
    use bandear_desktop_console::router::Section;

    #[test]
    fn the_model_sends_its_commands_for_its_screen() {
        let console = Console::new(Options::default()).for_owner(Owner::Screen(Section::Mix));
        let model = Model::new(console);
        assert_eq!(model.console().owner(), Owner::Screen(Section::Mix));
    }
}
