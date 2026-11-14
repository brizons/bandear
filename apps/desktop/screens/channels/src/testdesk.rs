//! The desk's channel and group commands for the model's tests, on the console's simulated desk:
//! `channel_table` reads the whole table the desk keeps (entries past the device included),
//! `set_channels` keeps a new one and fits it to the device in use, and `set_groups` keeps the
//! groups (a new one gets an id) and shows them in the status, numbered from 0. Each answers as the
//! desk does: applied when an engine runs, saved always.

use std::cell::RefCell;
use std::rc::Rc;

use bandear_desktop_console::command::{Answer, Command};
use bandear_desktop_console::testing::FakeDesk;
use bandear_desktop_console::types::SessionChannel;
use bandear_desktop_console::types::dto::{ChannelsSetDto, GroupsSetDto};
use bandear_desktop_console::wire::{Channel, Group};
use serde_json::json;

use crate::channels::{check_channels, fit_channels};

/// What the desk keeps of the engineer's channel details and groups.
#[derive(Clone, Debug)]
pub struct Kept {
    pub table: Vec<SessionChannel>,
    /// Every group with its id and its inputs, numbered from 1.
    pub groups: Vec<(String, String, Vec<usize>)>,
    next_id: usize,
}

/// The desk's details, shared by the scripts and the test.
pub type Shared = Rc<RefCell<Kept>>;

/// Scripts `desk` with the channel and group commands, starting from the sample status's inputs
/// and groups.
pub fn channel_desk(desk: &mut FakeDesk) -> Shared {
    let status = &desk.report.status;
    let kept = Rc::new(RefCell::new(Kept {
        table: status
            .channels
            .iter()
            .map(|channel| SessionChannel {
                name: channel.name.clone(),
                kind: channel.kind.clone(),
                hidden: channel.hidden,
                link: channel.link,
            })
            .collect(),
        groups: status
            .groups
            .iter()
            .map(|group| {
                (
                    group.id.clone(),
                    group.name.clone(),
                    group.channels.iter().map(|index| index + 1).collect(),
                )
            })
            .collect(),
        next_id: 0,
    }));

    let table = Rc::clone(&kept);
    desk.script("channel_table", move |_, _| {
        Ok(Answer::ChannelTable(table.borrow().table.clone()))
    });

    let table = Rc::clone(&kept);
    desk.script("set_channels", move |desk, command| {
        let Command::SetChannels { channels } = command else {
            return Err("not set_channels".to_owned());
        };
        let next: Vec<SessionChannel> = channels
            .iter()
            .map(|channel| SessionChannel {
                name: channel.name.trim().to_owned(),
                kind: channel.kind.clone(),
                hidden: channel.hidden,
                link: channel.link,
            })
            .collect();
        let before = table.borrow().table.clone();
        check_channels(&next, &before)?;
        table.borrow_mut().table = next.clone();
        show(desk, &table.borrow());
        Ok(Answer::ChannelsSet(ChannelsSetDto {
            channels: next,
            applied: desk.running,
            saved: true,
            save_error: None,
        }))
    });

    let groups = Rc::clone(&kept);
    desk.script("set_groups", move |desk, command| {
        let Command::SetGroups { groups: asked } = command else {
            return Err("not set_groups".to_owned());
        };
        {
            let mut kept = groups.borrow_mut();
            let mut next = Vec::new();
            for group in asked {
                let id = match &group.id {
                    Some(id) => id.clone(),
                    None => {
                        kept.next_id += 1;
                        format!("g{}", kept.next_id)
                    }
                };
                next.push((id, group.name.clone(), group.channels.clone()));
            }
            kept.groups = next;
        }
        let kept = groups.borrow();
        show(desk, &kept);
        let answer: Vec<_> = kept
            .groups
            .iter()
            .map(|(id, name, channels)| json!({ "id": id, "name": name, "channels": channels }))
            .collect();
        Ok(Answer::GroupsSet(GroupsSetDto {
            groups: serde_json::from_value(json!(answer)).expect("the desk's groups"),
            applied: desk.running,
            saved: true,
            save_error: None,
        }))
    });
    kept
}

/// Puts what the desk keeps on the status of the device in use (its first `source_channels`
/// inputs), as the engine shows it, and sends that status.
pub fn show(desk: &mut FakeDesk, kept: &Kept) {
    let status = &mut desk.report.status;
    let count = status.source_channels;
    let fitted = fit_channels(&kept.table, count);
    status.groups = kept
        .groups
        .iter()
        .map(|(id, name, channels)| Group {
            id: id.clone(),
            name: name.clone(),
            channels: channels.iter().map(|number| number - 1).collect(),
            invalid: channels.iter().any(|number| *number > count),
        })
        .collect();
    let groups = status.groups.clone();
    status.channels = fitted
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let group = groups
                .iter()
                .find(|group| !group.invalid && group.channels.contains(&index));
            Channel {
                number: index as u8 + 1,
                name: entry.name.clone(),
                kind: entry.kind.clone(),
                hidden: entry.hidden,
                link: entry.link,
                group: group.map(|group| group.name.clone()),
                group_id: group.map(|group| group.id.clone()),
                level: 0,
            }
        })
        .collect();
    if desk.running {
        desk.emit_status();
    }
}

/// Puts the engine on a device with `count` inputs, as a device switch does: the details the desk
/// keeps for the inputs past it stay kept.
pub fn use_device(desk: &mut FakeDesk, kept: &Shared, count: usize) {
    desk.report.status.source_channels = count;
    desk.report.status.capture_format.channels = count as u16;
    show(desk, &kept.borrow());
}
