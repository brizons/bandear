//! The Mix tools' commands (`console-app.js`, `savePreset` to `setMirror`, and the screen's
//! `mixToolCommands`): each answers in words and reads the status after a done one; the screen says
//! the answer above the strips and hands it to the tools.

use std::cell::Ref;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use bandear_desktop_console::command::Command;
use bandear_desktop_console::console::Console;
use bandear_desktop_console::format::sentence_with_reason;
use bandear_desktop_console::state::Action;
use bandear_desktop_console::types::dto::{MirrorRequest, MirrorSetDto, MixesSetDto, PresetsDto};

use super::{Model, label_at, label_of, notify};
use crate::Said;
use crate::tools::{
    Clears, ToolRequest, Tools, format_copied, format_mirror_set, format_preset_applied,
};

// The Mix tools: their state, and their commands.
impl Model {
    /// The tools as they are now. Read and let go.
    pub fn tools(&self) -> Ref<'_, Tools> {
        Ref::map(self.local.borrow(), |local| &local.tools)
    }

    /// Changes the tools (a tick, a panel, an armed delete) and answers what `change` answers.
    pub fn with_tools<T>(&self, change: impl FnOnce(&mut Tools) -> T) -> T {
        change(&mut self.local.borrow_mut().tools)
    }

    /// The fields the last answers emptied, once.
    pub fn take_clears(&self) -> Clears {
        std::mem::take(&mut self.local.borrow_mut().clears)
    }

    /// The last outcome said above the strips.
    pub fn outcome(&self) -> Option<Said> {
        self.local.borrow().outcome.clone()
    }

    /// Reads the desk's presets again.
    pub fn load_presets(&self) {
        let load = self.console.load_presets();
        self.console.spawn(load);
    }

    /// Runs `request` for the open mix (`mixToolCommands`), and lets it go on by itself.
    pub fn tool(&self, request: ToolRequest) {
        if request == ToolRequest::LoadPresets {
            self.load_presets();
            return;
        }
        let run = self.run_tool(request);
        self.console.spawn(run);
    }

    /// Runs `request` on the open musician's mix, says its answer above the strips, then hands it to
    /// the tools whatever happened, so a Mirror choice never stays on show unanswered. With no mix
    /// open nothing runs. Resolves to the answer, if one came.
    pub fn run_tool(&self, request: ToolRequest) -> impl Future<Output = Option<Said>> + 'static {
        let kind = request.kind();
        let run = self
            .selected()
            .map(|address| self.command(&address, request));
        let local = Rc::clone(&self.local);
        async move {
            let said = match run {
                Some(run) => run.await,
                None => None,
            };
            {
                let mut local = local.borrow_mut();
                if let Some(said) = &said
                    && !local.away
                {
                    local.outcome = Some(said.clone());
                }
                if let Some(kind) = kind {
                    let ok = said.as_ref().is_some_and(|said| said.ok);
                    let clears = local.tools.answered(kind, ok);
                    local.clears.save |= clears.save;
                    local.clears.rename |= clears.rename;
                }
            }
            notify(&local);
            said
        }
    }

    fn command(
        &self,
        address: &str,
        request: ToolRequest,
    ) -> Pin<Box<dyn Future<Output = Option<Said>>>> {
        let address = address.to_owned();
        match request {
            ToolRequest::Mirror { leader } => {
                let said = self.set_mirror(&address, leader.as_deref());
                Box::pin(async move { Some(said.await) })
            }
            ToolRequest::Copy { to } => {
                let said = self.copy_mix(&address, to);
                Box::pin(async move { Some(said.await) })
            }
            ToolRequest::Save { name } => {
                let said = self.save_preset(&address, &name);
                Box::pin(async move { Some(said.await) })
            }
            ToolRequest::Apply { id, to } => {
                let said = self.apply_preset(&id, to);
                Box::pin(async move { Some(said.await) })
            }
            ToolRequest::Rename { id, name } => {
                let said = self.rename_preset(&id, &name);
                Box::pin(async move { Some(said.await) })
            }
            ToolRequest::Delete { id } => {
                let said = self.delete_preset(&id);
                Box::pin(async move { Some(said.await) })
            }
            ToolRequest::LoadPresets => Box::pin(async { None }),
        }
    }

    /// The name of the preset `id` as the console knows it, or a plain word for one it does not.
    fn preset_name(console: &Console, id: &str) -> String {
        console
            .state()
            .presets
            .iter()
            .flatten()
            .find(|preset| preset.id == id)
            .map_or_else(|| "the preset".to_owned(), |preset| preset.name.clone())
    }

    /// Saves `address`'s mix as a new preset called `name`.
    pub fn save_preset(&self, address: &str, name: &str) -> impl Future<Output = Said> + 'static {
        let (console, address, name) = (self.console.clone(), address.to_owned(), name.to_owned());
        let reader = console.clone();
        mix_command(
            console,
            async move {
                let outcome = reader
                    .call::<PresetsDto>(Command::SavePreset {
                        address: address.clone(),
                        name: name.clone(),
                    })
                    .await?;
                let saved = outcome
                    .preset
                    .as_ref()
                    .map_or(name, |preset| preset.name.clone());
                reader.dispatch(Action::Presets(outcome.presets));
                let whose = label_at(&reader.state(), &address);
                Ok(format!("Saved {whose}'s mix as \"{saved}\"."))
            },
            "The preset was not saved",
        )
    }

    /// Renames the preset `id` to `name`.
    pub fn rename_preset(&self, id: &str, name: &str) -> impl Future<Output = Said> + 'static {
        let console = self.console.clone();
        let before = Self::preset_name(&console, id);
        let (reader, id, name) = (console.clone(), id.to_owned(), name.to_owned());
        mix_command(
            console,
            async move {
                let outcome = reader
                    .call::<PresetsDto>(Command::RenamePreset {
                        id,
                        name: name.clone(),
                    })
                    .await?;
                let renamed = outcome
                    .preset
                    .as_ref()
                    .map_or(name, |preset| preset.name.clone());
                reader.dispatch(Action::Presets(outcome.presets));
                Ok(format!("Renamed \"{before}\" to \"{renamed}\"."))
            },
            "The preset was not renamed",
        )
    }

    /// Deletes the preset `id`.
    pub fn delete_preset(&self, id: &str) -> impl Future<Output = Said> + 'static {
        let console = self.console.clone();
        let name = Self::preset_name(&console, id);
        let (reader, id) = (console.clone(), id.to_owned());
        mix_command(
            console,
            async move {
                let outcome = reader
                    .call::<PresetsDto>(Command::DeletePreset { id })
                    .await?;
                reader.dispatch(Action::Presets(outcome.presets));
                Ok(format!("Deleted the preset \"{name}\"."))
            },
            "The preset was not deleted",
        )
    }

    /// Puts the preset `id` on the mixes of `addresses`.
    pub fn apply_preset(
        &self,
        id: &str,
        addresses: Vec<String>,
    ) -> impl Future<Output = Said> + 'static {
        let (console, id) = (self.console.clone(), id.to_owned());
        let reader = console.clone();
        mix_command(
            console,
            async move {
                let outcome = reader
                    .call::<MixesSetDto>(Command::ApplyPreset {
                        id: id.clone(),
                        musicians: addresses,
                    })
                    .await?;
                let names = names_of(&reader, &outcome);
                Ok(format_preset_applied(
                    &Self::preset_name(&reader, &id),
                    &names,
                ))
            },
            "The preset was not applied",
        )
    }

    /// Copies `from`'s mix onto the mixes of `to`.
    pub fn copy_mix(&self, from: &str, to: Vec<String>) -> impl Future<Output = Said> + 'static {
        let (console, from) = (self.console.clone(), from.to_owned());
        let reader = console.clone();
        mix_command(
            console,
            async move {
                let outcome = reader
                    .call::<MixesSetDto>(Command::CopyMix {
                        from: from.clone(),
                        to,
                    })
                    .await?;
                let names = names_of(&reader, &outcome);
                Ok(format_copied(&label_at(&reader.state(), &from), &names))
            },
            "The mix was not copied",
        )
    }

    /// Makes `address`'s mix follow the mix of the musician at `leader`, or stops it with `None`;
    /// the leader's phone and the name this console shows for them go with it, so the follower's
    /// phone says whose mix it follows.
    pub fn set_mirror(
        &self,
        address: &str,
        leader: Option<&str>,
    ) -> impl Future<Output = Said> + 'static {
        let console = self.console.clone();
        let (follower_phone, request) = {
            let state = console.state();
            let follower_phone = state
                .musician_by_address(address)
                .and_then(|musician| musician.client_id.clone());
            let request = leader.map(|leader| {
                state
                    .musician_by_address(leader)
                    .map(|musician| MirrorRequest {
                        address: musician.address.clone(),
                        client_id: musician.client_id.clone(),
                        name: label_of(musician),
                    })
                    .ok_or_else(|| format!("no musician is listed at {leader}"))
            });
            (follower_phone, request.transpose())
        };
        let (reader, address) = (console.clone(), address.to_owned());
        mix_command(
            console,
            async move {
                let request = request?;
                let leader_name = request.as_ref().map(|leader| leader.name.clone());
                let outcome = reader
                    .call::<MirrorSetDto>(Command::SetMirror {
                        address: address.clone(),
                        client_id: follower_phone,
                        leader: request,
                    })
                    .await?;
                let follower = label_at(&reader.state(), &address);
                Ok(format_mirror_set(
                    &outcome,
                    &follower,
                    leader_name.as_deref(),
                ))
            },
            "The mirror was not changed",
        )
    }
}

/// The musicians' names an answer of several mixes names, in its order.
fn names_of(console: &Console, outcome: &MixesSetDto) -> Vec<String> {
    let state = console.state();
    outcome
        .mixes
        .iter()
        .map(|mix| label_at(&state, &mix.address))
        .collect()
}

/// Runs one of the tools' commands: `run` resolves to the outcome in words, and the status is read
/// after it; a refusal is said after `refused`.
fn mix_command(
    console: Console,
    run: impl Future<Output = Result<String, String>> + 'static,
    refused: &'static str,
) -> impl Future<Output = Said> + 'static {
    async move {
        match run.await {
            Ok(said) => {
                console.read_status().await;
                Said::done(said)
            }
            Err(error) => Said::refused(sentence_with_reason(refused, Some(&error))),
        }
    }
}
