//! The BandEar desktop app: a native GPUI window over the desktop core (D1).
//!
//! [run] opens the desk as the Tauri shell did ([bandear_desktop_core::lifecycle::open]), streams the
//! running engine's events into the window ([bridge::Sink]), runs the console's effects on GPUI's
//! executors ([bridge::Driver]) and draws the [shell::Shell] with the six screens. On quit it does
//! the desk's exit work ([bandear_desktop_core::lifecycle::exit]).

pub mod bridge;
pub mod shell;

use std::sync::{Arc, OnceLock, Weak};

use bandear_desktop_console::console::{Console, Options};
use bandear_desktop_core::commands::DesktopState;
use bandear_desktop_core::events::EventPump;
use bandear_desktop_core::folders::AppFolders;
use bandear_desktop_core::lifecycle;
use bandear_desktop_kit::gpui::{
    App, AppContext as _, Bounds, KeyBinding, Menu, MenuItem, TitlebarOptions, WindowBounds,
    WindowOptions, actions, px, size,
};
use bandear_desktop_kit::gpui_component::Root;
use bandear_desktop_kit::hub::Hub;
use personal_monitoring::{log_error, log_warn};

actions!(bandear, [Quit]);

/// The app's version, as the log's first line names it.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The window's title before the console names the device (C35).
pub const TITLE: &str = "BandEar Server";

/// The desk, kept for the whole process and never dropped. The event pump reads it through a weak
/// reference it upgrades for each read, and the desk owns the pump: were the window's last strong
/// reference let go while a read holds one, the pump's thread would drop the desk and join itself.
/// This one is never let go, so the last reference is never the pump's.
static DESK: OnceLock<Arc<DesktopState>> = OnceLock::new();

/// Opens the desk in `folders`, its every engine streaming to `events`.
fn open_desk(
    folders: &AppFolders,
    events: futures::channel::mpsc::UnboundedSender<bandear_desktop_console::wire::EngineEvent>,
) -> Arc<DesktopState> {
    Arc::new_cyclic(|desk: &Weak<DesktopState>| {
        let desk = desk.clone();
        lifecycle::open(folders, VERSION).with_event_pump(move |bus, run| {
            EventPump::start(bus, run, desk.clone(), bridge::Sink(events.clone()))
        })
    })
}

/// Set (to anything) to open the window without starting an engine when none runs, for a look at
/// the app that opens no audio input and sends nothing on the network.
pub const NO_AUTO_START: &str = "BANDEAR_NO_AUTO_START";

/// The console's options: the automatic start of the old window, unless [NO_AUTO_START] is set.
pub fn console_options() -> Options {
    Options {
        auto_start: std::env::var_os(NO_AUTO_START).is_none(),
        ..Options::default()
    }
}

/// Runs the app until it quits.
pub fn run() {
    // The desk's folders, named as Tauri named them; with no config folder the session has nowhere
    // to live and the app does not start, as the Tauri shell did not.
    let folders = match AppFolders::resolve() {
        Ok(folders) => folders,
        Err(error) => {
            eprintln!("BandEar cannot start: {error}");
            std::process::exit(1);
        }
    };
    let (events, heard) = futures::channel::mpsc::unbounded();
    let desk = Arc::clone(DESK.get_or_init(|| open_desk(&folders, events)));

    gpui_platform::application()
        .with_assets(bandear_desktop_kit::assets::Assets)
        .run(move |cx: &mut App| {
            if let Err(error) = bandear_desktop_kit::init(cx) {
                log_warn!("app", "the bundled fonts could not be loaded: {error}");
            }
            shell::bind_keys(cx);
            cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
            cx.on_action(|_: &Quit, cx| cx.quit());
            cx.set_menus([Menu {
                name: "BandEar".into(),
                items: vec![MenuItem::action("Quit BandEar", Quit)],
                disabled: false,
            }]);
            // Closing the window quits the app, as it did under Tauri.
            cx.on_window_closed(|cx, _| cx.quit()).detach();
            {
                let desk = Arc::clone(&desk);
                cx.on_app_quit(move |_| {
                    // The local mix, the channel processing and a mix backup still waiting are
                    // written, then the log is flushed, with the desk the whole process keeps.
                    let desk = Arc::clone(&desk);
                    async move { lifecycle::exit(Some(&desk)) }
                })
                .detach();
            }

            let console = Console::new(console_options());
            let hub = cx.new(|_| Hub::new(console.clone()));
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1180.0), px(780.0)),
                    cx,
                ))),
                window_min_size: Some(size(px(820.0), px(600.0))),
                titlebar: Some(TitlebarOptions {
                    title: Some(TITLE.into()),
                    ..TitlebarOptions::default()
                }),
                app_id: Some(bandear_desktop_core::folders::APP_IDENTIFIER.to_owned()),
                ..WindowOptions::default()
            };
            let shell_hub = hub.clone();
            let window = cx.open_window(options, |window, cx| {
                let shell = cx.new(|cx| shell::Shell::new(shell_hub, window, cx));
                cx.new(|cx| Root::new(shell, window, cx))
            });
            let window = match window {
                Ok(window) => window,
                Err(error) => {
                    log_error!("app", "the window could not be opened: {error}");
                    cx.quit();
                    return;
                }
            };
            bridge::Driver::start(console.clone(), desk, hub, window.into(), heard, cx);
            let connect = console.connect();
            console.spawn(connect);
        });
}
