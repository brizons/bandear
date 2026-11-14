//! The Session panel's flows (`sessionPanelCommands` in `sessions.js` and the session and backup
//! commands of `console-app.js`): each outcome is said in the panel, and after every session
//! command the saved sessions are listed again (N40).

use std::future::Future;

use bandear_desktop_console::command::Command;
use bandear_desktop_console::format::sentence_with_reason;
use bandear_desktop_console::state::Action;
use bandear_desktop_console::types::SnapshotSummary;
use bandear_desktop_console::types::dto::{MixRestoredDto, SessionLoadedDto, SessionSavedDto};

use super::Model;
use super::arming::{CONFIRM_DELAY, rest_of_arming};
use super::musicians::Said;
use super::sessions::{Kind, format_mix_restored, format_session_loaded, format_session_saved};

impl Model {
    /// The name of the saved session `id` as the console lists it, or a plain word for one it does
    /// not.
    fn session_name(&self, id: &str) -> String {
        self.console()
            .state()
            .sessions
            .list
            .as_ref()
            .and_then(|list| list.iter().find(|saved| saved.id == id))
            .map_or_else(
                || "the saved session".to_owned(),
                |saved| saved.name.clone(),
            )
    }

    /// Runs one session command: `run` resolves to the outcome in words or the refusal, which is
    /// said after `refused`; the saved sessions are listed again whatever the answer.
    fn session_command(
        &self,
        run: impl Future<Output = Result<String, String>> + 'static,
        refused: String,
    ) -> impl Future<Output = Said> + 'static {
        let console = self.console().clone();
        async move {
            let said = match run.await {
                Ok(message) => Said::new(true, message),
                Err(error) => Said::new(false, sentence_with_reason(&refused, Some(&error))),
            };
            console.load_sessions().await;
            said
        }
    }

    /// Says `said` in the panel (when there is something to say) and tells the panel its command
    /// for `kind` was answered, so a load asked about never stays asked.
    fn answer_panel(&self, kind: Kind, said: Option<Said>) {
        self.update(|local| {
            let ok = said.as_ref().is_some_and(|said| said.ok);
            let clear = local.panel.answered(kind, ok);
            if clear.save {
                local.cleared.save_name += 1;
            }
            if clear.rename {
                local.cleared.rename += 1;
            }
            if let Some(said) = said {
                local.session_said = Some(said);
            }
        });
    }

    /// Save: the desk session and, while an engine runs, every musician's mix (N35); a blank name
    /// does nothing.
    pub fn save_session(&self, name: &str) {
        let Some(name) = self.shared.local.borrow().panel.save(name) else {
            return;
        };
        let console = self.console().clone();
        let saved = console.call::<SessionSavedDto>(Command::SaveSession { name });
        let said = self.session_command(
            async move { saved.await.map(|answer| format_session_saved(&answer)) },
            "The session was not saved".to_owned(),
        );
        let model = self.clone();
        console.spawn(async move {
            let said = said.await;
            model.answer_panel(Kind::Save, Some(said));
        });
    }

    /// "List them again".
    pub fn list_sessions(&self) {
        let console = self.console().clone();
        let model = self.clone();
        console.clone().spawn(async move {
            console.load_sessions().await;
            model.answer_panel(Kind::List, None);
        });
    }

    /// Another saved session chosen.
    pub fn choose_session(&self, id: &str) {
        self.update(|local| local.panel.choose(id));
    }

    /// Load pressed: at once, or the panel asks first while edits are on their way (N36).
    pub fn press_load(&self) {
        let unsaved = self.console().unsaved_edits();
        let id = self.update(|local| local.panel.load(unsaved));
        if let Some(id) = id {
            self.load_session(id);
        }
    }

    /// "Load anyway".
    pub fn confirm_load(&self) {
        let id = self.update(|local| local.panel.confirm_load());
        if let Some(id) = id {
            self.load_session(id);
        }
    }

    /// "Cancel", or Escape in the ask.
    pub fn cancel_load(&self) {
        self.update(|local| local.panel.cancel_load());
    }

    /// Loads the saved session `id` once every desk edit asked for before it was answered. The
    /// sends are held while it loads; once it loaded the edits waiting are dropped (none was meant
    /// for it) and the status and strips are read again; a refused load changed nothing, so the
    /// edits held go out (N37, C31).
    pub fn load_session(&self, id: String) {
        let console = self.console().clone();
        let name = self.session_name(&id);
        let loader = console.clone();
        let load = async move {
            let outcome = loader
                .call::<SessionLoadedDto>(Command::LoadSession { id })
                .await?;
            loader.dispatch(Action::SessionLoaded(Box::new(outcome.clone())));
            loader.read_status().await;
            // The loaded session's processing replaced every strip; strips that cannot be read
            // are no longer known, and the answer says so.
            let unread = loader.load_processing().await;
            if let Some(error) = &unread {
                loader.dispatch(Action::ProcessingUnknown(sentence_with_reason(
                    "The loaded session's channel processing could not be read",
                    Some(error),
                )));
            }
            let musicians = loader.state().musicians.clone();
            let words = format_session_loaded(&outcome, &musicians);
            Ok(match unread {
                Some(error) => format!(
                    "{words} {}",
                    sentence_with_reason("Its channel processing could not be read", Some(&error))
                ),
                None => words,
            })
        };
        let said = self.session_command(load, format!("\"{name}\" was not loaded"));
        let holder = console.clone();
        let turn = console.in_turn(false, async move {
            holder.hold_sends();
            let said = said.await;
            // A refused load changed nothing, so the edits held go out; after a load only the
            // edits made since are left.
            holder.release_sends();
            said
        });
        let model = self.clone();
        console.spawn(async move {
            let said = turn.await;
            model.answer_panel(Kind::Load, Some(said));
        });
    }

    /// Rename: the chosen session to the name typed; a blank name or a session that cannot be
    /// loaded does nothing (N38).
    pub fn rename_session(&self, name: &str) {
        let Some((id, name)) = self.shared.local.borrow().panel.rename(name) else {
            return;
        };
        let before = self.session_name(&id);
        let console = self.console().clone();
        let renamed = console.call::<SnapshotSummary>(Command::RenameSession { id, name });
        let said = self.session_command(
            async move {
                renamed.await.map(|renamed| {
                    format!(
                        "Renamed the saved session \"{before}\" to \"{}\".",
                        renamed.name
                    )
                })
            },
            "The saved session was not renamed".to_owned(),
        );
        let model = self.clone();
        console.spawn(async move {
            let said = said.await;
            model.answer_panel(Kind::Rename, Some(said));
        });
    }

    /// Delete pressed for the chosen session: it asks first, and disarms itself after a while (N39).
    pub fn arm_delete(&self) {
        let generation = self.update(|local| {
            let id = local.panel.chosen().map(|row| row.id.clone())?;
            Some(local.panel.delete.arm(id))
        });
        let Some(generation) = generation else {
            return;
        };
        let model = self.clone();
        let console = self.console().clone();
        console.clone().spawn(async move {
            console.sleep(CONFIRM_DELAY).await;
            if !model.update(|local| local.panel.delete.ready(generation)) {
                return;
            }
            console.sleep(rest_of_arming()).await;
            model.update(|local| local.panel.delete.expire(generation));
        });
    }

    /// Keep, or Escape on Keep.
    pub fn keep_delete(&self) {
        self.update(|local| local.panel.delete.disarm());
    }

    /// The confirming Delete: works only once armed long enough.
    pub fn confirm_delete(&self) {
        let id = self.update(|local| {
            let id = local.panel.chosen().map(|row| row.id.clone())?;
            local.panel.delete.confirm(&id).then_some(id)
        });
        let Some(id) = id else {
            return;
        };
        let name = self.session_name(&id);
        let console = self.console().clone();
        let deleted = console.call::<()>(Command::DeleteSession { id });
        let said = self.session_command(
            async move {
                deleted
                    .await
                    .map(|()| format!("Deleted the saved session \"{name}\"."))
            },
            "The saved session was not deleted".to_owned(),
        );
        let model = self.clone();
        console.spawn(async move {
            let said = said.await;
            model.answer_panel(Kind::Delete, Some(said));
        });
    }

    /// Refresh: the backups listed again; a list that worked says nothing, a failed one why.
    pub fn refresh_backups(&self) {
        let console = self.console().clone();
        let listed = console.load_mix_backups();
        let model = self.clone();
        console.spawn(async move {
            let said = listed.await.err().map(|message| Said::new(false, message));
            model.answer_panel(Kind::Backups, said);
        });
    }

    /// Restore pressed for `stamp`: it asks first, for a backup that can be restored (N42).
    pub fn ask_restore(&self, stamp: &str) {
        self.update(|local| local.backups.ask(stamp));
    }

    /// "Cancel", or Escape in the ask.
    pub fn cancel_restore(&self) {
        self.update(|local| local.backups.cancel());
    }

    /// The confirming Restore: in turn with the desk's edits; the status and the backups are read
    /// again after it, whatever the answer.
    pub fn confirm_restore(&self) {
        let Some(stamp) = self.update(|local| local.backups.confirm()) else {
            return;
        };
        let console = self.console().clone();
        let restorer = console.clone();
        let zone = self.shared_zone();
        let turn = console.in_turn(false, async move {
            let said = match restorer
                .call::<MixRestoredDto>(Command::RestoreMixBackup { stamp })
                .await
            {
                Ok(outcome) => {
                    restorer.read_status().await;
                    let musicians = restorer.state().musicians.clone();
                    Said::new(true, format_mix_restored(&outcome, &musicians, zone))
                }
                Err(error) => Said::new(
                    false,
                    sentence_with_reason("The mixes were not restored", Some(&error)),
                ),
            };
            drop(restorer.load_mix_backups().await);
            said
        });
        let model = self.clone();
        console.spawn(async move {
            let said = turn.await;
            model.answer_panel(Kind::Restore, Some(said));
        });
    }

    fn shared_zone(&self) -> super::clock::Zone {
        self.shared.zone.get()
    }
}
