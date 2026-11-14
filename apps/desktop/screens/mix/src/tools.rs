//! The Mix tools beside the musician tabs: Mirror (follow no one or another musician's mix), Copy
//! mix to… and Presets, for the open musician's mix. What they offer and how they word the desk's
//! answers (`mixes.js`), and their behaviour apart from the window (`createMixTools`): which panel
//! is open and where the focus goes when it closes, which musicians are ticked, what the Mirror
//! control shows, when a request may go out, and the guarded delete. No window: the view draws what
//! [Tools] answers and runs the [ToolRequest]s it gives through the model.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use bandear_desktop_console::format::{musician_label, sentence_with_reason};
use bandear_desktop_console::state::ConsoleState;
use bandear_desktop_console::types::dto::MirrorSetDto;
use bandear_desktop_console::wire::Musician;
use bandear_desktop_kit::buttons::{CONFIRM_DELAY, CONFIRM_TIMEOUT, confirm_ready};

/// The longest preset name the desk keeps, in characters.
pub const MAX_PRESET_NAME_LENGTH: usize = 40;

/// The Mirror control's value for a leader who is not here: what the mix follows, not a choice.
pub const AWAY: &str = "away";

/// How long a confirming Delete waits after the first press, so a double click never deletes.
pub const DELETE_DELAY: Duration = CONFIRM_DELAY;
/// How long an armed delete stays armed before it goes back to a plain Delete.
pub const ARMED_FOR: Duration = CONFIRM_TIMEOUT;

/// One choice of the Mirror control.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MirrorOption {
    pub value: String,
    pub label: String,
}

impl MirrorOption {
    fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
        }
    }
}

/// The Mirror control (`mirrorChoice`): `value` is "" for none, the leader's address, or [AWAY]
/// while the leader is not here; `note` says in words whom the mix follows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MirrorChoice {
    pub value: String,
    pub options: Vec<MirrorOption>,
    pub note: Option<String>,
}

/// True when the musician at `follower` following the one at `leader` would close a circle: the
/// leader already follows the follower, directly or through others. Following oneself is one too.
pub fn closes_circle(musicians: &[Musician], follower: &str, leader: &str) -> bool {
    let mut seen = BTreeSet::new();
    let mut current = Some(leader.to_owned());
    while let Some(address) = current {
        if address == follower {
            return true;
        }
        if !seen.insert(address.clone()) {
            return false;
        }
        current = musicians
            .iter()
            .find(|musician| musician.address == address)
            .and_then(|musician| musician.mirror.as_ref())
            .and_then(|mirror| mirror.leader_address.clone());
    }
    false
}

/// "Bass", "Bass and Keys", "Bass, Keys and Drums".
pub fn join_names(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

fn label_of(musician: &Musician) -> String {
    musician_label(musician.name.as_deref(), &musician.address)
}

fn label_at(state: &ConsoleState, address: &str) -> String {
    state
        .musician_by_address(address)
        .map_or_else(|| address.to_owned(), label_of)
}

pub fn mirror_choice(state: &ConsoleState, address: &str) -> MirrorChoice {
    let musician = state.musician_by_address(address);
    let mut options = vec![MirrorOption::new("", "Off")];
    for other in &state.musicians {
        if other.address != address && !closes_circle(&state.musicians, address, &other.address) {
            options.push(MirrorOption::new(other.address.clone(), label_of(other)));
        }
    }
    let Some(mirror) = musician.and_then(|musician| musician.mirror.as_ref()) else {
        return MirrorChoice {
            value: String::new(),
            options,
            note: None,
        };
    };
    let name = label_at(state, address);
    let leader = &mirror.name;
    match &mirror.leader_address {
        None => {
            options.push(MirrorOption::new(AWAY, format!("{leader} (not here)")));
            MirrorChoice {
                value: AWAY.to_owned(),
                options,
                note: Some(format!(
                    "{name} follows {leader}'s mix. {leader} is not here, so {name}'s mix stays as {leader} left it until they are back."
                )),
            }
        }
        Some(leader_address) => MirrorChoice {
            value: leader_address.clone(),
            options,
            note: Some(format!(
                "{name} follows {}'s mix: its channels and groups move with it, its master stays its own.",
                label_at(state, leader_address)
            )),
        },
    }
}

/// A musician a copy or a preset can reach: a follower is offered disabled, with whom it follows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub address: String,
    pub label: String,
    pub disabled: bool,
    pub note: Option<String>,
}

