//! Named sessions and mix backups as the Session panel shows them (`sessions.js`,
//! `mix-backups-view.js`): the lists, the words for the desk's answers, and the panel's own
//! behaviour (which session is chosen, the load that asks first while edits are on their way, the
//! guarded delete, the restore that asks first). Pure: the timers are the model's.
//!
//! A saved session keeps the whole desk session and, while an engine runs, every musician's mix
//! (N35). Loading replaces the desk session and pushes each saved mix to the musician it was
//! saved for when their phone is connected; one that is not keeps its own mix (N37). The desk
//! backs the live mixes up a few seconds after they go still and keeps the newest 10; a restore
//! sends each backed-up mix to the phones connected now, never queued (N41, N42).

use bandear_desktop_console::format::{musician_label, sentence_with_reason};
use bandear_desktop_console::state::ConsoleState;
use bandear_desktop_console::types::dto::{
    MixRestoredDto, SessionLoadedDto, SessionSavedDto, SkippedMixDto,
};
use bandear_desktop_console::types::{MixBackupSummary, SnapshotSummary};
use bandear_desktop_console::wire::Musician;

use super::arming::Arming;
use super::clock::{Zone, format_backup_time, format_saved_at};

/// The longest name of a saved session, in characters (the fields cap at it; the desk refuses a
/// longer one).
pub const MAX_SESSION_NAME_LENGTH: usize = 40;

/// "A", "A and B", "A, B and C".
pub fn join_names(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// "no mixes", "1 musician's mix", "3 musicians' mixes".
pub fn mixes_text(count: usize) -> String {
    match count {
        0 => "no mixes".to_owned(),
        1 => "1 musician's mix".to_owned(),
        count => format!("{count} musicians' mixes"),
    }
}

/// A saved session as the panel lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionRow {
    pub id: String,
    pub name: String,
    pub loadable: bool,
    /// The option's text.
    pub label: String,
    /// When it was saved and how many mixes it keeps, or why it cannot be loaded.
    pub detail: String,
}

/// The saved sessions as the panel lists them.
pub fn session_rows(sessions: &[SnapshotSummary], zone: Zone) -> Vec<SessionRow> {
    sessions
        .iter()
        .map(|saved| {
            let loadable = saved.problem.is_none();
            let detail = if loadable {
                let when =
                    format_saved_at(saved.saved_at, zone).map(|when| format!("Saved {when}"));
                when.into_iter()
                    .chain([mixes_text(saved.mixes)])
                    .collect::<Vec<_>>()
                    .join(" \u{b7} ")
            } else {
                sentence_with_reason("It cannot be loaded", saved.problem.as_deref())
            };
            SessionRow {
                id: saved.id.clone(),
                name: saved.name.clone(),
                loadable,
                label: if loadable {
                    saved.name.clone()
                } else {
                    format!("{} (cannot be loaded)", saved.name)
                },
                detail,
            }
        })
        .collect()
}

/// What the Session panel shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionPanelView {
    /// What the desk said when it opened its session.
    pub notice: Option<String>,
    pub rows: Vec<SessionRow>,
    /// Why the saved sessions could not be listed ("" while they are listed); `None` once they were.
    pub unread: Option<String>,
    pub engine_running: bool,
}

/// The Session panel for `state`.
pub fn session_panel_view(state: &ConsoleState, zone: Zone) -> SessionPanelView {
    let listing = &state.sessions;
    SessionPanelView {
        notice: state.session_notice.as_deref().map(|notice| {
            sentence_with_reason("When the console opened its saved session", Some(notice))
        }),
        rows: session_rows(listing.list.as_deref().unwrap_or(&[]), zone),
        unread: listing
            .list
            .is_none()
            .then(|| listing.error.clone().unwrap_or_default()),
        engine_running: state.engine.status.is_some(),
    }
}

/// The unread line: listing, or why the sessions could not be listed.
pub fn sessions_unread_text(unread: &str) -> String {
    if unread.is_empty() {
        "Listing the saved sessions\u{2026}".to_owned()
    } else {
        format!("The saved sessions could not be listed: {unread}.")
    }
}

