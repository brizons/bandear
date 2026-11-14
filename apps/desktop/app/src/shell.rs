//! The window's shell (`shell.js`, inventory A §1.2-1.6): the sidebar with the brand and the six
//! sections (an icon rail at 1099 px and narrower), the chips above every screen, and the screen
//! shown. Navigating moves the focus to the screen's heading (R6), never on the first paint.

use bandear_desktop_console::chips::Chip;
use bandear_desktop_console::demo_source::demo_chip;
use bandear_desktop_console::router::{Route, Section};
use bandear_desktop_console::state::ConsoleState;
use bandear_desktop_kit::assets::{BRAND_MARK, nav_icon};
use bandear_desktop_kit::gpui::{
    AnyView, App, AppContext as _, ClickEvent, Context, Entity, FocusHandle, InteractiveElement,
    IntoElement, KeyBinding, ParentElement, Render, Role, SharedString, StatefulInteractiveElement,
    Styled, Window, actions, div, prelude::FluentBuilder, px, svg,
};
use bandear_desktop_kit::hub::{Hub, Screen};
use bandear_desktop_kit::status::ChipLink;
use bandear_desktop_kit::theme::{
    FONT_SANS, RAIL_BREAKPOINT, RAIL_WIDTH, SIDEBAR_WIDTH, color, colors, radius, text, weight,
};
use bandear_desktop_kit::tooltip::tooltip;

use bandear_desktop_channels::ChannelsScreen;
use bandear_desktop_effects::EffectsScreen;
use bandear_desktop_mix::MixScreen;
use bandear_desktop_network::NetworkScreen;
use bandear_desktop_processing::ProcessingScreen;
use bandear_desktop_record::RecordScreen;

actions!(shell, [FocusNext, FocusPrevious]);

/// The key context of the shell, where Tab moves the focus.
const CONTEXT: &str = "Shell";

/// Binds Tab and Shift-Tab to the focus order (GPUI binds no keys of its own).
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("tab", FocusNext, Some(CONTEXT)),
        KeyBinding::new("shift-tab", FocusPrevious, Some(CONTEXT)),
    ]);
}

/// True when a window `width` px wide shows the sidebar as an icon rail.
pub fn shows_rail(width: f32) -> bool {
    width <= RAIL_BREAKPOINT
}

/// The chips above every screen, in order: the demo source, then the Record screen's (a take
/// recording, a soundcheck playing).
pub fn chips(state: &ConsoleState) -> Vec<Chip> {
    demo_chip(state.engine.status.as_ref())
        .into_iter()
        .chain(bandear_desktop_record::model::shell_chips(state))
        .collect()
}

/// The six screens, built once.
struct Screens {
    mix: Entity<MixScreen>,
    record: Entity<RecordScreen>,
    channels: Entity<ChannelsScreen>,
    processing: Entity<ProcessingScreen>,
    effects: Entity<EffectsScreen>,
    network: Entity<NetworkScreen>,
}

impl Screens {
    fn view(&self, section: Section) -> AnyView {
        match section {
            Section::Mix => self.mix.clone().into(),
            Section::Record => self.record.clone().into(),
            Section::Channels => self.channels.clone().into(),
            Section::Processing => self.processing.clone().into(),
            Section::Effects => self.effects.clone().into(),
            Section::Network => self.network.clone().into(),
        }
    }

    fn heading(&self, section: Section, cx: &App) -> FocusHandle {
        match section {
            Section::Mix => self.mix.read(cx).heading().clone(),
            Section::Record => self.record.read(cx).heading().clone(),
            Section::Channels => self.channels.read(cx).heading().clone(),
            Section::Processing => self.processing.read(cx).heading().clone(),
            Section::Effects => self.effects.read(cx).heading().clone(),
            Section::Network => self.network.read(cx).heading().clone(),
        }
    }
}

/// The window's root view.
pub struct Shell {
    hub: Entity<Hub>,
    screens: Screens,
    main: FocusHandle,
    /// The section drawn last, so a change of section (from the sidebar or from a link inside a
    /// screen) moves the focus once, while a screen that only moves its own route (Processing's
    /// open channel, as `replaceState` did) keeps the focus where it is.
    shown: Section,
}