/// The musicians a change from `address`'s tools can reach, in the status's order; with
/// `include_self` the open musician comes first ("this mix").
pub fn mix_targets(state: &ConsoleState, address: &str, include_self: bool) -> Vec<Target> {
    let mut targets: Vec<Target> = state
        .musicians
        .iter()
        .filter(|musician| include_self || musician.address != address)
        .map(|musician| {
            let following = musician
                .mirror
                .as_ref()
                .map(|mirror| format!("follows {}", mirror.name));
            let note = if musician.address == address {
                Some(match &following {
                    Some(following) => format!("this mix, {following}"),
                    None => "this mix".to_owned(),
                })
            } else {
                following
            };
            Target {
                address: musician.address.clone(),
                label: label_of(musician),
                disabled: musician.mirror.is_some(),
                note,
            }
        })
        .collect();
    targets.sort_by_key(|target| target.address != address);
    targets
}

/// A preset as the tools list it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresetItem {
    pub id: String,
    pub name: String,
}

/// What the tools show for `address`'s mix (`mixToolsView`). `presets` is `None` until the desk's
/// list was read; `can_use` is false while no engine runs (presets are saved from and put on mixes
/// the engine plays; a mirror can still be kept for the next start).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MixToolsView {
    pub address: String,
    pub name: String,
    pub mirror: MirrorChoice,
    pub copy_targets: Vec<Target>,
    pub preset_targets: Vec<Target>,
    pub presets: Option<Vec<PresetItem>>,
    pub can_use: bool,
}

pub fn mix_tools_view(state: &ConsoleState, address: &str) -> MixToolsView {
    MixToolsView {
        address: address.to_owned(),
        name: label_at(state, address),
        mirror: mirror_choice(state, address),
        copy_targets: mix_targets(state, address, false),
        preset_targets: mix_targets(state, address, true),
        presets: state.presets.as_ref().map(|presets| {
            presets
                .iter()
                .map(|preset| PresetItem {
                    id: preset.id.clone(),
                    name: preset.name.clone(),
                })
                .collect()
        }),
        can_use: state.engine.status.is_some(),
    }
}

/// What a mirror change did (`formatMirrorSet`), naming the follower and the leader.
pub fn format_mirror_set(outcome: &MirrorSetDto, follower: &str, leader: Option<&str>) -> String {
    let done = match &outcome.leader {
        None => {
            if outcome.applied {
                format!("{follower} no longer follows another mix; their mix stays as it is")
            } else {
                format!("{follower} will follow no other mix from the next start")
            }
        }
        Some(_) => {
            let leader = leader.unwrap_or_default();
            if outcome.applied {
                format!("{follower} follows {leader}'s mix now")
            } else {
                format!("{follower} will follow {leader}'s mix from the next start")
            }
        }
    };
    if outcome.saved {
        format!("{done}.")
    } else {
        sentence_with_reason(
            &format!("{done}, but this was not saved"),
            outcome.save_error.as_deref(),
        )
    }
}

/// What a copy did: "Copied Ana's mix to Bass and Keys: channels, pans, mutes, solos and group
/// levels."
pub fn format_copied(from: &str, names: &[String]) -> String {
    format!(
        "Copied {from}'s mix to {}: channels, pans, mutes, solos and group levels.",
        join_names(names)
    )
}

/// What a preset put on musicians did: 'Applied "Verse" to Bass and Keys.'
pub fn format_preset_applied(preset: &str, names: &[String]) -> String {
    format!("Applied \"{preset}\" to {}.", join_names(names))
}

/// A panel the tools open under their row, one at a time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel {
    Copy,
    Presets,
}