/// What a save did.
pub fn format_session_saved(answer: &SessionSavedDto) -> String {
    let saved = format!("Saved the session as \"{}\"", answer.session.name);
    if !answer.engine_running {
        return format!("{saved} without mixes: the engine is not running.");
    }
    if answer.session.mixes == 0 {
        return format!("{saved}. No musician was listed, so no mix was kept.");
    }
    format!("{saved}, with {}.", mixes_text(answer.session.mixes))
}

/// The name of the musician a skipped saved mix was for: the label the loaded session keeps, else
/// the musician listed under that key, else their address, else "a phone".
pub fn skipped_name(skipped: &SkippedMixDto, musicians: &[Musician]) -> String {
    if let Some(label) = skipped.label.as_deref().filter(|label| !label.is_empty()) {
        return label.to_owned();
    }
    let (kind, value) = skipped.key.split_once(':').unwrap_or(("", &skipped.key));
    let row = musicians.iter().find(|musician| {
        if kind == "client" {
            musician.client_id.as_deref() == Some(value)
        } else {
            musician.address == value
        }
    });
    match row {
        Some(row) => musician_label(row.name.as_deref(), &row.address),
        None if kind == "address" => value.to_owned(),
        None => "a phone".to_owned(),
    }
}

/// What the console calls the musician at `address`: their name, else their IP; the address when
/// nobody is listed there.
pub fn label_at(musicians: &[Musician], address: &str) -> String {
    musicians
        .iter()
        .find(|musician| musician.address == address)
        .map_or_else(
            || address.to_owned(),
            |musician| musician_label(musician.name.as_deref(), &musician.address),
        )
}

/// What a load did, in sentences, with the musicians listed after it: what applies, whether it
/// was saved, the mixes pushed and to whom, those that went to nobody and why, what the engine
/// refused, what could not be read, the audio input and the local mix.
pub fn format_session_loaded(outcome: &SessionLoadedDto, musicians: &[Musician]) -> String {
    let name = &outcome.session.name;
    let mut sentences = vec![if outcome.applied {
        format!(
            "Loaded \"{name}\": its channels, groups, presets, labels, limits, locks and mirrors apply now."
        )
    } else {
        format!("Loaded \"{name}\": it applies at the next start.")
    }];
    if !outcome.saved {
        sentences.push(sentence_with_reason(
            "It runs, but was not saved",
            outcome.save_error.as_deref(),
        ));
    }
    let full: Vec<String> = outcome
        .mixes
        .iter()
        .filter(|mix| mix.follows.is_none())
        .map(|mix| label_at(musicians, &mix.address))
        .collect();
    if !full.is_empty() {
        sentences.push(format!(
            "Pushed the saved {} to {}.",
            if full.len() == 1 { "mix" } else { "mixes" },
            join_names(&full)
        ));
    }
    for mix in &outcome.mixes {
        if let Some(leader) = &mix.follows {
            sentences.push(format!(
                "{} follows {leader}'s mix, so only their master volume and mute were set.",
                label_at(musicians, &mix.address)
            ));
        }
    }
    let skipped = |reason: &str| -> Vec<&SkippedMixDto> {
        outcome
            .skipped
            .iter()
            .filter(|mix| mix.reason == reason)
            .collect()
    };
    let names = |list: &[&SkippedMixDto]| -> Vec<String> {
        list.iter()
            .map(|mix| skipped_name(mix, musicians))
            .collect()
    };
    match skipped("notRunning").len() {
        0 => {}
        1 => sentences.push("The saved mix was not pushed: the engine is not running.".to_owned()),
        count => sentences.push(format!(
            "The {count} saved mixes were not pushed: the engine is not running."
        )),
    }
    let away = names(&skipped("notConnected"));
    if !away.is_empty() {
        sentences.push(format!(
            "Not pushed to {}: not connected, and a phone's own mix wins when it connects.",
            join_names(&away)
        ));
    }
    let gone = names(&skipped("notHere"));
    if !gone.is_empty() {
        sentences.push(format!("Not pushed to {}: not here.", join_names(&gone)));
    }
    for mix in skipped("refused") {
        sentences.push(sentence_with_reason(
            &format!("Not pushed to {}", skipped_name(mix, musicians)),
            mix.error.as_deref(),
        ));
    }
    if !outcome.refused.is_empty() {
        sentences.push(sentence_with_reason(
            "Not applied",
            Some(&outcome.refused.join("; ")),
        ));
    }
    if !outcome.damaged.is_empty() {
        sentences.push(sentence_with_reason(
            "Part of the saved session could not be read and was left out",
            Some(&outcome.damaged.join("; ")),
        ));
    }
    if outcome.audio_loaded {
        sentences.push("Its audio input is used from the next start.".to_owned());
    } else if let Some(skipped) = outcome
        .audio_skipped
        .as_deref()
        .filter(|why| !why.is_empty())
    {
        sentences.push(format!(
            "{} The desk keeps its own.",
            sentence_with_reason("Its audio input was not taken", Some(skipped))
        ));
    }
    if outcome.local_mix.is_some() {
        sentences.push("The local mix is the saved one.".to_owned());
    }
    if let Some(notice) = outcome
        .local_output_notice
        .as_deref()
        .filter(|notice| !notice.is_empty())
    {
        sentences.push(notice.to_owned());
    }
    sentences.join(" ")
}

