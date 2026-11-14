//! The BandEar desktop's GPUI kit: the web console's tokens, fonts, icons and shared controls,
//! drawn with GPUI and gpui-component.
//!
//! [init] sets the kit up as the app starts (gpui-component, the fonts, the theme); [assets::Assets]
//! is the app's asset source. A screen crate builds on [hub::Hub] (the console, as an entity) and
//! implements [hub::Screen]. The controls take plain data and callbacks and keep no state but their
//! gesture's: the values and their maths come from the console crate.
//!
//! - [slider]: [slider::fader], [slider::pan] and [slider::parameter], over one gesture (C39, V7).
//! - [meters]: [meters::LevelMeter], [meters::stereo_meter] and [meters::ReductionMeter].
//! - [buttons]: [buttons::Button], [buttons::ToggleButton] (Mute, Solo) and
//!   [buttons::ConfirmButton].
//! - [status]: [status::StatusPill], [status::ChipLink], [status::Notice], [status::GroupChip],
//!   [status::LinkMark] and [status::warning_icon].
//! - [forms]: the select and the text input.
//! - [top_bar]: the Mix screen's top bar.
//! - [a11y]: headings, visually hidden text, toggled states.

pub mod a11y;
pub mod assets;
pub mod buttons;
pub mod forms;
pub mod hub;
pub mod meters;
pub mod slider;
pub mod status;
pub mod theme;
pub mod tooltip;
pub mod top_bar;

/// Sets the kit up: gpui-component, the kit's fonts and the console's theme on the components.
pub fn init(cx: &mut gpui::App) -> gpui::Result<()> {
    gpui_component::init(cx);
    theme::apply_to_components(cx);
    theme::load_fonts(cx)
}
