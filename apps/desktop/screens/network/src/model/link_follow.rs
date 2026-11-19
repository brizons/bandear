//! The link health panel's wiring, without the window (`link-health-controller.js` and the
//! follower of `link-health-view.js`): whose panel is open, its range and its table, and the
//! follower that reads their history once a second while it is open.
//!
//! - Picking the open musician again closes the panel; picking another follows them instead (N18).
//! - The panel goes, and the follower stops, when the musician leaves the list (removed, or the
//!   engine stopped) and when the screen goes away (N19).
//! - A phone listed by its address that then registers changes key (`address:` to `client:`): the
//!   panel stays with the row at that address and follows its new key (N20).
//! - The first read is whole, then each second only what is new; a failed read keeps what was
//!   held, says why once, and tries again next second; an answer for a musician no longer followed
//!   is dropped (N21-N23). Range and table belong to the panel: closing hides the table but keeps
//!   the range (N24).

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use bandear_desktop_console::command::Command;
use bandear_desktop_console::console::Console;
use bandear_desktop_console::types::dto::LinkHistoryDto;
use bandear_desktop_console::wire::{LinkSample, Musician};

use super::clock::Zone;
use super::link_view::{
    DEFAULT_RANGE_MS, History, HistoryRead, PanelView, cursor_of, history_update, link_health_view,
};
use super::musicians::musician_key;

/// How often an open panel asks for what is new: one sample's period.
pub const HISTORY_POLL: Duration = Duration::from_millis(1000);

/// The answer of `link_history` as the panel reads it.
pub fn history_read(answer: &LinkHistoryDto) -> HistoryRead {
    HistoryRead {
        history_id: answer.history_id,
        max_samples: answer.max_samples,
        samples: answer
            .samples
            .iter()
            .map(|sample| LinkSample {
                seq: sample.seq,
                mono_ms: sample.mono_ms,
                at_ms: sample.at_ms,
                gap: sample.gap,
                reconnected: sample.reconnected,
                health: sample.health.to_owned(),
                interval_ms: sample.interval_ms,
                packets_received: sample.packets_received,
                packets_lost: sample.packets_lost,
                loss_percent: sample.loss_percent,
                underruns: sample.underruns,
                receive_timeouts: sample.receive_timeouts,
                rtt_ms: sample.rtt_ms,
                jitter_ms: sample.jitter_ms,
                cushion_ms: sample.cushion_ms,
                backlog_ms: sample.backlog_ms,
                recovered: sample.recovered,
            })
            .collect(),
    }
}

/// The listed musician the panel follows by `key`, last seen at `address`: when an `address:` key
/// is gone, the row at that same address that now has a client id is the same phone; a phone that
/// already had a client id is never followed to another one at its old address.
pub fn followed_musician<'a>(
    musicians: &'a [Musician],
    key: &str,
    address: Option<&str>,
) -> Option<&'a Musician> {
    let same = musicians
        .iter()
        .find(|musician| musician_key(musician) == key);
    if same.is_some() || !key.starts_with("address:") {
        return same;
    }
    musicians.iter().find(|musician| {
        musician
            .client_id
            .as_deref()
            .is_some_and(|id| !id.is_empty())
            && Some(musician.address.as_str()) == address
    })
}

/// What the follower tells of: the history held, and why the last read failed.
pub type OnHistory = Rc<dyn Fn(Option<History>, Option<String>)>;

#[derive(Default)]
struct Following {
    key: Option<String>,
    buffer: Option<History>,
    error: Option<String>,
    /// Bumped by every stop, so a read that comes back for an older follow is dropped.
    generation: u64,
}

/// Follows one musician's history: read whole, then each second only what is new.
#[derive(Clone)]
pub struct Follower {
    console: Console,
    state: Rc<RefCell<Following>>,
    on_change: OnHistory,
}

impl Follower {
    pub fn new(console: Console, on_change: OnHistory) -> Self {
        Self {
            console,
            state: Rc::new(RefCell::new(Following::default())),
            on_change,
        }
    }

    /// Follows the musician `key` names (`None` for nobody); the same one again changes nothing.
    pub fn follow(&self, key: Option<String>) {
        if self.state.borrow().key == key {
            return;
        }
        let generation = {
            let mut state = self.state.borrow_mut();
            state.generation += 1;
            state.key = key.clone();
            state.buffer = None;
            state.error = None;
            state.generation
        };
        if key.is_some() {
            self.run(generation);
        }
    }

    /// Stops following: no more reads, and a late answer is dropped.
    pub fn stop(&self) {
        let mut state = self.state.borrow_mut();
        state.generation += 1;
        state.key = None;
        state.buffer = None;
        state.error = None;
    }

    /// The history held now.
    pub fn buffer(&self) -> Option<History> {
        self.state.borrow().buffer.clone()
    }

    fn current(&self, generation: u64) -> bool {
        self.state.borrow().generation == generation
    }

    fn run(&self, generation: u64) {
        let follower = self.clone();
        self.console.spawn(async move {
            while follower.current(generation) {
                follower.poll(generation).await;
                if !follower.current(generation) {
                    return;
                }
                follower.console.sleep(HISTORY_POLL).await;
            }
        });
    }