/// A list of musicians to tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TickList {
    /// "Copy to".
    Copy,
    /// "Apply to".
    Apply,
}

/// The command a tool runs, as its answer is handed back ([Tools::answered]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolKind {
    Mirror,
    Copy,
    Save,
    Apply,
    Rename,
    Delete,
}

/// What the tools ask the screen to run for the open musician's mix.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToolRequest {
    /// Follow the musician at `leader`, or no one.
    Mirror {
        leader: Option<String>,
    },
    Copy {
        to: Vec<String>,
    },
    Save {
        name: String,
    },
    Apply {
        id: String,
        to: Vec<String>,
    },
    Rename {
        id: String,
        name: String,
    },
    Delete {
        id: String,
    },
    /// Read the presets again (they could not be read).
    LoadPresets,
}

impl ToolRequest {
    /// The command it runs, or `None` for a read.
    pub fn kind(&self) -> Option<ToolKind> {
        Some(match self {
            Self::Mirror { .. } => ToolKind::Mirror,
            Self::Copy { .. } => ToolKind::Copy,
            Self::Save { .. } => ToolKind::Save,
            Self::Apply { .. } => ToolKind::Apply,
            Self::Rename { .. } => ToolKind::Rename,
            Self::Delete { .. } => ToolKind::Delete,
            Self::LoadPresets => return None,
        })
    }
}

/// Where the focus goes once a panel opened or closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    /// Into the panel opened (its first field or button).
    Panel(Panel),
    /// Back to the button of the panel that closed.
    Opener(Panel),
    /// Nowhere new.
    Stay,
}

/// The fields an answer empties: a done save's name, a done rename's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Clears {
    pub save: bool,
    pub rename: bool,
}

/// How a preset's delete shows: asking (`armed`), and whether the confirming Delete works yet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DeleteState {
    pub armed: bool,
    pub ready: bool,
}

#[derive(Clone, Debug)]
struct Armed {
    id: String,
    at: Instant,
    generation: u64,
}

/// The tools of one Mix screen (`createMixTools`).
#[derive(Clone, Debug, Default)]
pub struct Tools {
    data: Option<MixToolsView>,
    open: Option<Panel>,
    copy_ticks: BTreeSet<String>,
    apply_ticks: Option<BTreeSet<String>>,
    /// The Mirror choice sent and not answered yet; the desk's own value shows once it is answered
    /// or the desk's mirror changes.
    pending_mirror: Option<String>,
    armed: Option<Armed>,
    generations: u64,
}

impl Tools {
    pub fn new() -> Self {
        Self::default()
    }

    /// The data the tools show now.
    pub fn data(&self) -> Option<&MixToolsView> {
        self.data.as_ref()
    }

    /// Takes the tools' data for the open musician's mix.
    pub fn set_data(&mut self, next: MixToolsView) {
        match &self.data {
            Some(data) if data.address == next.address => {
                // The desk's mirror changed (the answer's status, or any other route): it shows now.
                if data.mirror.value != next.mirror.value {
                    self.pending_mirror = None;
                }
            }
            _ => {
                // Another musician's mix: nothing ticked, chosen or armed for the last one stays.
                self.copy_ticks.clear();
                self.apply_ticks = None;
                self.pending_mirror = None;
                self.armed = None;
            }
        }
        for target in next.copy_targets.iter().chain(&next.preset_targets) {
            if target.disabled {
                self.copy_ticks.remove(&target.address);
                if let Some(ticks) = self.apply_ticks.as_mut() {
                    ticks.remove(&target.address);
                }
            }
        }
        if self.apply_ticks.is_none() && next.presets.as_ref().is_some_and(|list| !list.is_empty())
        {
            // The open musician is ticked to begin with, unless their mix follows another's.
            self.apply_ticks = Some(
                next.preset_targets
                    .iter()
                    .filter(|target| target.address == next.address && !target.disabled)
                    .map(|target| target.address.clone())
                    .collect(),
            );
        }
        let armed_gone = self.armed.as_ref().is_some_and(|armed| {
            !next
                .presets
                .iter()
                .flatten()
                .any(|preset| preset.id == armed.id)
        });
        if armed_gone {
            self.armed = None;
        }
        self.data = Some(next);
    }

