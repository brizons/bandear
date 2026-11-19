//! What the screen's tests share: the sample engine's states, and a simulated desk that answers the
//! Mix screen's commands as the engine does (the console's [FakeDesk] knows only the lifecycle's),
//! so the web console's fake-bridge tests port as deterministic ones.

use std::cell::RefCell;
use std::rc::Rc;

use bandear_desktop_console::command::{Answer, Command, Owner};
use bandear_desktop_console::console::Options;
use bandear_desktop_console::mix::Mix;
use bandear_desktop_console::router::Section;
use bandear_desktop_console::state::{Action, ConsoleState};
use bandear_desktop_console::testing::{FakeDesk, Harness, sample_status};
use bandear_desktop_console::types::dto::{
    EngineerControlsSetDto, FxReturnSetDto, LimiterSetDto, MirrorSetDto, MixDto, MixUpdateRequest,
    MixesSetDto, MusicianMixDto, PresetsDto,
};
use bandear_desktop_console::types::{Effect, LimiterSettings, SessionPreset};
use bandear_desktop_console::wire::{EngineEvent, Mirror, MixChanged, Musician, StatusReport};

use crate::model::Model;

pub const LEAD: &str = "192.168.1.3:50000";
pub const BASS: &str = "192.168.1.12:50000";
pub const KEYS: &str = "192.168.1.17:50000";
pub const DRUMS: &str = "192.168.1.20:50000";

/// The state after `actions`, from a console that heard nothing.
pub fn reduced(actions: Vec<Action>) -> ConsoleState {
    with_status(ConsoleState::new(), actions)
}

/// `state` after `actions`.
pub fn with_status(state: ConsoleState, actions: Vec<Action>) -> ConsoleState {
    let mut state = state;
    for action in &actions {
        state.reduce(action);
    }
    state
}

pub fn status_of(report: StatusReport) -> Action {
    Action::Status(Box::new(report))
}

/// The sample engine running, its first musician open.
pub fn running() -> ConsoleState {
    reduced(vec![status_of(sample_status())])
}

fn level(value: i32) -> u8 {
    value.clamp(0, 100) as u8
}

fn replace<T: Copy>(table: &mut Vec<T>, values: &[T]) {
    for (index, value) in values.iter().enumerate() {
        if index < table.len() {
            table[index] = *value;
        }
    }
}

/// What the engine stores when it merges a `set_mix` request into `mix` (`MixUpdate::merge_into`):
/// each table replaced from its first entry, levels held to 0-100, absent fields kept.
pub fn merge_mix_update(mix: &Mix, request: &MixUpdateRequest) -> Mix {
    let mut merged = mix.clone();
    if let Some(value) = request.volume_percent {
        merged.volume_percent = level(value);
    }
    if let Some(value) = request.max_level_percent {
        merged.max_level_percent = level(value);
    }
    if let Some(value) = request.muted {
        merged.muted = value;
    }
    let levels = |values: &Option<Vec<i32>>| -> Vec<u8> {
        values.iter().flatten().map(|value| level(*value)).collect()
    };
    replace(&mut merged.channel_gains, &levels(&request.channel_gains));
    replace(&mut merged.pans, &levels(&request.pans));
    replace(&mut merged.group_levels, &levels(&request.group_levels));
    replace(
        &mut merged.channel_muted,
        request.channel_muted.as_deref().unwrap_or_default(),
    );
    replace(
        &mut merged.channel_solo,
        request.channel_solo.as_deref().unwrap_or_default(),
    );
    merged
}

/// The desk's copy of a mix.
pub fn dto(mix: &Mix) -> MixDto {
    MixDto {
        revision: mix.revision,
        volume_percent: mix.volume_percent,
        max_level_percent: mix.max_level_percent,
        muted: mix.muted,
        channel_gains: mix.channel_gains.clone(),
        pans: mix.pans.clone(),
        channel_muted: mix.channel_muted.clone(),
        channel_solo: mix.channel_solo.clone(),
        group_levels: mix.group_levels.clone(),
        engineer_limit_percent: mix.engineer_limit_percent,
        locked_channels: mix.locked_channels.clone(),
    }
}

