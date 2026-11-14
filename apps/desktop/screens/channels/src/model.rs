//! The Channels and groups screen's model (`channelsScreenCommands` in `channels-view.js`, and the
//! channel and group edits of `console-app.js`): its own state and flows, over the console. Pure:
//! no window, tested with plain `#[test]`s over the console's simulated desk.
//!
//! - Channel and group edits run on the console's serial desk lane ([Console::in_turn], C29), one
//!   at a time across every screen, each counted among the console's unsaved edits until it is
//!   answered. A channel edit is built on the whole table the desk keeps, read afresh
//!   (`channel_table`, inputs past the device included); a group edit on the last `set_groups`
//!   answer, else the status's groups, else it is refused (C30). Each answers in words and is
//!   followed by a status read.
//! - A loaded session's groups are the desk's: the next group edit builds on the status again.
//! - A Shown box or a group box the engineer clicked is held as clicked until its edit is answered
//!   (CH11), so a status that still carries the old value does not flip it back.

use std::cell::RefCell;
use std::future::Future;
use std::rc::Rc;

use bandear_desktop_console::command::Command;
use bandear_desktop_console::console::Console;
use bandear_desktop_console::format::{
    format_channels_set, format_groups_set, sentence_with_reason,
};
use bandear_desktop_console::state::Action;
use bandear_desktop_console::types::SessionChannel;
use bandear_desktop_console::types::dto::{ChannelRequest, ChannelsSetDto, GroupsSetDto};

use crate::groups::{
    GroupCard, GroupDefinition, PendingToggles, ToggleKey, add_group, can_add_group, delete_group,
    group_cards, group_definitions, group_definitions_of, rename_group, set_group_members,
    with_pending_members, with_pending_shown,
};
use crate::table::{
    ChannelRow, channel_rows, rename_channel, set_channel_hidden, set_channel_kind,
    set_channel_link,
};

/// What an edit did, in words: the status line's when it worked, the alert's when it did not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Said {
    pub ok: bool,
    pub message: String,
}

impl Said {
    fn refused(lead: &str, reason: &str) -> Self {
        Self {
            ok: false,
            message: sentence_with_reason(lead, Some(reason)),
        }
    }
}

/// Why a channel edit changed nothing, before the reason.
pub const CHANNELS_REFUSED: &str = "The channel details were not changed";
/// Why a group edit changed nothing, before the reason.
pub const GROUPS_REFUSED: &str = "The groups were not changed";
/// Why a group edit cannot be built while no engine runs and no answer was kept.
pub const GROUPS_UNKNOWN: &str =
    "no engine is running, so this desk does not know them yet; start the engine first";

/// One of the screen's edits, as its controls ask for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Edit {
    /// Input `index` (0-based) named `name`.
    Rename {
        index: usize,
        name: String,
    },
    /// Input `index` of instrument `kind`.
    Kind {
        index: usize,
        kind: String,
    },
    /// Input `index` paired with the next input, or its pair undone.
    Link {
        index: usize,
        linked: bool,
    },
    /// Input `index` (and its pair partner) shown to musicians or hidden.
    Shown {
        index: usize,
        shown: bool,
    },
    /// A new group, "Group 3" or the next free number.
    GroupAdd,
    GroupRename {
        id: String,
        name: String,
    },
    /// The inputs `numbers` (1-based) of the choice `key` in group `id`, or out of it.
    GroupMember {
        id: String,
        key: String,
        numbers: Vec<usize>,
        member: bool,
    },
    GroupDelete {
        id: String,
    },
}

/// What the screen draws now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScreenView {
    /// False while no engine runs: the desk does not know its inputs.
    pub known: bool,
    pub rows: Vec<ChannelRow>,
    pub cards: Vec<GroupCard>,
    pub can_add: bool,
    /// The last edit's outcome; `None` before the first.
    pub outcome: Option<Said>,
}

#[derive(Default)]
struct Inner {
    /// The groups as the last answer to a group edit gave them (1-based); `None` until one
    /// answered, and again once a saved session was loaded.
    kept_groups: Option<Vec<GroupDefinition>>,
    pending: PendingToggles,
    outcome: Option<Said>,
    changed: Option<Rc<dyn Fn()>>,
}

