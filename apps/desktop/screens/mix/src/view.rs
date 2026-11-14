//! The Mix screen's GPUI view (`mix-screen.js`): it draws the model's [Page] and hands the
//! engineer's moves to the model. Top to bottom: the visually hidden heading, the top bar, the tabs
//! with the tools, the message lines, then the notice or the open mix (the channel strips beside
//! the group strips and the master; under 860 px the side goes under the channels).

mod atoms;
mod master;
mod strips;
mod tools;

use std::collections::HashMap;

use bandear_desktop_console::router::{Route, Section};
use bandear_desktop_kit::a11y::{heading, seen};
use bandear_desktop_kit::gpui::{
    AnyElement, App, ClickEvent, Context, Entity, FocusHandle, InteractiveElement, IntoElement,
    KeyDownEvent, ParentElement, Render, Role, SharedString, StatefulInteractiveElement, Styled,
    WeakEntity, Window, div, prelude::FluentBuilder, px,
};
use bandear_desktop_kit::hub::{Hub, Screen};
use bandear_desktop_kit::status::{LinkMark, Notice, NoticeKind};
use bandear_desktop_kit::theme::{TARGET_PRIMARY, color, colors, radius, text, weight};
use bandear_desktop_kit::tooltip::tooltip;
use bandear_desktop_kit::top_bar::{EngineAction, TopBar, TopBarData};
use futures::StreamExt;

use crate::mix_view::{MixNotice, MixTab, NoticeAction, TabLevel};
use crate::model::{Body, Model, Page};

/// Under this window width the groups and the master go under the channels.
const NARROW: f32 = 860.0;

/// The Mix screen.
pub struct MixScreen {
    hub: Entity<Hub>,
    heading: FocusHandle,
    model: Model,
    /// The Mix section shows (the screen lives on while another does).
    shown: bool,
    /// Each tab's focus, by the address it opens: the tabs move the focus with the arrows.
    tabs: HashMap<String, FocusHandle>,
    tools: tools::ToolsParts,
}

impl MixScreen {
    /// The screen's model.
    pub fn model(&self) -> &Model {
        &self.model
    }

    /// The hub the screen draws from.
    pub fn hub(&self) -> &Entity<Hub> {
        &self.hub
    }
}

impl Screen for MixScreen {
    const SECTION: Section = Section::Mix;

    fn new(hub: Entity<Hub>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let model = Model::new(hub.read(cx).console_for(Self::SECTION));
        let shown = hub.read(cx).route().section == Self::SECTION;
        // Draw again whenever the console changes; leaving the screen and coming back starts the
        // tools afresh and drops what late answers would have said, as a new mount did.
        cx.observe(&hub, |this: &mut Self, hub, cx| {
            let shown = hub.read(cx).route().section == Self::SECTION;
            if shown != this.shown {
                this.shown = shown;
                if shown {
                    this.model.enter();
                } else {
                    this.model.leave();
                }
            }
            cx.notify();
        })
        .detach();
        // The model's own changes (an outcome, the tools' answers) draw the screen again too.
        let (wake, mut woken) = futures::channel::mpsc::unbounded::<()>();
        model.on_change(move || drop(wake.unbounded_send(())));
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            while woken.next().await.is_some() {
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        })
        .detach();
        let tools = tools::ToolsParts::new(window, cx);
        Self {
            hub,
            heading: cx.focus_handle(),
            model,
            shown,
            tabs: HashMap::new(),
            tools,
        }
    }

    fn heading(&self) -> &FocusHandle {
        &self.heading
    }
}

impl MixScreen {
    fn navigate(hub: &Entity<Hub>, route: Route, cx: &mut App) {
        hub.update(cx, |hub, cx| hub.navigate(route, cx));
    }

    fn top_bar(&self, page: &Page) -> AnyElement {
        let bar = &page.top_bar;
        let data = TopBarData {
            phase: Some(bar.phase),
            device: bar.device.clone().map(SharedString::from),
            format: bar.format.clone().unwrap_or_default().into(),
            attention: bar.attention_label.clone().map(SharedString::from),
            action: bar.action,
        };
        let (hub, model) = (self.hub.clone(), self.model.clone());
        TopBar::new(data)
            .on_attention(move |_, cx| Self::navigate(&hub, Route::from(Section::Network), cx))
            .on_engine(move |action, _, _| match action {
                EngineAction::Start => model.start(),
                EngineAction::Stop => model.stop(),
            })
            .into_any_element()
    }