/// The key the desk knows a musician by: their phone's, else their address.
pub fn key_of(musician: &Musician) -> String {
    crate::model::musician_key(musician)
}

fn musician<'a>(desk: &'a mut FakeDesk, address: &str) -> Option<&'a mut Musician> {
    desk.report
        .musicians
        .iter_mut()
        .find(|musician| musician.address == address)
}

fn by_key<'a>(desk: &'a mut FakeDesk, key: &str) -> Option<&'a mut Musician> {
    desk.report
        .musicians
        .iter_mut()
        .find(|musician| key_of(musician) == key)
}

/// Announces `address`'s mix (or the local mix's) as the engine does after a change.
fn announce(desk: &mut FakeDesk, address: Option<&str>, origin: &str, mix: Mix) {
    let run = Some(desk.run);
    desk.events
        .push_back(EngineEvent::MixChanged(Box::new(MixChanged {
            run,
            address: address.map(str::to_owned),
            id: address.is_none().then(|| "local".to_owned()),
            origin: origin.to_owned(),
            mix,
        })));
}

/// The limiter the desk keeps: held in range, as the engine holds it.
fn held(settings: &LimiterSettings) -> LimiterSettings {
    LimiterSettings {
        enabled: settings.enabled,
        ceiling_db: settings.ceiling_db.clamp(-24.0, 0.0),
        release_ms: settings.release_ms.clamp(1.0, 1000.0),
    }
}

/// True when `follower` following `leader` would close a circle among the desk's rows.
fn closes_circle(desk: &FakeDesk, follower: &str, leader: &str) -> bool {
    let mut current = Some(leader.to_owned());
    let mut seen = Vec::new();
    while let Some(address) = current {
        if address == follower {
            return true;
        }
        if seen.contains(&address) {
            return false;
        }
        current = desk
            .report
            .musicians
            .iter()
            .find(|row| row.address == address)
            .and_then(|row| row.mirror.as_ref())
            .and_then(|mirror| mirror.leader_address.clone());
        seen.push(address);
    }
    false
}

/// The presets a scripted desk keeps.
pub type Presets = Rc<RefCell<Vec<SessionPreset>>>;

fn check_name(
    presets: &[SessionPreset],
    name: &str,
    renaming: Option<&str>,
) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("a preset needs a name".to_owned());
    }
    if trimmed.chars().count() > 40 {
        return Err("a preset name is at most 40 characters".to_owned());
    }
    let lower = trimmed.to_lowercase();
    if presets
        .iter()
        .any(|preset| Some(preset.id.as_str()) != renaming && preset.name.to_lowercase() == lower)
    {
        return Err(format!("a preset named \"{trimmed}\" already exists"));
    }
    Ok(trimmed.to_owned())
}