/// The screen's model, built once as the window opens and kept for its life, so its flows go on
/// while another screen is shown. Clones share it.
#[derive(Clone)]
pub struct Model {
    console: Console,
    inner: Rc<RefCell<Inner>>,
}

impl Model {
    /// The model over `console` (a handle that sends its commands for this screen).
    pub fn new(console: Console) -> Self {
        let inner = Rc::new(RefCell::new(Inner::default()));
        let model = Self { console, inner };
        model.forget_groups_on_load();
        model
    }

    /// The console this screen talks to the desk through.
    pub fn console(&self) -> &Console {
        &self.console
    }

    /// Calls `changed` whenever the model's own state changes outside the console's (a box held,
    /// an outcome), so the view draws again.
    pub fn on_change(&self, changed: impl Fn() + 'static) {
        self.inner.borrow_mut().changed = Some(Rc::new(changed));
    }

    fn changed(&self) {
        let changed = self.inner.borrow().changed.clone();
        if let Some(changed) = changed {
            changed();
        }
    }

    /// After a loaded session the desk's groups are the loaded ones, so the next group edit
    /// builds on the status again.
    fn forget_groups_on_load(&self) {
        let inner = Rc::clone(&self.inner);
        self.console.subscribe(move |_, action| {
            if matches!(action, Action::SessionLoaded(_)) {
                inner.borrow_mut().kept_groups = None;
            }
        });
    }