    /// What the Mirror control shows: a choice on its way while it is still offered and the desk's
    /// mirror has not changed since, else the desk's own.
    pub fn mirror_value(&self) -> String {
        let Some(data) = &self.data else {
            return String::new();
        };
        match &self.pending_mirror {
            Some(pending)
                if data
                    .mirror
                    .options
                    .iter()
                    .any(|option| &option.value == pending) =>
            {
                pending.clone()
            }
            _ => data.mirror.value.clone(),
        }
    }

    /// The engineer chose `value` in the Mirror control: the request to send, if any. The leader
    /// away and the mirror already shown send nothing.
    pub fn choose_mirror(&mut self, value: &str) -> Option<ToolRequest> {
        if self.data.is_none() || value == AWAY || value == self.mirror_value() {
            return None;
        }
        self.pending_mirror = Some(value.to_owned());
        Some(ToolRequest::Mirror {
            leader: (!value.is_empty()).then(|| value.to_owned()),
        })
    }

    /// The screen's command for `kind` was answered, `ok` or not: whatever the answer, the Mirror
    /// control shows the desk's mirror again; a done copy unticks its musicians.
    pub fn answered(&mut self, kind: ToolKind, ok: bool) -> Clears {
        if kind == ToolKind::Mirror {
            self.pending_mirror = None;
        }
        if ok && kind == ToolKind::Copy {
            self.copy_ticks.clear();
        }
        Clears {
            save: ok && kind == ToolKind::Save,
            rename: ok && kind == ToolKind::Rename,
        }
    }

    /// The panel open, if any.
    pub fn open(&self) -> Option<Panel> {
        self.open
    }

    /// Opens `panel`, or closes it when it is open: where the focus goes, and a read of the presets
    /// when Presets opens before they could be read.
    pub fn toggle(&mut self, panel: Panel) -> (Focus, Option<ToolRequest>) {
        let previous = self.open;
        self.open = if previous == Some(panel) {
            None
        } else {
            Some(panel)
        };
        let unread = self
            .data
            .as_ref()
            .is_some_and(|data| data.presets.is_none());
        let load =
            (self.open == Some(Panel::Presets) && unread).then_some(ToolRequest::LoadPresets);
        match self.open {
            Some(open) => (Focus::Panel(open), load),
            None => {
                self.armed = None;
                (previous.map_or(Focus::Stay, Focus::Opener), load)
            }
        }
    }

    /// Escape in a panel: closes it, and the focus goes back to its button.
    pub fn escape(&mut self) -> Focus {
        match self.open {
            Some(open) => self.toggle(open).0,
            None => Focus::Stay,
        }
    }

    /// Closes every panel and forgets the ticks, choice and delete: the screen was left.
    pub fn reset(&mut self) {
        *self = Self {
            generations: self.generations,
            ..Self::default()
        };
    }

    fn targets(&self, list: TickList) -> &[Target] {
        match (&self.data, list) {
            (Some(data), TickList::Copy) => &data.copy_targets,
            (Some(data), TickList::Apply) => &data.preset_targets,
            (None, _) => &[],
        }
    }

    /// The musicians ticked in `list` that can be reached, in the order the targets list them.
    fn ticked(&self, list: TickList) -> Vec<String> {
        let ticks = match list {
            TickList::Copy => Some(&self.copy_ticks),
            TickList::Apply => self.apply_ticks.as_ref(),
        };
        let Some(ticks) = ticks else {
            return Vec::new();
        };
        self.targets(list)
            .iter()
            .filter(|target| !target.disabled && ticks.contains(&target.address))
            .map(|target| target.address.clone())
            .collect()
    }

