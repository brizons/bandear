//! Between the desk and the console: the pump's events into a channel the window reads (D3), and
//! the console's effects run (commands on GPUI's background executor, timers, the title).

use std::rc::Rc;
use std::sync::Arc;

use bandear_desktop_console::console::{Console, Effect};
use bandear_desktop_console::desk::execute;
use bandear_desktop_console::wire::EngineEvent;
use bandear_desktop_core::commands::DesktopState;
use bandear_desktop_core::events::PumpSink;
use bandear_desktop_kit::gpui::{AnyWindowHandle, App, AppContext as _, AsyncApp, Entity};
use bandear_desktop_kit::hub::Hub;
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use personal_monitoring::log_warn;
use serde::Serialize;

/// The pump's sink: each event read as the console reads it, on the pump's thread, then sent to
/// the window. Once the window is gone the events have nowhere to go and are dropped.
pub struct Sink(pub UnboundedSender<EngineEvent>);

impl PumpSink for Sink {
    fn emit<P: Serialize + Clone>(&self, event: &str, payload: P) {
        let value = match serde_json::to_value(payload) {
            Ok(value) => value,
            Err(error) => {
                log_warn!("events", "could not read `{event}`: {error}");
                return;
            }
        };
        match EngineEvent::from_json(event, value) {
            Ok(Some(event)) => {
                // A closed channel is a window that is gone: nothing to tell.
                drop(self.0.unbounded_send(event));
            }
            Ok(None) => {}
            Err(error) => log_warn!("events", "could not read `{event}`: {error}"),
        }
    }
}

/// Runs what the console asks for, on the window's thread: the commands go to the desk on the
/// background executor and their replies come back here, the timers wait there too.
pub struct Driver {
    console: Console,
    desk: Arc<DesktopState>,
    hub: Entity<Hub>,
    window: AnyWindowHandle,
}

impl Driver {
    /// Starts driving `console` over `desk`, drawing `hub` again when the console changes, and
    /// hearing the pump's `events`.
    pub fn start(
        console: Console,
        desk: Arc<DesktopState>,
        hub: Entity<Hub>,
        window: AnyWindowHandle,
        events: UnboundedReceiver<EngineEvent>,
        cx: &mut App,
    ) {
        let driver = Rc::new(Driver {
            console: console.clone(),
            desk,
            hub,
            window,
        });
        // The console calls its waker from inside its own steps, where nothing may call back
        // into it: a wake only asks the loop below to drain once more.
        let (wake, mut woken) = unbounded::<()>();
        console.set_waker(Rc::new(move || {
            drop(wake.unbounded_send(()));
        }));
        {
            let driver = Rc::clone(&driver);
            cx.spawn(async move |cx: &mut AsyncApp| {
                while woken.next().await.is_some() {
                    // Several wakes since the last drain are one drain.
                    while woken.try_recv().is_ok() {}
                    driver.drain(cx);
                }
            })
            .detach();
        }
        {
            let console = console.clone();
            let mut events = events;
            cx.spawn(async move |_: &mut AsyncApp| {
                while let Some(event) = events.next().await {
                    console.event(event);
                }
            })
            .detach();
        }
    }

    /// Runs every effect queued, then draws again if the console changed.
    fn drain(self: &Rc<Self>, cx: &mut AsyncApp) {
        for effect in self.console.take_effects() {
            match effect {
                Effect::Run {
                    ticket, command, ..
                } => {
                    let desk = Arc::clone(&self.desk);
                    let reply = cx.background_spawn(async move { execute(&desk, command) });
                    let console = self.console.clone();
                    cx.spawn(async move |_: &mut AsyncApp| {
                        console.answer(ticket, reply.await);
                    })
                    .detach();
                }
                Effect::Timer { ticket, after } => {
                    let console = self.console.clone();
                    let timer = cx.background_executor().timer(after);
                    cx.spawn(async move |_: &mut AsyncApp| {
                        timer.await;
                        console.fire(ticket);
                    })
                    .detach();
                }
                // The pump's channel is the window's from its first frame: listening always works.
                Effect::Listen { ticket } => self.console.listened(ticket, Ok(())),
                Effect::SetTitle(title) => {
                    let set = self
                        .window
                        .update(cx, |_, window, _| window.set_window_title(&title));
                    // The window may be closing; the title is a courtesy (C35).
                    if set.is_err() {
                        log_warn!("app", "could not set the window title to \"{title}\"");
                    }
                }
            }
        }
        if self.console.take_changed() {
            self.hub.update(cx, |_, cx| cx.notify());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use bandear_desktop_core::dto::{MetersDto, OutputLevelsDto};

    use super::*;

    #[test]
    fn a_pump_payload_reaches_the_window_as_the_console_reads_it() {
        let (sender, mut received) = unbounded();
        let sink = Sink(sender);
        let local = OutputLevelsDto {
            left: 40,
            right: 41,
            limiter_reduction_db: -1.5,
        };
        sink.emit(
            "meters",
            MetersDto {
                run: 3,
                channels: vec![50, 60],
                processed: vec![49, 59],
                gate_reduction_db: vec![0.0, 0.0],
                compressor_reduction_db: vec![0.0, 2.0],
                musicians: BTreeMap::from([("192.168.1.3:50000".to_owned(), local)]),
                local,
            },
        );
        sink.emit("something-else", serde_json::json!({}));

        let Ok(EngineEvent::Meters(meters)) = received.try_recv() else {
            panic!("the meters reach the window");
        };
        assert_eq!(meters.run, Some(3));
        assert_eq!(meters.channels, [50, 60]);
        assert_eq!(meters.local.limiter_reduction_db, -1.5);
        assert_eq!(meters.musicians["192.168.1.3:50000"].right, 41);
        assert!(
            received.try_recv().is_err(),
            "an event the console does not hear is dropped"
        );
    }
}
