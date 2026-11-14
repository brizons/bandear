//! The requests that carry a mix's waiting edits to the desk (`mix.js`, `buildMixRequest` and
//! `buildEngineerRequest`): `set_mix` (and `set_local_mix`) for the mix's own fields, and
//! `set_engineer_controls` for the engineer's limit and locks, which `set_mix` never carries.
//!
//! Only edited fields are sent. The engine replaces a table from its first entry, so a change to one
//! channel sends the table from channel 0 up to that channel, the earlier entries as the engine's
//! mix (with the other edits) shows them. Levels go out as whole numbers from 0 to 100, which the
//! engine's integer fields require.

use std::collections::BTreeMap;

use bandear_desktop_console::mix::{EditValue, Edits, Mix, MixField, split_edits};
use bandear_desktop_console::types::dto::{EngineerControlsRequest, MixUpdateRequest};

/// A level as the engine's integer fields take it: 0 to 100.
fn level(value: EditValue) -> i32 {
    i32::from(value.level().min(100))
}

/// What one field of a request carries: a whole-mix value, or a table from its first entry.
enum Reached {
    Level(i32),
    Flag(bool),
    Levels(Vec<i32>),
    Flags(Vec<bool>),
}

/// The fields `edits` reach in `mix`, as the engine replaces them: a scalar on its own, a table up
/// to the furthest edited entry, every value as `mix` with the edits shows it.
fn reached(mix: &Mix, edits: &Edits) -> BTreeMap<MixField, Reached> {
    let view = mix.with_edits(edits);
    let mut reach: BTreeMap<MixField, Option<usize>> = BTreeMap::new();
    for key in edits.keys() {
        let furthest = reach.entry(key.field).or_insert(key.index);
        if let (Some(furthest), Some(index)) = (furthest.as_mut(), key.index) {
            *furthest = (*furthest).max(index);
        }
    }
    let levels = |table: &[u8], index: usize| -> Vec<i32> {
        (0..=index)
            .map(|entry| level(EditValue::Level(table.get(entry).copied().unwrap_or(0))))
            .collect()
    };
    let flags = |table: &[bool], index: usize| -> Vec<bool> {
        (0..=index)
            .map(|entry| table.get(entry).copied().unwrap_or(false))
            .collect()
    };
    reach
        .into_iter()
        .map(|(field, index)| {
            let index = index.unwrap_or(0);
            let value = match field {
                MixField::VolumePercent => Reached::Level(level(view.volume_percent.into())),
                MixField::MaxLevelPercent => Reached::Level(level(view.max_level_percent.into())),
                MixField::EngineerLimitPercent => {
                    Reached::Level(level(view.engineer_limit_percent.into()))
                }
                MixField::Muted => Reached::Flag(view.muted),
                MixField::ChannelGains => Reached::Levels(levels(&view.channel_gains, index)),
                MixField::Pans => Reached::Levels(levels(&view.pans, index)),
                MixField::GroupLevels => Reached::Levels(levels(&view.group_levels, index)),
                MixField::ChannelMuted => Reached::Flags(flags(&view.channel_muted, index)),
                MixField::ChannelSolo => Reached::Flags(flags(&view.channel_solo, index)),
                MixField::LockedChannels => Reached::Flags(flags(&view.locked_channels, index)),
            };
            (field, value)
        })
        .collect()
}

/// The `set_mix` request (or `set_local_mix`'s) that carries `edits` on `mix` (the engine's), or
/// `None` when they hold no edit of the mix's own fields. Edits of the engineer's controls are left
/// out: [build_engineer_request] carries them.
pub fn build_mix_request(mix: &Mix, edits: &Edits) -> Option<MixUpdateRequest> {
    let (own, _) = split_edits(edits);
    if own.is_empty() {
        return None;
    }
    let mut request = MixUpdateRequest::default();
    for (field, value) in reached(mix, &own) {
        match (field, value) {
            (MixField::VolumePercent, Reached::Level(value)) => {
                request.volume_percent = Some(value)
            }
            (MixField::MaxLevelPercent, Reached::Level(value)) => {
                request.max_level_percent = Some(value)
            }
            (MixField::Muted, Reached::Flag(value)) => request.muted = Some(value),
            (MixField::ChannelGains, Reached::Levels(table)) => request.channel_gains = Some(table),
            (MixField::Pans, Reached::Levels(table)) => request.pans = Some(table),
            (MixField::GroupLevels, Reached::Levels(table)) => request.group_levels = Some(table),
            (MixField::ChannelMuted, Reached::Flags(table)) => request.channel_muted = Some(table),
            (MixField::ChannelSolo, Reached::Flags(table)) => request.channel_solo = Some(table),
            _ => {}
        }
    }
    Some(request)
}