/// What the Load ask says: "1 change is still on the way …".
pub fn load_ask_text(unsaved: usize, name: &str) -> String {
    let edits = if unsaved == 1 {
        "1 change is".to_owned()
    } else {
        format!("{unsaved} changes are")
    };
    format!("{edits} still on the way to the engine. Loading \"{name}\" drops them.")
}

/// A load waiting for the engineer's word.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asking {
    pub id: String,
    pub unsaved: usize,
    pub name: String,
}

/// Which fields a command's answer empties.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Clear {
    pub save: bool,
    pub rename: bool,
}

/// What a Session panel command is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Save,
    Load,
    Rename,
    Delete,
    List,
    Backups,
    Restore,
}

/// The Session panel's behaviour (`createSessionPanel`): the chosen session, the load that asks
/// first, and the guarded delete.
#[derive(Clone, Debug, Default)]
pub struct SessionPanel {
    rows: Vec<SessionRow>,
    chosen: Option<String>,
    asking: Option<(String, usize)>,
    pub delete: Arming<String>,
}

impl SessionPanel {
    fn row(&self, id: Option<&str>) -> Option<&SessionRow> {
        let id = id?;
        self.rows.iter().find(|row| row.id == id)
    }

    /// Takes the rows; the choice stays on its session while it is listed, else the first.
    pub fn set_rows(&mut self, rows: Vec<SessionRow>) {
        self.rows = rows;
        if self.row(self.chosen.as_deref()).is_none() {
            self.chosen = self.rows.first().map(|row| row.id.clone());
        }
        if let Some(armed) = self.delete.armed().cloned()
            && self.row(Some(&armed)).is_none()
        {
            self.delete.disarm();
        }
        if let Some((id, _)) = &self.asking
            && self.row(Some(id)).is_none()
        {
            self.asking = None;
        }
    }

    pub fn rows(&self) -> &[SessionRow] {
        &self.rows
    }

    /// The saved session chosen, or `None` when none is listed.
    pub fn chosen(&self) -> Option<&SessionRow> {
        self.row(self.chosen.as_deref())
    }

    /// The engineer chose `id`: anything asked about another one is dropped (N36).
    pub fn choose(&mut self, id: &str) {
        if self.chosen.as_deref() == Some(id) || self.row(Some(id)).is_none() {
            return;
        }
        self.chosen = Some(id.to_owned());
        self.asking = None;
        self.delete.disarm();
    }

    /// The name a Save sends, trimmed, or `None` for a blank one.
    pub fn save(&self, name: &str) -> Option<String> {
        let name = name.trim();
        (!name.is_empty()).then(|| name.to_owned())
    }

    /// True when the chosen session can be loaded (N38).
    pub fn load_ready(&self) -> bool {
        self.chosen().is_some_and(|row| row.loadable)
    }