impl Shell {
    pub fn new(hub: Entity<Hub>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe_in(&hub, window, |this, hub, window, cx| {
            let section = hub.read(cx).route().section;
            if section != this.shown {
                this.shown = section;
                let heading = this.screens.heading(section, cx);
                window.focus(&heading, cx);
            }
            cx.notify();
        })
        .detach();
        let shown = hub.read(cx).route().section;
        let screens = Screens {
            mix: cx.new(|cx| MixScreen::new(hub.clone(), window, cx)),
            record: cx.new(|cx| RecordScreen::new(hub.clone(), window, cx)),
            channels: cx.new(|cx| ChannelsScreen::new(hub.clone(), window, cx)),
            processing: cx.new(|cx| ProcessingScreen::new(hub.clone(), window, cx)),
            effects: cx.new(|cx| EffectsScreen::new(hub.clone(), window, cx)),
            network: cx.new(|cx| NetworkScreen::new(hub.clone(), window, cx)),
        };
        Self {
            hub,
            screens,
            main: cx.focus_handle(),
            shown,
        }
    }

    /// Opens `route`; the hub's observer moves the focus to its screen's heading (R6).
    pub fn navigate(&mut self, route: Route, _window: &mut Window, cx: &mut Context<Self>) {
        self.hub.update(cx, |hub, cx| hub.navigate(route, cx));
    }

    fn sidebar(&self, active: Section, rail: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let links = Section::ALL.into_iter().map(|section| {
            let current = section == active;
            let label: SharedString = section.label().into();
            let tip = label.clone();
            div()
                .id(SharedString::from(format!("nav-{}", section.id())))
                .role(Role::Link)
                .aria_label(label.clone())
                .when(current, |link| {
                    link.aria_selected(true).aria_description("Current section")
                })
                .focusable()
                .tab_stop(true)
                .when(rail, |link| {
                    link.tooltip(move |window, cx| tooltip(tip.clone(), window, cx))
                })
                .flex()
                .items_center()
                .when(rail, |link| link.justify_center().w(px(44.0)).mx_auto())
                .gap(px(10.0))
                .min_h(px(40.0))
                .px(px(12.0))
                .py(px(6.0))
                .rounded(px(radius::MD))
                .text_size(px(text::BASE))
                .cursor_pointer()
                .focus_visible(|style| style.border_2().border_color(color(colors::FOCUS)))
                .map(|link| {
                    if current {
                        link.bg(color(colors::SURFACE_RAISED))
                            .text_color(color(colors::TEXT))
                            .font_weight(weight::BOLD)
                    } else {
                        link.text_color(color(colors::TEXT_SECONDARY))
                            .font_weight(weight::MEDIUM)
                            .hover(|style| {
                                style
                                    .bg(color(colors::SURFACE))
                                    .text_color(color(colors::TEXT))
                            })
                    }
                })
                .on_click(cx.listener(move |shell, _: &ClickEvent, window, cx| {
                    shell.navigate(Route::from(section), window, cx);
                }))
                .child(
                    svg()
                        .path(nav_icon(section))
                        .flex_none()
                        .size(px(18.0))
                        .text_color(color(if current {
                            colors::ACCENT
                        } else {
                            colors::TEXT_SECONDARY
                        })),
                )
                .when(!rail, |link| {
                    link.child(bandear_desktop_kit::a11y::seen(label))
                })
        });
        div()
            .id("sidebar")
            .role(Role::Navigation)
            .aria_label("Sections")
            .flex()
            .flex_col()
            .flex_none()
            .gap(px(4.0))
            .w(px(if rail { RAIL_WIDTH } else { SIDEBAR_WIDTH }))
            .h_full()
            .overflow_y_scroll()
            .px(px(if rail { 10.0 } else { 12.0 }))
            .py(px(20.0))
            .border_r_1()
            .border_color(color(colors::BORDER_SUBTLE))
            .bg(color(colors::SIDEBAR))
            .child(
                // The brand: the mark (decorative) and the name, which the rail hides.
                div()
                    .flex()
                    .items_center()
                    .when(rail, |brand| brand.justify_center())
                    .gap(px(10.0))
                    .pt(px(6.0))
                    .px(px(10.0))
                    .pb(px(18.0))
                    .text_size(px(text::XL))
                    .font_weight(weight::BOLD)
                    .text_color(color(colors::TEXT))
                    .child(
                        svg()
                            .path(BRAND_MARK)
                            .flex_none()
                            .size(px(32.0))
                            .m(px(-6.0))
                            .ml(px(-9.0))
                            .text_color(color(colors::BRAND)),
                    )
                    .when(!rail, |brand| {
                        brand.child(bandear_desktop_kit::a11y::seen("BandEar"))
                    }),
            )
            .children(links)
    }