/// The `set_engineer_controls` request that carries the engineer's edits among `edits` on `mix`
/// (the others are left out), or `None` when there are none: the limit, and the locks from CH 1 up
/// to the furthest edited channel.
pub fn build_engineer_request(mix: &Mix, edits: &Edits) -> Option<EngineerControlsRequest> {
    let (_, engineer) = split_edits(edits);
    if engineer.is_empty() {
        return None;
    }
    let mut request = EngineerControlsRequest::default();
    for (field, value) in reached(mix, &engineer) {
        match (field, value) {
            (MixField::EngineerLimitPercent, Reached::Level(value)) => {
                request.limit_percent = Some(value)
            }
            (MixField::LockedChannels, Reached::Flags(table)) => {
                request.locked_channels = Some(table)
            }
            _ => {}
        }
    }
    Some(request)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bandear_desktop_console::mix::{MAX_CHANNELS, edits};

    use crate::testing::merge_mix_update;

    /// The request in words, field by field (the desk's request types compare no other way).
    fn shown(request: Option<MixUpdateRequest>) -> String {
        format!("{request:?}")
    }

    fn sent(change: impl FnOnce(&mut MixUpdateRequest)) -> String {
        let mut request = MixUpdateRequest::default();
        change(&mut request);
        shown(Some(request))
    }

    #[test]
    fn with_no_edits_there_is_nothing_to_send() {
        assert!(build_mix_request(&Mix::engine_default(), &Edits::new()).is_none());
    }

    #[test]
    fn a_whole_mix_edit_sends_only_that_field_and_never_the_revision() {
        let mix = Mix {
            revision: 7,
            ..Mix::engine_default()
        };
        let request = build_mix_request(&mix, &edits([("volumePercent", 80.into())]).unwrap());
        assert_eq!(
            shown(request),
            sent(|request| request.volume_percent = Some(80))
        );
    }

    #[test]
    fn one_channels_change_sends_its_table_from_the_first_channel_up_to_it_the_others_as_they_are()
    {
        let mut mix = Mix::engine_default();
        mix.channel_gains[..3].copy_from_slice(&[70, 60, 50]);
        let request = build_mix_request(&mix, &edits([("channelGains.3", 40.into())]).unwrap());
        assert_eq!(
            shown(request),
            sent(|request| request.channel_gains = Some(vec![70, 60, 50, 40]))
        );
    }

    #[test]
    fn several_edits_in_one_table_send_it_up_to_the_furthest_one() {
        let request = build_mix_request(
            &Mix::engine_default(),
            &edits([("pans.4", 50.into()), ("pans.1", 30.into())]).unwrap(),
        );
        assert_eq!(
            shown(request),
            sent(|request| request.pans = Some(vec![0, 30, 0, 100, 50]))
        );
    }

    #[test]
    fn edits_across_tables_and_whole_mix_fields_travel_together() {
        let request = build_mix_request(
            &Mix::engine_default(),
            &edits([
                ("channelMuted.1", true.into()),
                ("channelSolo.0", true.into()),
                ("groupLevels.0", 55.into()),
                ("maxLevelPercent", 90.into()),
                ("muted", true.into()),
            ])
            .unwrap(),
        );
        assert_eq!(
            shown(request),
            sent(|request| {
                request.channel_muted = Some(vec![false, true]);
                request.channel_solo = Some(vec![true]);
                request.group_levels = Some(vec![55]);
                request.max_level_percent = Some(90);
                request.muted = Some(true);
            })
        );
    }

    #[test]
    fn levels_go_out_as_whole_numbers_from_0_to_100_as_the_engines_integer_fields_need() {
        let request = build_mix_request(
            &Mix::engine_default(),
            &edits([
                ("volumePercent", 140.into()),
                ("channelGains.0", 255.into()),
            ])
            .unwrap(),
        );
        assert_eq!(
            shown(request),
            sent(|request| {
                request.volume_percent = Some(100);
                request.channel_gains = Some(vec![100]);
            })
        );
    }

    #[test]
    fn the_engines_merge_of_a_built_request_gives_the_mix_the_edits_show() {
        let mut mix = Mix::engine_default();
        mix.channel_gains[5] = 12;
        mix.group_levels[1] = 33;
        let changes = edits([
            ("channelGains.3", 40.into()),
            ("pans.2", 50.into()),
            ("channelSolo.6", true.into()),
            ("groupLevels.0", 70.into()),
            ("volumePercent", 64.into()),
            ("muted", true.into()),
        ])
        .unwrap();
        let request = build_mix_request(&mix, &changes).expect("a request");
        assert_eq!(merge_mix_update(&mix, &request), mix.with_edits(&changes));
    }

    #[test]
    fn the_engineers_edits_go_out_as_their_own_request_never_in_set_mix() {
        let changes = edits([
            ("channelGains.1", 40.into()),
            ("engineerLimitPercent", 60.into()),
            ("lockedChannels.2", true.into()),
        ])
        .unwrap();
        let mut base = Mix::engine_default();
        base.locked_channels[0] = true;

        let request = build_engineer_request(&base, &changes).expect("the engineer's request");
        assert_eq!(request.limit_percent, Some(60));
        assert_eq!(request.locked_channels, Some(vec![true, false, true]));
        let locks =
            build_engineer_request(&base, &edits([("lockedChannels.1", true.into())]).unwrap())
                .expect("a lock");
        assert_eq!(locks.limit_percent, None);
        assert_eq!(locks.locked_channels, Some(vec![true, true]));
        assert!(build_engineer_request(&base, &Edits::new()).is_none());
        assert_eq!(
            shown(build_mix_request(&base, &changes)),
            sent(|request| request.channel_gains = Some(vec![100, 40])),
            "set_mix carries the mix's own edits only"
        );
        assert!(
            build_mix_request(
                &base,
                &edits([("engineerLimitPercent", 60.into())]).unwrap()
            )
            .is_none(),
            "set_mix cannot carry the engineer's controls"
        );
        assert_eq!(base.locked_channels.len(), MAX_CHANNELS);
    }
}