    fn tab_focus(&mut self, address: &str, cx: &mut Context<Self>) -> FocusHandle {
        self.tabs
            .entry(address.to_owned())
            .or_insert_with(|| cx.focus_handle())
            .clone()
    }

    /// The tabs (`musician-tabs.js`): one per musician and the Local mix; the open one in the tab
    /// order (the first while none is), the arrows, Home and End moving the focus and opening.
    fn tabs(&mut self, tabs: &[MixTab], cx: &mut Context<Self>) -> AnyElement {
        let any_open = tabs.iter().any(|tab| tab.selected);
        let addresses: Vec<String> = tabs.iter().map(|tab| tab.address.clone()).collect();
        let handles: Vec<FocusHandle> = addresses
            .iter()
            .map(|address| self.tab_focus(address, cx))
            .collect();
        let row = div()
            .id("tabs")
            .role(Role::TabList)
            .aria_label("Whose mix")
            .flex()
            .flex_wrap()
            .gap(px(6.0))
            .flex_1()
            .min_w(px(320.0));
        tabs.iter()
            .enumerate()
            .fold(row, |row, (index, tab)| {
                let in_order = tab.selected || (!any_open && index == 0);
                let (model, address) = (self.model.clone(), tab.address.clone());
                let (keys, here, handles) = (self.model.clone(), index, handles.clone());
                let addresses = addresses.clone();
                let detail = SharedString::from(tab.link.detail.clone());
                let level = match tab.link.level {
                    TabLevel::Good => bandear_desktop_console::format::LinkLevel::Good,
                    TabLevel::Warning => bandear_desktop_console::format::LinkLevel::Warning,
                    TabLevel::Bad => bandear_desktop_console::format::LinkLevel::Bad,
                    TabLevel::Unknown | TabLevel::Local => {
                        bandear_desktop_console::format::LinkLevel::Unknown
                    }
                };
                row.child(
                    div()
                        .id(SharedString::from(format!("tab-{}", tab.address)))
                        .role(Role::Tab)
                        .aria_label(SharedString::from(format!(
                            "{} {}",
                            tab.label, tab.link.label
                        )))
                        .aria_selected(tab.selected)
                        .track_focus(&handles[index].clone().tab_stop(in_order))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .min_h(px(TARGET_PRIMARY))
                        .px(px(14.0))
                        .rounded(px(radius::MD))
                        .text_size(px(text::BASE))
                        .font_weight(weight::BOLD)
                        .cursor_pointer()
                        .map(|tab_element| {
                            if tab.selected {
                                tab_element
                                    .border_2()
                                    .border_color(color(colors::ACCENT))
                                    .bg(color(colors::SURFACE_RAISED))
                                    .border_b_4()
                            } else {
                                tab_element
                                    .border_1()
                                    .border_color(color(colors::BORDER))
                                    .hover(|style| style.border_color(color(colors::TEXT_MUTED)))
                            }
                        })
                        .focus_visible(|style| style.border_color(color(colors::FOCUS)))
                        .tooltip(move |window, cx| tooltip(detail.clone(), window, cx))
                        .on_click(move |_: &ClickEvent, _, _| model.select(&address))
                        .on_key_down(move |event: &KeyDownEvent, window, cx| {
                            let count = addresses.len();
                            let next = match event.keystroke.key.as_str() {
                                "left" | "up" => (here + count - 1) % count,
                                "right" | "down" => (here + 1) % count,
                                "home" => 0,
                                "end" => count - 1,
                                _ => return,
                            };
                            cx.stop_propagation();
                            window.focus(&handles[next], cx);
                            keys.select(&addresses[next]);
                        })
                        .child(seen(tab.label.clone()))
                        .child(
                            div()
                                .text_color(color(colors::TEXT_MUTED))
                                .text_size(px(text::SM))
                                .child(
                                    LinkMark::new(
                                        SharedString::from(format!("link-{}", tab.address)),
                                        level,
                                    )
                                    .label(tab.link.label.clone())
                                    .detail(tab.link.detail.clone()),
                                ),
                        ),
                )
            })
            .into_any_element()
    }

    /// The sentences above the strips, in their fixed order.
    fn messages(&self, page: &Page) -> Vec<AnyElement> {
        let mut lines = Vec::new();
        if let Some(said) = &page.outcome {
            let id = if said.ok { "tools-done" } else { "tools-error" };
            lines.push(atoms::message_line(id, !said.ok, said.message.clone()));
        }
        let messages = &page.messages;
        for (id, alert, said) in [
            ("departed", false, &messages.departed),
            ("error", true, &messages.error),
            ("controls", false, &messages.controls),
            ("command", true, &messages.command),
            ("audio", false, &messages.audio),
        ] {
            if let Some(said) = said {
                lines.push(atoms::message_line(id, alert, said.clone()));
            }
        }
        lines
            .into_iter()
            .map(IntoElement::into_any_element)
            .collect()
    }