    /// The engineer ticked or unticked `address` in `list`.
    pub fn tick(&mut self, list: TickList, address: &str, checked: bool) {
        let reachable = self
            .targets(list)
            .iter()
            .any(|target| target.address == address && !target.disabled);
        let ticks = match list {
            TickList::Copy => Some(&mut self.copy_ticks),
            TickList::Apply => self.apply_ticks.as_mut(),
        };
        let Some(ticks) = ticks.filter(|_| reachable) else {
            return;
        };
        if checked {
            ticks.insert(address.to_owned());
        } else {
            ticks.remove(address);
        }
    }

    /// Whether `address` shows ticked in `list`.
    pub fn is_ticked(&self, list: TickList, address: &str) -> bool {
        self.ticked(list).iter().any(|ticked| ticked == address)
    }

    fn can_use(&self) -> bool {
        self.data.as_ref().is_some_and(|data| data.can_use)
    }

    pub fn copy_ready(&self) -> bool {
        self.can_use() && !self.ticked(TickList::Copy).is_empty()
    }

    pub fn apply_ready(&self, id: Option<&str>) -> bool {
        self.can_use()
            && id.is_some_and(|id| !id.is_empty())
            && !self.ticked(TickList::Apply).is_empty()
    }

    pub fn save_ready(&self, name: &str) -> bool {
        self.can_use() && !name.trim().is_empty()
    }

    pub fn copy(&self) -> Option<ToolRequest> {
        self.copy_ready().then(|| ToolRequest::Copy {
            to: self.ticked(TickList::Copy),
        })
    }

    pub fn save(&self, name: &str) -> Option<ToolRequest> {
        self.save_ready(name).then(|| ToolRequest::Save {
            name: name.trim().to_owned(),
        })
    }

    pub fn apply(&self, id: Option<&str>) -> Option<ToolRequest> {
        let id = id.filter(|_| self.apply_ready(id))?;
        Some(ToolRequest::Apply {
            id: id.to_owned(),
            to: self.ticked(TickList::Apply),
        })
    }

    /// Renaming needs no engine: the presets live in the desk's session.
    pub fn rename(&self, id: Option<&str>, name: &str) -> Option<ToolRequest> {
        let trimmed = name.trim();
        match id {
            Some(id) if !id.is_empty() && !trimmed.is_empty() => Some(ToolRequest::Rename {
                id: id.to_owned(),
                name: trimmed.to_owned(),
            }),
            _ => None,
        }
    }

    /// Delete pressed for the preset `id` at `now`: it asks first. Answers the arming's number,
    /// for [Tools::expire] once [ARMED_FOR] has passed.
    pub fn arm(&mut self, id: &str, now: Instant) -> u64 {
        self.generations += 1;
        self.armed = Some(Armed {
            id: id.to_owned(),
            at: now,
            generation: self.generations,
        });
        self.generations
    }

    /// Keep pressed (or Escape): the preset stays.
    pub fn keep(&mut self) {
        self.armed = None;
    }

    /// The arming numbered `generation` has been armed for [ARMED_FOR]: it disarms, unless another
    /// arming took its place. True when it disarmed.
    pub fn expire(&mut self, generation: u64) -> bool {
        if self
            .armed
            .as_ref()
            .is_some_and(|armed| armed.generation == generation)
        {
            self.armed = None;
            return true;
        }
        false
    }

    /// The confirming Delete for `id` at `now`: works only once armed long enough.
    pub fn confirm_delete(&mut self, id: &str, now: Instant) -> Option<ToolRequest> {
        let armed = self.armed.as_ref().filter(|armed| armed.id == id)?;
        if !confirm_ready(Some(armed.at), now) {
            return None;
        }
        self.armed = None;
        Some(ToolRequest::Delete { id: id.to_owned() })
    }

    /// How the delete for `id` shows at `now`.
    pub fn delete_state(&self, id: &str, now: Instant) -> DeleteState {
        match self.armed.as_ref().filter(|armed| armed.id == id) {
            Some(armed) => DeleteState {
                armed: true,
                ready: confirm_ready(Some(armed.at), now),
            },
            None => DeleteState::default(),
        }
    }
}

#[cfg(test)]
mod tests;