    /// Load pressed with `unsaved` edits on their way: the session to load at once, or `None`
    /// when it cannot be loaded or the panel asks first.
    pub fn load(&mut self, unsaved: usize) -> Option<String> {
        if !self.load_ready() {
            return None;
        }
        let id = self.chosen.clone()?;
        if unsaved > 0 {
            self.asking = Some((id, unsaved));
            return None;
        }
        self.asking = None;
        Some(id)
    }

    /// What the panel asks before a load.
    pub fn asking(&self) -> Option<Asking> {
        let (id, unsaved) = self.asking.as_ref()?;
        Some(Asking {
            id: id.clone(),
            unsaved: *unsaved,
            name: self
                .row(Some(id))
                .map(|row| row.name.clone())
                .unwrap_or_default(),
        })
    }

    /// "Load anyway": the session asked about.
    pub fn confirm_load(&mut self) -> Option<String> {
        self.asking.take().map(|(id, _)| id)
    }

    /// "Cancel": nothing is loaded.
    pub fn cancel_load(&mut self) {
        self.asking = None;
    }

    /// The id and trimmed name a Rename sends, or `None` (a blank name, or a session that cannot
    /// be loaded).
    pub fn rename(&self, name: &str) -> Option<(String, String)> {
        let name = name.trim();
        let row = self.chosen().filter(|row| row.loadable)?;
        (!name.is_empty()).then(|| (row.id.clone(), name.to_owned()))
    }

    /// The command for `kind` was answered: which fields to empty; any answered load drops the ask.
    pub fn answered(&mut self, kind: Kind, ok: bool) -> Clear {
        if kind == Kind::Load {
            self.asking = None;
        }
        Clear {
            save: ok && kind == Kind::Save,
            rename: ok && kind == Kind::Rename,
        }
    }

    /// The session the delete is armed for, when it is the chosen one.
    pub fn delete_state(&self) -> (bool, bool) {
        let chosen = self.chosen.as_ref();
        let armed = chosen.is_some_and(|id| self.delete.is_armed(id));
        (
            armed,
            armed && chosen.is_some_and(|id| self.delete.is_ready(id)),
        )
    }
}

/// The unread line of the backups.
pub fn backups_unread_text(unread: &str) -> String {
    if unread.is_empty() {
        "Listing the mix backups\u{2026}".to_owned()
    } else {
        format!("The mix backups could not be listed: {unread}.")
    }
}

/// A backup as the panel lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackupRow {
    pub stamp: String,
    pub restorable: bool,
    /// When it was taken, to the second, or "Backup <stamp>".
    pub label: String,
    /// Whose mixes it keeps, or why it cannot be restored.
    pub detail: String,
}

fn musicians_text(count: usize, names: &[String]) -> String {
    if count == 0 {
        return "no musicians".to_owned();
    }
    let lead = if count == 1 {
        "1 musician".to_owned()
    } else {
        format!("{count} musicians")
    };
    if names.is_empty() {
        lead
    } else {
        format!("{lead}: {}", join_names(names))
    }
}

/// The backups as the panel lists them, the newest first as the desk lists them.
pub fn mix_backup_rows(list: &[MixBackupSummary], zone: Zone) -> Vec<BackupRow> {
    list.iter()
        .map(|kept| {
            let restorable = kept.problem.is_none();
            BackupRow {
                stamp: kept.stamp.clone(),
                restorable,
                label: restorable
                    .then(|| format_backup_time(kept.saved_at, zone))
                    .flatten()
                    .unwrap_or_else(|| format!("Backup {}", kept.stamp)),
                detail: if restorable {
                    musicians_text(kept.musicians, &kept.names)
                } else {
                    sentence_with_reason("It cannot be restored", kept.problem.as_deref())
                },
            }
        })
        .collect()
}

/// What the Mix backups part shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackupsView {
    pub rows: Vec<BackupRow>,
    /// Why the backups could not be listed ("" while they are listed); `None` once they were.
    pub unread: Option<String>,
    pub engine_running: bool,
}