    fn chip_strip(&self, chips: Vec<Chip>, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_wrap()
            .justify_end()
            .gap(px(8.0))
            .pt(px(8.0))
            .px(px(22.0))
            .bg(color(colors::GROUND))
            .children(chips.into_iter().enumerate().map(|(index, chip)| {
                let shell = cx.entity().downgrade();
                ChipLink::new(("chip", index), chip).on_open(move |section, window, cx| {
                    // A shell that is gone has nowhere to go.
                    drop(shell.update(cx, |shell, cx| {
                        shell.navigate(Route::from(section), window, cx)
                    }));
                })
            }))
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (route, chips) = {
            let hub = self.hub.read(cx);
            let state = hub.console().state();
            (hub.route(), chips(&state))
        };
        let rail = shows_rail(f32::from(window.viewport_size().width));
        let main = self.main.clone();
        div()
            .id("shell")
            .key_context(CONTEXT)
            .on_action(|_: &FocusNext, window, cx| window.focus_next(cx))
            .on_action(|_: &FocusPrevious, window, cx| window.focus_prev(cx))
            .relative()
            .flex()
            .size_full()
            .bg(color(colors::GROUND))
            .text_color(color(colors::TEXT))
            .font_family(FONT_SANS)
            .text_size(px(text::BASE))
            .child(
                // The skip link (R7): off screen until focused; it takes the focus to the screen.
                div()
                    .id("skip")
                    .role(Role::Link)
                    .aria_label("Skip to content")
                    .focusable()
                    .tab_stop(true)
                    .absolute()
                    .left(px(8.0))
                    .top(px(-100.0))
                    .focus_visible(|style| style.top(px(8.0)))
                    .px(px(12.0))
                    .py(px(6.0))
                    .rounded(px(radius::MD))
                    .bg(color(colors::SURFACE_RAISED))
                    .text_color(color(colors::ACCENT))
                    .on_click(move |_: &ClickEvent, window, cx| window.focus(&main, cx))
                    .child(bandear_desktop_kit::a11y::seen("Skip to content")),
            )
            .child(self.sidebar(route.section, rail, cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .when(!chips.is_empty(), |body| {
                        body.child(self.chip_strip(chips, cx))
                    })
                    .child(
                        div()
                            .id("main")
                            .role(Role::Main)
                            .track_focus(&self.main)
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_h_0()
                            .gap(px(12.0))
                            .pt(px(14.0))
                            .px(px(22.0))
                            .pb(px(22.0))
                            .overflow_y_scroll()
                            .child(self.screens.view(route.section)),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use bandear_desktop_console::chips::ChipKind;
    use bandear_desktop_console::state::Action;
    use bandear_desktop_console::testing::sample_status;

    use super::*;

    #[test]
    fn the_sidebar_becomes_an_icon_rail_at_1099_px_and_narrower() {
        assert!(shows_rail(820.0), "the narrowest window");
        assert!(shows_rail(1099.0));
        assert!(!shows_rail(1100.0));
        assert!(!shows_rail(1180.0), "the window as it opens");
    }

    #[test]
    fn the_chips_show_the_demo_source_only_while_it_plays() {
        let mut state = ConsoleState::new();
        assert!(chips(&state).is_empty(), "no engine: nothing to say");
        let mut demo = sample_status();
        demo.status.demo = true;
        state.reduce(&Action::Status(Box::new(demo)));
        let shown: Vec<_> = chips(&state)
            .into_iter()
            .map(|chip| (chip.kind, chip.target))
            .collect();
        assert_eq!(shown, [(ChipKind::Demo, Section::Network)]);
    }
}