    fn notice(&self, notice: &MixNotice) -> AnyElement {
        let mut card = Notice::new("notice", NoticeKind::Status, notice.title());
        if notice.links_network() {
            let hub = self.hub.clone();
            let words = notice.text().unwrap_or_default();
            card = card.child(
                div()
                    .id("notice-text")
                    .role(Role::Label)
                    .aria_label(SharedString::from(format!("{words}Network and system.")))
                    .flex()
                    .flex_wrap()
                    .text_size(px(text::BASE))
                    .text_color(color(colors::TEXT_SECONDARY))
                    .child(seen(words))
                    .child(
                        div()
                            .id("notice-network")
                            .role(Role::Link)
                            .aria_label("Network and system")
                            .focusable()
                            .tab_stop(true)
                            .text_color(color(colors::ACCENT))
                            .underline()
                            .cursor_pointer()
                            .on_click(move |_: &ClickEvent, _, cx| {
                                Self::navigate(&hub, Route::from(Section::Network), cx)
                            })
                            .child(seen("Network and system")),
                    )
                    .child(seen(".")),
            );
        } else if let Some(words) = notice.text() {
            card = card.text(words);
        }
        if let Some((label, action)) = notice.action() {
            let model = self.model.clone();
            card = card.action(label, move |_, _| match action {
                NoticeAction::Reconnect => model.reconnect(),
                NoticeAction::Start => model.start(),
            });
        }
        div()
            .max_w(px(560.0))
            .mt(px(24.0))
            .child(card)
            .into_any_element()
    }

    /// The open mix: the channel strips (scrolling sideways), then the group strips and the master.
    fn body(&self, body: &Body, narrow: bool) -> AnyElement {
        let channels = body.strips.iter().zip(&body.strip_levels).fold(
            div()
                .id("channels")
                .role(Role::Group)
                .aria_label("Channels")
                .flex()
                .gap(px(6.0))
                .min_w_0()
                .min_h(px(320.0))
                .pb(px(6.0))
                .overflow_x_scroll(),
            |channels, (strip, level)| {
                channels.child(strips::channel_strip(strip, *level, &self.model, &self.hub))
            },
        );
        let side = body.groups.iter().fold(
            div()
                .id("side")
                .role(Role::Group)
                .aria_label("Groups and master")
                .flex()
                .flex_none()
                .gap(px(6.0))
                .min_h(px(320.0))
                .border_color(color(colors::BORDER_SUBTLE)),
            |side, group| side.child(strips::group_strip(group, &self.model)),
        );
        let side = side.child(master::master_strip(body, &self.model));
        let side = if narrow {
            side.pt(px(10.0)).border_t_1().overflow_x_scroll()
        } else {
            side.pl(px(10.0)).border_l_1()
        };
        div()
            .id("body")
            .role(Role::TabPanel)
            .aria_label(SharedString::from(body.label.clone()))
            .flex()
            .flex_1()
            .gap(px(10.0))
            .items_stretch()
            .min_h(px(320.0))
            .map(|row| if narrow { row.flex_col() } else { row })
            .child(if narrow { channels } else { channels.flex_1() })
            .child(side)
            .into_any_element()
    }
}

impl Render for MixScreen {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_tools(window, cx);
        let page = self.model.page();
        let narrow = f32::from(window.viewport_size().width) <= NARROW;
        let tabs = self.tabs(&page.tabs, cx);
        let tools = if page.tools {
            Some(self.tools_row(cx))
        } else {
            None
        };
        let top_bar = self.top_bar(&page);
        let messages = self.messages(&page);
        let notice = page.notice.as_ref().map(|notice| self.notice(notice));
        let body = page.body.as_ref().map(|body| self.body(body, narrow));
        // The screen fills the window's height, as the web screen filled the page (`100vh`).
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap(px(12.0))
            .child(heading(
                "mix-heading",
                Self::SECTION.title(),
                &self.heading,
                true,
            ))
            .child(top_bar)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_start()
                    .gap(px(8.0))
                    .pb(px(10.0))
                    .border_b_1()
                    .border_color(color(colors::BORDER_SUBTLE))
                    .child(tabs)
                    .when_some(tools, |row, tools| row.child(tools)),
            )
            .children(messages)
            .children(notice)
            .children(body)
    }
}