/// The Mix backups part for `state`.
pub fn mix_backups_view(state: &ConsoleState, zone: Zone) -> BackupsView {
    let listing = &state.mix_backups;
    BackupsView {
        rows: mix_backup_rows(listing.list.as_deref().unwrap_or(&[]), zone),
        unread: listing
            .list
            .is_none()
            .then(|| listing.error.clone().unwrap_or_default()),
        engine_running: state.engine.status.is_some(),
    }
}

/// What the restore ask says.
pub fn restore_ask_text(row: &BackupRow) -> String {
    format!(
        "Restore the mixes backed up at {} ({})? Each connected musician's phone is sent their backed-up mix now.",
        row.label, row.detail
    )
}

/// What a restore did, in sentences, with the musicians listed after it.
pub fn format_mix_restored(outcome: &MixRestoredDto, musicians: &[Musician], zone: Zone) -> String {
    let when = format_backup_time(outcome.backup.saved_at, zone);
    let mut sentences = Vec::new();
    let full: Vec<String> = outcome
        .restored
        .iter()
        .filter(|mix| mix.follows.is_none())
        .map(|mix| label_at(musicians, &mix.address))
        .collect();
    if !full.is_empty() {
        let at = when.map(|when| format!(" at {when}")).unwrap_or_default();
        sentences.push(format!(
            "Restored the mixes backed up{at} to {}.",
            join_names(&full)
        ));
    } else if outcome.restored.is_empty() {
        sentences.push("No musician's mix was restored from the backup.".to_owned());
    }
    for mix in &outcome.restored {
        if let Some(leader) = &mix.follows {
            sentences.push(format!(
                "{} follows {leader}'s mix, so only their master volume and mute were set.",
                label_at(musicians, &mix.address)
            ));
        }
    }
    let skipped = |reason: &str| -> Vec<String> {
        outcome
            .skipped
            .iter()
            .filter(|mix| mix.reason == reason)
            .map(|mix| skipped_name(mix, musicians))
            .collect()
    };
    let away = skipped("notConnected");
    if !away.is_empty() {
        sentences.push(format!(
            "Not restored to {}: not connected, and a phone's own mix wins when it connects.",
            join_names(&away)
        ));
    }
    let gone = skipped("notHere");
    if !gone.is_empty() {
        sentences.push(format!("Not restored to {}: not here.", join_names(&gone)));
    }
    for mix in outcome.skipped.iter().filter(|mix| mix.reason == "refused") {
        sentences.push(sentence_with_reason(
            &format!("Not restored to {}", skipped_name(mix, musicians)),
            mix.error.as_deref(),
        ));
    }
    sentences.join(" ")
}

/// The Mix backups part's behaviour: Restore asks first, and only the confirming Restore restores.
#[derive(Clone, Debug, Default)]
pub struct BackupsPanel {
    rows: Vec<BackupRow>,
    asking: Option<String>,
}

impl BackupsPanel {
    /// Takes the rows; a backup no longer listed is no longer asked about.
    pub fn set_rows(&mut self, rows: Vec<BackupRow>) {
        self.rows = rows;
        if let Some(stamp) = &self.asking
            && !self.rows.iter().any(|row| &row.stamp == stamp)
        {
            self.asking = None;
        }
    }

    /// Restore pressed for `stamp`: asks first, for a backup that can be restored.
    pub fn ask(&mut self, stamp: &str) {
        self.asking = self
            .rows
            .iter()
            .find(|row| row.stamp == stamp && row.restorable)
            .map(|row| row.stamp.clone());
    }

    /// The backup asked about, as listed.
    pub fn asking(&self) -> Option<&BackupRow> {
        let stamp = self.asking.as_ref()?;
        self.rows.iter().find(|row| &row.stamp == stamp)
    }

    pub fn cancel(&mut self) {
        self.asking = None;
    }

    /// The confirming Restore: the backup asked about.
    pub fn confirm(&mut self) -> Option<String> {
        self.asking.take()
    }
}

#[cfg(test)]
#[path = "sessions_tests.rs"]
mod tests;