/// The sample engine running on a desk that answers every Mix command, keeping `presets`.
pub fn mix_desk(presets: Presets) -> FakeDesk {
    let mut desk = FakeDesk::running();
    desk.script("set_mix", |desk, command| {
        let Command::SetMix { address, mix } = command else {
            unreachable!()
        };
        if !desk.running {
            return Err("the engine is not running".to_owned());
        }
        let Some(row) = musician(desk, address) else {
            return Err(format!("no musician is listed at {address}"));
        };
        let mut merged = merge_mix_update(&row.mix, mix);
        merged.revision = row.mix.revision + 1;
        row.mix = merged.clone();
        announce(desk, Some(address), "desktop", merged.clone());
        Ok(Answer::Mix(dto(&merged)))
    });
    desk.script("set_local_mix", |desk, command| {
        let Command::SetLocalMix { mix } = command else {
            unreachable!()
        };
        let Some(local) = desk.report.status.local_mix.as_mut() else {
            return Err("the engine is not running".to_owned());
        };
        let mut merged = merge_mix_update(local, mix);
        merged.revision = local.revision + 1;
        *local = merged.clone();
        announce(desk, None, "desktop", merged.clone());
        Ok(Answer::Mix(dto(&merged)))
    });
    desk.script("set_engineer_controls", |desk, command| {
        let Command::SetEngineerControls {
            address, controls, ..
        } = command
        else {
            unreachable!()
        };
        let Some(row) = musician(desk, address) else {
            return Err(format!("no musician is listed at {address}"));
        };
        if let Some(limit) = controls.limit_percent {
            row.mix.engineer_limit_percent = level(limit);
        }
        if let Some(locks) = &controls.locked_channels {
            replace(&mut row.mix.locked_channels, locks);
        }
        row.mix.revision += 1;
        let mix = row.mix.clone();
        announce(desk, Some(address), "desktop", mix.clone());
        Ok(Answer::EngineerControlsSet(EngineerControlsSetDto {
            address: address.clone(),
            limit_percent: mix.engineer_limit_percent,
            locked_channels: mix.locked_channels.clone(),
            kept_for: "phone",
            applied: true,
            saved: true,
            save_error: None,
            mix: Some(dto(&mix)),
        }))
    });
    desk.script("set_musician_limiter", |desk, command| {
        let Command::SetMusicianLimiter {
            musician_key,
            settings,
        } = command
        else {
            unreachable!()
        };
        let Some(row) = by_key(desk, musician_key) else {
            return Err(format!("no musician is known as {musician_key}"));
        };
        row.limiter = held(settings);
        Ok(Answer::LimiterSet(LimiterSetDto {
            musician_key: Some(musician_key.clone()),
            outcome: "set",
            settings: row.limiter,
            saved: true,
            save_error: None,
            message: "This musician's limiter is set.".to_owned(),
        }))
    });
    desk.script("set_local_limiter", |desk, command| {
        let Command::SetLocalLimiter { settings } = command else {
            unreachable!()
        };
        desk.report.status.local_limiter = held(settings);
        Ok(Answer::LimiterSet(LimiterSetDto {
            musician_key: None,
            outcome: "set",
            settings: desk.report.status.local_limiter,
            saved: true,
            save_error: None,
            message: "The local mix's limiter is set.".to_owned(),
        }))
    });
    desk.script("set_musician_fx_return", |desk, command| {
        let Command::SetMusicianFxReturn {
            musician_key,
            effect,
            level: wanted,
        } = command
        else {
            unreachable!()
        };
        let Some(row) = by_key(desk, musician_key) else {
            return Err(format!("no musician is known as {musician_key}"));
        };
        match effect {
            Effect::Reverb => row.fx_returns.reverb = (*wanted).min(100),
            Effect::Delay => row.fx_returns.delay = (*wanted).min(100),
        }
        Ok(Answer::FxReturnSet(FxReturnSetDto {
            musician_key: Some(musician_key.clone()),
            outcome: "set",
            effect: *effect,
            returns: row.fx_returns,
            saved: true,
            save_error: None,
            message: "The return is set.".to_owned(),
        }))
    });
    desk.script("set_local_fx_return", |desk, command| {
        let Command::SetLocalFxReturn {
            effect,
            level: wanted,
        } = command
        else {
            unreachable!()
        };
        let returns = &mut desk.report.status.local_fx_returns;
        match effect {
            Effect::Reverb => returns.reverb = (*wanted).min(100),
            Effect::Delay => returns.delay = (*wanted).min(100),
        }
        Ok(Answer::FxReturnSet(FxReturnSetDto {
            musician_key: None,
            outcome: "set",
            effect: *effect,
            returns: *returns,
            saved: true,
            save_error: None,
            message: "The return is set.".to_owned(),
        }))
    });
    script_presets(&mut desk, presets);
    desk.script("apply_preset", |desk, command| {
        let Command::ApplyPreset { musicians, .. } = command else {
            unreachable!()
        };
        mixes_of(desk, musicians)
    });
    desk.script("copy_mix", |desk, command| {
        let Command::CopyMix { to, .. } = command else {
            unreachable!()
        };
        mixes_of(desk, to)
    });
    desk.script("set_mirror", |desk, command| {
        let Command::SetMirror {
            address, leader, ..
        } = command
        else {
            unreachable!()
        };
        if let Some(leader) = leader
            && closes_circle(desk, address, &leader.address)
        {
            return Err(format!(
                "{} cannot follow {}: the mirrors would go round in a circle",
                address, leader.name
            ));
        }
        let applied = desk.running;
        let Some(row) = musician(desk, address) else {
            return Err(format!("no musician is listed at {address}"));
        };
        row.mirror = leader.as_ref().map(|leader| Mirror {
            leader: format!("address:{}", leader.address),
            leader_address: Some(leader.address.clone()),
            name: leader.name.clone(),
        });
        Ok(Answer::MirrorSet(MirrorSetDto {
            address: address.clone(),
            leader: leader.as_ref().map(|leader| leader.address.clone()),
            name: leader.as_ref().map(|leader| leader.name.clone()),
            kept_for: "address",
            applied,
            saved: true,
            save_error: None,
            mix: None,
        }))
    });
    desk
}