    /// Replaces the channel details the desk keeps with `change(table, count)`: `table` is the
    /// whole table read afresh (inputs past the device included), `count` the device's inputs when
    /// the edit was asked for. It waits for every desk turn asked for before it.
    pub fn edit_channels(
        &self,
        change: impl FnOnce(&[SessionChannel], usize) -> Result<Vec<SessionChannel>, String> + 'static,
    ) -> impl Future<Output = Said> + 'static {
        let count = self
            .console
            .state()
            .engine
            .status
            .as_ref()
            .map_or(0, |status| status.source_channels);
        let console = self.console.clone();
        self.console.in_turn(true, async move {
            let table = match console
                .call::<Vec<SessionChannel>>(Command::ChannelTable)
                .await
            {
                Ok(table) => table,
                Err(error) => return Said::refused(CHANNELS_REFUSED, &error),
            };
            let next = match change(&table, count) {
                Ok(next) => next,
                Err(error) => return Said::refused(CHANNELS_REFUSED, &error),
            };
            let outcome = match console
                .call::<ChannelsSetDto>(Command::SetChannels {
                    channels: requests(&next),
                })
                .await
            {
                Ok(outcome) => outcome,
                Err(error) => return Said::refused(CHANNELS_REFUSED, &error),
            };
            console.read_status().await;
            Said {
                ok: true,
                message: format_channels_set(
                    outcome.applied,
                    outcome.saved,
                    outcome.save_error.as_deref(),
                ),
            }
        })
    }

    /// Replaces the groups the desk keeps with `change(groups)`: `groups` is the last answer's
    /// list, else the status's, and with neither the edit is refused (the desk's own list is not
    /// replaced blindly). It waits for every desk turn asked for before it.
    pub fn edit_groups(
        &self,
        change: impl FnOnce(&[GroupDefinition]) -> Result<Vec<GroupDefinition>, String> + 'static,
    ) -> impl Future<Output = Said> + 'static {
        let console = self.console.clone();
        let inner = Rc::clone(&self.inner);
        self.console.in_turn(true, async move {
            let kept = inner.borrow().kept_groups.clone();
            let base = match kept {
                Some(kept) => Some(kept),
                None => {
                    let state = console.state();
                    state
                        .engine
                        .status
                        .is_some()
                        .then(|| group_definitions_of(&state))
                }
            };
            let Some(base) = base else {
                return Said::refused(GROUPS_REFUSED, GROUPS_UNKNOWN);
            };
            let next = match change(&base) {
                Ok(next) => next,
                Err(error) => return Said::refused(GROUPS_REFUSED, &error),
            };
            let outcome = match console
                .call::<GroupsSetDto>(Command::SetGroups {
                    groups: next.iter().map(GroupDefinition::request).collect(),
                })
                .await
            {
                Ok(outcome) => outcome,
                Err(error) => return Said::refused(GROUPS_REFUSED, &error),
            };
            // An answer numbers inputs from 1, as the request did; a status from 0.
            inner.borrow_mut().kept_groups = Some(group_definitions(
                outcome.groups.iter().map(|group| {
                    (
                        group.id.as_str(),
                        group.name.as_str(),
                        group.channels.as_slice(),
                    )
                }),
                1,
            ));
            console.read_status().await;
            Said {
                ok: true,
                message: format_groups_set(
                    outcome.applied,
                    outcome.saved,
                    outcome.save_error.as_deref(),
                ),
            }
        })
    }

    /// Runs one of the screen's edits and keeps its outcome; a box it toggles (`toggle`) is held as
    /// clicked until it is answered, and the screen is asked to draw at once.
    pub fn handle(&self, edit: Edit) -> impl Future<Output = Said> + 'static {
        let (flow, toggle): (std::pin::Pin<Box<dyn Future<Output = Said>>>, _) =
            match edit {
                Edit::Rename { index, name } => (
                    Box::pin(self.edit_channels(move |table, count| {
                        rename_channel(table, index, &name, count)
                    })),
                    None,
                ),
                Edit::Kind { index, kind } => (
                    Box::pin(self.edit_channels(move |table, count| {
                        set_channel_kind(table, index, &kind, count)
                    })),
                    None,
                ),
                Edit::Link { index, linked } => (
                    Box::pin(self.edit_channels(move |table, count| {
                        set_channel_link(table, index, linked, count)
                    })),
                    None,
                ),
                Edit::Shown { index, shown } => (
                    Box::pin(self.edit_channels(move |table, count| {
                        set_channel_hidden(table, index, !shown, count)
                    })),
                    Some((ToggleKey::Shown(index), shown)),
                ),
                Edit::GroupAdd => (Box::pin(self.edit_groups(add_group)), None),
                Edit::GroupRename { id, name } => (
                    Box::pin(self.edit_groups(move |groups| rename_group(groups, &id, &name))),
                    None,
                ),
                Edit::GroupMember {
                    id,
                    key,
                    numbers,
                    member,
                } => {
                    let toggle = ToggleKey::Member {
                        id: id.clone(),
                        key,
                    };
                    (
                        Box::pin(self.edit_groups(move |groups| {
                            set_group_members(groups, &id, &numbers, member)
                        })),
                        Some((toggle, member)),
                    )
                }
                Edit::GroupDelete { id } => (
                    Box::pin(self.edit_groups(move |groups| delete_group(groups, &id))),
                    None,
                ),
            };
        let held = toggle.map(|(key, value)| {
            let token = self.inner.borrow_mut().pending.hold(key.clone(), value);
            (key, token)
        });
        if held.is_some() {
            self.changed();
        }
        let model = self.clone();
        async move {
            let said = flow.await;
            {
                let mut inner = model.inner.borrow_mut();
                if let Some((key, token)) = &held {
                    inner.pending.release(key, *token);
                }
                inner.outcome = Some(said.clone());
            }
            model.changed();
            said
        }
    }

    /// What the screen draws now: the rows and cards with every box the engineer clicked held as
    /// clicked until its edit is answered.
    pub fn view(&self) -> ScreenView {
        let state = self.console.state();
        let known = state.engine.status.is_some();
        let inner = self.inner.borrow();
        ScreenView {
            known,
            rows: if known {
                with_pending_shown(channel_rows(&state), &inner.pending)
            } else {
                Vec::new()
            },
            cards: if known {
                with_pending_members(group_cards(&state), &inner.pending)
            } else {
                Vec::new()
            },
            can_add: can_add_group(&state),
            outcome: inner.outcome.clone(),
        }
    }
}

/// The details as the `set_channels` request carries them.
fn requests(table: &[SessionChannel]) -> Vec<ChannelRequest> {
    table
        .iter()
        .map(|channel| ChannelRequest {
            name: channel.name.clone(),
            kind: channel.kind.clone(),
            hidden: channel.hidden,
            link: channel.link,
        })
        .collect()
}

#[cfg(test)]
mod tests;