    fn read(
        &self,
        key: String,
        since: Option<u64>,
    ) -> impl std::future::Future<Output = Result<HistoryRead, String>> + 'static {
        let call = self.console.call::<LinkHistoryDto>(Command::LinkHistory {
            musician_key: key,
            since_seq: since,
        });
        async move { call.await.map(|answer| history_read(&answer)) }
    }

    async fn poll(&self, generation: u64) {
        let (key, since) = {
            let state = self.state.borrow();
            let Some(key) = state.key.clone() else {
                return;
            };
            (key, cursor_of(state.buffer.as_ref()))
        };
        let mut outcome = self.read(key.clone(), since).await.map(|read| {
            let state = self.state.borrow();
            history_update(state.buffer.as_ref(), &read, since)
        });
        if let Ok(update) = &outcome
            && update.refetch
            && self.current(generation)
        {
            outcome = self.read(key, None).await.map(|read| {
                let state = self.state.borrow();
                history_update(state.buffer.as_ref(), &read, None)
            });
        }
        if !self.current(generation) {
            return;
        }
        let heard = {
            let mut state = self.state.borrow_mut();
            match outcome {
                Ok(update) => {
                    let said = state.error.take();
                    state.buffer = update.buffer;
                    (update.changed || said.is_some()).then(|| (state.buffer.clone(), None))
                }
                Err(error) => (state.error.as_ref() != Some(&error)).then(|| {
                    state.error = Some(error.clone());
                    (state.buffer.clone(), Some(error))
                }),
            }
        };
        if let Some((buffer, error)) = heard {
            (self.on_change)(buffer, error);
        }
    }
}

struct Open {
    key: Option<String>,
    address: Option<String>,
    range_ms: u64,
    show_table: bool,
    buffer: Option<History>,
    error: Option<String>,
}

impl Default for Open {
    fn default() -> Self {
        Self {
            key: None,
            address: None,
            range_ms: DEFAULT_RANGE_MS,
            show_table: false,
            buffer: None,
            error: None,
        }
    }
}

/// The panel's controller.
#[derive(Clone)]
pub struct LinkHealth {
    open: Rc<RefCell<Open>>,
    follower: Follower,
}

impl LinkHealth {
    /// A closed panel whose history `console` reads; `changed` hears of every new sample and of
    /// each failed read, so the screen draws again.
    pub fn new(console: &Console, changed: Rc<dyn Fn()>) -> Self {
        let open = Rc::new(RefCell::new(Open::default()));
        let kept = Rc::clone(&open);
        let follower = Follower::new(
            console.clone(),
            Rc::new(move |buffer, error| {
                {
                    let mut open = kept.borrow_mut();
                    open.buffer = buffer;
                    open.error = error;
                }
                changed();
            }),
        );
        Self { open, follower }
    }

    /// Closes the panel: the follower stops, the table hides, the range stays.
    pub fn close(&self) {
        self.follower.stop();
        let mut open = self.open.borrow_mut();
        open.key = None;
        open.address = None;
        open.buffer = None;
        open.error = None;
        open.show_table = false;
    }

    fn follow(&self, musician: &Musician) {
        let key = musician_key(musician);
        {
            let mut open = self.open.borrow_mut();
            open.key = Some(key.clone());
            open.address = Some(musician.address.clone());
        }
        self.follower.follow(Some(key));
    }

    /// The open musician as listed now, following a registration to its new key; closes the panel
    /// when they are no longer listed.
    fn current(&self, musicians: &[Musician]) -> Option<Musician> {
        let (key, address) = {
            let open = self.open.borrow();
            (open.key.clone()?, open.address.clone())
        };
        let Some(musician) = followed_musician(musicians, &key, address.as_deref()) else {
            self.close();
            return None;
        };
        if musician_key(musician) != key {
            // The same phone, now registered: the history held stays until the new key's read.
            self.follow(musician);
        }
        self.open.borrow_mut().address = Some(musician.address.clone());
        Some(musician.clone())
    }

    /// Opens the panel of the musician listed at `address`, or closes it if they are the open one.
    pub fn select(&self, musicians: &[Musician], address: &str) {
        let shown = self.current(musicians);
        let picked = musicians
            .iter()
            .find(|musician| musician.address == address);
        self.close();
        if let Some(picked) = picked
            && shown
                .as_ref()
                .is_none_or(|shown| musician_key(shown) != musician_key(picked))
        {
            self.follow(picked);
        }
    }

    pub fn set_range(&self, range_ms: u64) {
        self.open.borrow_mut().range_ms = range_ms;
    }

    pub fn show_table(&self, shown: bool) {
        self.open.borrow_mut().show_table = shown;
    }

    /// Closes the panel when the musician it shows is no longer listed (a status went by).
    pub fn follow_list(&self, musicians: &[Musician]) {
        self.current(musicians);
    }

    /// What to draw for `musicians`: the open musician's address for the table, and the panel;
    /// both `None` while it is closed.
    pub fn view(&self, musicians: &[Musician], zone: Zone) -> (Option<String>, Option<PanelView>) {
        let Some(musician) = self.current(musicians) else {
            return (None, None);
        };
        let open = self.open.borrow();
        let panel = link_health_view(
            &musician,
            open.buffer.as_ref(),
            open.range_ms,
            open.error.as_deref(),
            open.show_table,
            zone,
        );
        (Some(musician.address), Some(panel))
    }
}

#[cfg(test)]
#[path = "link_follow_tests.rs"]
mod tests;