fn mixes_of(desk: &mut FakeDesk, addresses: &[String]) -> Result<Answer, String> {
    let mut mixes = Vec::new();
    for address in addresses {
        let Some(row) = musician(desk, address) else {
            return Err(format!("no musician is listed at {address}"));
        };
        if let Some(mirror) = &row.mirror {
            return Err(format!(
                "{address} follows {}'s mix: stop the mirror first",
                mirror.name
            ));
        }
        row.mix.revision += 1;
        mixes.push(MusicianMixDto {
            address: address.clone(),
            mix: dto(&row.mix),
        });
    }
    Ok(Answer::Mixes(MixesSetDto { mixes }))
}

fn script_presets(desk: &mut FakeDesk, presets: Presets) {
    let kept = Rc::clone(&presets);
    desk.script("presets", move |_, _| {
        Ok(Answer::Presets(kept.borrow().clone()))
    });
    let kept = Rc::clone(&presets);
    let mut last = 0;
    desk.script("save_preset", move |desk, command| {
        let Command::SavePreset { address, name } = command else {
            unreachable!()
        };
        if !desk.running {
            return Err("no engine is running".to_owned());
        }
        if musician(desk, address).is_none() {
            return Err(format!("no musician is listed at {address}"));
        }
        let name = check_name(&kept.borrow(), name, None)?;
        last += 1;
        let preset = SessionPreset {
            id: format!("p{last}"),
            name,
            mix: Default::default(),
        };
        kept.borrow_mut().push(preset.clone());
        Ok(Answer::PresetsSet(PresetsDto {
            preset: Some(preset),
            presets: kept.borrow().clone(),
        }))
    });
    let kept = Rc::clone(&presets);
    desk.script("rename_preset", move |_, command| {
        let Command::RenamePreset { id, name } = command else {
            unreachable!()
        };
        let name = check_name(&kept.borrow(), name, Some(id))?;
        let mut list = kept.borrow_mut();
        let Some(preset) = list.iter_mut().find(|preset| &preset.id == id) else {
            return Err(format!("no preset {id}"));
        };
        preset.name = name;
        let preset = preset.clone();
        Ok(Answer::PresetsSet(PresetsDto {
            preset: Some(preset),
            presets: list.clone(),
        }))
    });
    let kept = presets;
    desk.script("delete_preset", move |_, command| {
        let Command::DeletePreset { id } = command else {
            unreachable!()
        };
        let mut list = kept.borrow_mut();
        let Some(at) = list.iter().position(|preset| &preset.id == id) else {
            return Err(format!("no preset {id}"));
        };
        let preset = list.remove(at);
        Ok(Answer::PresetsSet(PresetsDto {
            preset: Some(preset),
            presets: list.clone(),
        }))
    });
}

/// A console connected to `desk`, and the Mix screen's model over it.
pub fn screen(desk: FakeDesk) -> (Harness, Model) {
    screen_with(desk, Options::default())
}

/// A console with `options` connected to `desk`, and the Mix screen's model over it.
pub fn screen_with(desk: FakeDesk, options: Options) -> (Harness, Model) {
    let harness = Harness::connected(desk, options);
    let model = Model::new(harness.console.for_owner(Owner::Screen(Section::Mix)));
    (harness, model)
}

/// The screen over the sample engine running on a desk that answers every Mix command.
pub fn connected() -> (Harness, Model) {
    screen(mix_desk(Presets::default()))
}
