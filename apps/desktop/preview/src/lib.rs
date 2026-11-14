//! Opens one screen of the desktop over the simulated desk ([FakeDesk]), so whoever works on a
//! screen sees it draw and answer without the rest of the app: no engine starts, no audio input
//! opens and nothing goes on the network. The desk answers every command at once (a test's
//! [FakeDesk::script] answers the ones it does not know), sends its events after each answer and
//! sends meters while its engine runs.
//!
//! A screen crate keeps an example that runs it, and takes this crate as a dev-dependency only:
//!
//! ```ignore
//! use bandear_desktop_console::testing::FakeDesk;
//!
//! fn main() {
//!     bandear_desktop_preview::run::<bandear_desktop_mix::view::MixScreen>(FakeDesk::running());
//! }
//! ```
//!
//! `cargo run -p bandear-desktop-mix --example mix_preview` then opens the Mix screen alone.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use bandear_desktop_console::console::{Console, Effect, Options};
use bandear_desktop_console::router::Route;
use bandear_desktop_console::testing::FakeDesk;
use bandear_desktop_kit::gpui::{
    AnyView, App, AppContext as _, AsyncApp, Bounds, Context, Entity, InteractiveElement,
    IntoElement, ParentElement, Render, Role, StatefulInteractiveElement, Styled, TitlebarOptions,
    Window, WindowBounds, WindowOptions, div, px, size,
};
use bandear_desktop_kit::gpui_component::Root;
use bandear_desktop_kit::hub::{Hub, Screen};
use bandear_desktop_kit::theme::{FONT_SANS, color, colors, text};
use futures::StreamExt;
use futures::channel::mpsc::unbounded;

/// How often the simulated engine sends meters while it runs: often enough to see them move.
const METERS_EVERY: Duration = Duration::from_millis(100);

/// Opens screen `S` alone over `desk` and runs until its window closes.
pub fn run<S: Screen>(desk: FakeDesk) {
    gpui_platform::application()
        .with_assets(bandear_desktop_kit::assets::Assets)
        .run(move |cx: &mut App| {
            if let Err(error) = bandear_desktop_kit::init(cx) {
                eprintln!("the bundled fonts could not be loaded: {error}");
            }
            cx.on_window_closed(|cx, _| cx.quit()).detach();

            // The simulated desk starts nothing on its own: a preview shows the desk as given.
            let console = Console::new(Options {
                auto_start: false,
                ..Options::default()
            });
            let hub = cx.new(|_| Hub::new(console.clone()));
            hub.update(cx, |hub, cx| hub.navigate(Route::from(S::SECTION), cx));
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1180.0), px(780.0)),
                    cx,
                ))),
                window_min_size: Some(size(px(820.0), px(600.0))),
                titlebar: Some(TitlebarOptions {
                    title: Some(format!("BandEar preview — {}", S::SECTION.label()).into()),
                    ..TitlebarOptions::default()
                }),
                ..WindowOptions::default()
            };
            let screen_hub = hub.clone();
            let opened = cx.open_window(options, |window, cx| {
                let screen = cx.new(|cx| S::new(screen_hub, window, cx));
                let frame = cx.new(|_| Frame {
                    screen: screen.into(),
                });
                cx.new(|cx| Root::new(frame, window, cx))
            });
            if let Err(error) = opened {
                eprintln!("the preview window could not be opened: {error}");
                cx.quit();
                return;
            }
            drive(console.clone(), Rc::new(RefCell::new(desk)), hub, cx);
            let connect = console.connect();
            console.spawn(connect);
        });
}

/// The screen inside the ground, type and padding the app's shell gives its main area, with no
/// sidebar or chips, so a screen looks as it will in the app.
struct Frame {
    screen: AnyView,
}

impl Render for Frame {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .size_full()
            .bg(color(colors::GROUND))
            .text_color(color(colors::TEXT))
            .font_family(FONT_SANS)
            .text_size(px(text::BASE))
            .child(
                div()
                    .id("main")
                    .role(Role::Main)
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .gap(px(12.0))
                    .pt(px(14.0))
                    .px(px(22.0))
                    .pb(px(22.0))
                    .overflow_y_scroll()
                    .child(self.screen.clone()),
            )
    }
}

/// Runs the console's effects against the simulated desk, as the app's driver runs them against
/// the real one, and sends the desk's meters while its engine runs.
fn drive(console: Console, desk: Rc<RefCell<FakeDesk>>, hub: Entity<Hub>, cx: &mut App) {
    let (wake, mut woken) = unbounded::<()>();
    console.set_waker(Rc::new(move || {
        drop(wake.unbounded_send(()));
    }));
    {
        let console = console.clone();
        let desk = Rc::clone(&desk);
        cx.spawn(async move |cx: &mut AsyncApp| {
            while woken.next().await.is_some() {
                while woken.try_recv().is_ok() {}
                drain(&console, &desk, &hub, cx);
            }
        })
        .detach();
    }
    cx.spawn(async move |cx: &mut AsyncApp| {
        loop {
            cx.background_executor().timer(METERS_EVERY).await;
            if desk.borrow().running {
                desk.borrow_mut().emit_meters();
            }
            deliver(&console, &desk);
        }
    })
    .detach();
}

fn drain(console: &Console, desk: &Rc<RefCell<FakeDesk>>, hub: &Entity<Hub>, cx: &mut AsyncApp) {
    for effect in console.take_effects() {
        match effect {
            Effect::Run {
                ticket, command, ..
            } => {
                let reply = desk.borrow_mut().answer(&command);
                console.answer(ticket, reply);
                deliver(console, desk);
            }
            Effect::Timer { ticket, after } => {
                let console = console.clone();
                let timer = cx.background_executor().timer(after);
                cx.spawn(async move |_: &mut AsyncApp| {
                    timer.await;
                    console.fire(ticket);
                })
                .detach();
            }
            Effect::Listen { ticket } => console.listened(ticket, Ok(())),
            // The preview's window keeps its own title.
            Effect::SetTitle(_) => {}
        }
    }
    if console.take_changed() {
        hub.update(cx, |_, cx| cx.notify());
    }
}

/// Hands the console every event the desk has waiting, one at a time, so the desk is never
/// borrowed while the console hears an event.
fn deliver(console: &Console, desk: &Rc<RefCell<FakeDesk>>) {
    loop {
        let next = desk.borrow_mut().events.pop_front();
        let Some(event) = next else {
            break;
        };
        console.event(event);
    }
}
