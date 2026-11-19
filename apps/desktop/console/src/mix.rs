//! A musician's mix as the console keeps it, and the engineer's edits shown over it (`mix.js`).
//!
//! A mix is the desk's `MixDto`: the engine's full tables of 32 channel and 16 group entries, its
//! whole-mix levels, and the engineer's controls (`engineerLimitPercent`, `lockedChannels`, which
//! only `set_engineer_controls` changes). `revision` is the engine's version of the stored mix:
//! every change has a higher one, so of two copies the higher revision is the newer. It is never
//! edited or sent.
//!
//! A local edit is keyed by field, plus the entry for a table ([EditKey]): `volumePercent`,
//! `channelGains.3`, `lockedChannels.0`. The edits waiting for the engine's answer are [Edits].

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

/// The channel entries of every mix table.
pub const MAX_CHANNELS: usize = 32;
/// The group entries of a mix's group levels.
pub const MAX_GROUPS: usize = 16;

/// A musician's mix (or the engineer's local mix), as the desk sends it.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Mix {
    pub revision: u64,
    pub volume_percent: u8,
    pub max_level_percent: u8,
    pub muted: bool,
    pub channel_gains: Vec<u8>,
    pub pans: Vec<u8>,
    pub channel_muted: Vec<bool>,
    pub channel_solo: Vec<bool>,
    pub group_levels: Vec<u8>,
    pub engineer_limit_percent: u8,
    pub locked_channels: Vec<bool>,
}

impl Default for Mix {
    /// A payload's mix with nothing in it (`mixFromDto({})`): the oldest revision, empty tables,
    /// and, from a desk that predates the engineer's controls, no limit and nothing locked.
    fn default() -> Self {
        Self {
            revision: 0,
            volume_percent: 0,
            max_level_percent: 0,
            muted: false,
            channel_gains: Vec::new(),
            pans: Vec::new(),
            channel_muted: Vec::new(),
            channel_solo: Vec::new(),
            group_levels: Vec::new(),
            engineer_limit_percent: 100,
            locked_channels: vec![false; MAX_CHANNELS],
        }
    }
}

impl Mix {
    /// The mix the engine starts every musician with (`defaultMix`): unity levels, even channels
    /// left, odd right, no limit from the engineer and nothing locked.
    pub fn engine_default() -> Self {
        Self {
            revision: 0,
            volume_percent: 100,
            max_level_percent: 100,
            muted: false,
            channel_gains: vec![100; MAX_CHANNELS],
            pans: (0..MAX_CHANNELS)
                .map(|index| if index % 2 == 0 { 0 } else { 100 })
                .collect(),
            channel_muted: vec![false; MAX_CHANNELS],
            channel_solo: vec![false; MAX_CHANNELS],
            group_levels: vec![100; MAX_GROUPS],
            engineer_limit_percent: 100,
            locked_channels: vec![false; MAX_CHANNELS],
        }
    }

    /// The mix the desk answered a command with.
    pub fn from_core(dto: &bandear_desktop_core::dto::MixDto) -> Self {
        Self {
            revision: dto.revision,
            volume_percent: dto.volume_percent,
            max_level_percent: dto.max_level_percent,
            muted: dto.muted,
            channel_gains: dto.channel_gains.clone(),
            pans: dto.pans.clone(),
            channel_muted: dto.channel_muted.clone(),
            channel_solo: dto.channel_solo.clone(),
            group_levels: dto.group_levels.clone(),
            engineer_limit_percent: dto.engineer_limit_percent,
            locked_channels: dto.locked_channels.clone(),
        }
    }

    /// The mix as it shows with `edits` on top; `self` is left as it was.
    pub fn with_edits(&self, edits: &Edits) -> Mix {
        let mut view = self.clone();
        for (key, value) in edits {
            view.set(*key, *value);
        }
        view
    }
}

impl Mix {
    /// Writes one edit into the mix; a table too short for the entry grows to it.
    fn set(&mut self, key: EditKey, value: EditValue) {
        fn entry<T: Clone + Default>(table: &mut Vec<T>, index: usize, value: T) {
            if table.len() <= index {
                table.resize(index + 1, T::default());
            }
            table[index] = value;
        }
        let index = key.index.unwrap_or(0);
        match key.field {
            MixField::VolumePercent => self.volume_percent = value.level(),
            MixField::MaxLevelPercent => self.max_level_percent = value.level(),
            MixField::Muted => self.muted = value.flag(),
            MixField::EngineerLimitPercent => self.engineer_limit_percent = value.level(),
            MixField::ChannelGains => entry(&mut self.channel_gains, index, value.level()),
            MixField::Pans => entry(&mut self.pans, index, value.level()),
            MixField::ChannelMuted => entry(&mut self.channel_muted, index, value.flag()),
            MixField::ChannelSolo => entry(&mut self.channel_solo, index, value.flag()),
            MixField::GroupLevels => entry(&mut self.group_levels, index, value.level()),
            MixField::LockedChannels => entry(&mut self.locked_channels, index, value.flag()),
        }
    }
}

/// A field of a mix an edit can change.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MixField {
    VolumePercent,
    MaxLevelPercent,
    Muted,
    EngineerLimitPercent,
    ChannelGains,
    Pans,
    ChannelMuted,
    ChannelSolo,
    GroupLevels,
    LockedChannels,
}

impl MixField {
    /// Every field an edit can change.
    pub const ALL: [MixField; 10] = [
        MixField::VolumePercent,
        MixField::MaxLevelPercent,
        MixField::Muted,
        MixField::EngineerLimitPercent,
        MixField::ChannelGains,
        MixField::Pans,
        MixField::ChannelMuted,
        MixField::ChannelSolo,
        MixField::GroupLevels,
        MixField::LockedChannels,
    ];

    /// The field's name in the desk's JSON.
    pub fn name(self) -> &'static str {
        match self {
            Self::VolumePercent => "volumePercent",
            Self::MaxLevelPercent => "maxLevelPercent",
            Self::Muted => "muted",
            Self::EngineerLimitPercent => "engineerLimitPercent",
            Self::ChannelGains => "channelGains",
            Self::Pans => "pans",
            Self::ChannelMuted => "channelMuted",
            Self::ChannelSolo => "channelSolo",
            Self::GroupLevels => "groupLevels",
            Self::LockedChannels => "lockedChannels",
        }
    }

    /// The entries of a table field, or `None` for a whole-mix field.
    pub fn table_size(self) -> Option<usize> {
        match self {
            Self::VolumePercent
            | Self::MaxLevelPercent
            | Self::Muted
            | Self::EngineerLimitPercent => None,
            Self::GroupLevels => Some(MAX_GROUPS),
            Self::ChannelGains
            | Self::Pans
            | Self::ChannelMuted
            | Self::ChannelSolo
            | Self::LockedChannels => Some(MAX_CHANNELS),
        }
    }

    /// True for a field of flags (mutes, solos, locks), false for a level.
    pub fn is_flag(self) -> bool {
        matches!(
            self,
            Self::Muted | Self::ChannelMuted | Self::ChannelSolo | Self::LockedChannels
        )
    }

    /// True for the engineer's controls, which only `set_engineer_controls` changes.
    pub fn is_engineer(self) -> bool {
        matches!(self, Self::EngineerLimitPercent | Self::LockedChannels)
    }
}

/// The key of one edit: a whole-mix field, or one entry of a table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EditKey {
    pub field: MixField,
    /// The table entry, from 0; `None` for a whole-mix field.
    pub index: Option<usize>,
}

impl EditKey {
    /// The key of `field` (and its entry `index` for a table). A key the engine would not accept
    /// is refused rather than sent and silently changing nothing.
    pub fn new(field: MixField, index: Option<usize>) -> Result<Self, String> {
        let key = match index {
            Some(index) => format!("{}.{index}", field.name()),
            None => field.name().to_owned(),
        };
        Self::parse(&key)
    }

    /// The key a string names (`volumePercent`, `channelGains.3`).
    pub fn parse(key: &str) -> Result<Self, String> {
        let mut parts = key.split('.');
        let name = parts.next().unwrap_or("");
        let entry = parts.next();
        let field = MixField::ALL.into_iter().find(|field| field.name() == name);
        let Some(field) = field.filter(|_| parts.next().is_none()) else {
            return Err(format!("unknown mix edit: {key}"));
        };
        let Some(size) = field.table_size() else {
            return match entry {
                None => Ok(Self { field, index: None }),
                Some(_) => Err(format!("mix edit {key}: {name} has no entries")),
            };
        };
        let index = entry
            .filter(|entry| !entry.is_empty() && entry.bytes().all(|byte| byte.is_ascii_digit()))
            .and_then(|entry| entry.parse::<usize>().ok())
            .filter(|index| *index < size);
        match index {
            Some(index) => Ok(Self {
                field,
                index: Some(index),
            }),
            None => Err(format!(
                "mix edit {key}: {name} needs an entry from 0 to {}",
                size - 1
            )),
        }
    }
}

impl fmt::Display for EditKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.index {
            Some(index) => write!(formatter, "{}.{index}", self.field.name()),
            None => formatter.write_str(self.field.name()),
        }
    }
}

/// An edit's value: a level (0-100) or a flag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditValue {
    Level(u8),
    Flag(bool),
}

impl EditValue {
    /// The value as a level; a flag is 0 or 1.
    pub fn level(self) -> u8 {
        match self {
            Self::Level(level) => level,
            Self::Flag(flag) => u8::from(flag),
        }
    }

    /// The value as a flag; a level is on unless 0.
    pub fn flag(self) -> bool {
        match self {
            Self::Level(level) => level != 0,
            Self::Flag(flag) => flag,
        }
    }
}

impl From<u8> for EditValue {
    fn from(level: u8) -> Self {
        Self::Level(level)
    }
}

impl From<bool> for EditValue {
    fn from(flag: bool) -> Self {
        Self::Flag(flag)
    }
}

/// Edits of one mix, by key.
pub type Edits = BTreeMap<EditKey, EditValue>;

/// The edits `pairs` name, keyed as [EditKey::parse] reads them.
pub fn edits<const N: usize>(pairs: [(&str, EditValue); N]) -> Result<Edits, String> {
    pairs
        .into_iter()
        .map(|(key, value)| Ok((EditKey::parse(key)?, value)))
        .collect()
}

/// The edits still waiting after the engine answered a request that carried `sent`: an edit is
/// settled when its value is still the one sent, and kept when it changed since or was not sent.
pub fn settle_edits(edits: Option<&Edits>, sent: &Edits) -> Edits {
    edits
        .into_iter()
        .flatten()
        .filter(|(key, value)| sent.get(key) != Some(value))
        .map(|(key, value)| (*key, *value))
        .collect()
}

/// `edits` in two: the edits `set_mix` carries, and the edits of the engineer's controls that
/// `set_engineer_controls` carries.
pub fn split_edits(edits: &Edits) -> (Edits, Edits) {
    edits
        .iter()
        .map(|(key, value)| (*key, *value))
        .partition(|(key, _)| !key.field.is_engineer())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_default_mix_mirrors_the_engine() {
        let mix = Mix::engine_default();
        assert_eq!(mix.channel_gains.len(), MAX_CHANNELS);
        assert_eq!(mix.pans.len(), MAX_CHANNELS);
        assert_eq!(mix.channel_muted.len(), MAX_CHANNELS);
        assert_eq!(mix.channel_solo.len(), MAX_CHANNELS);
        assert_eq!(mix.group_levels.len(), MAX_GROUPS);
        assert!(mix.channel_gains.iter().all(|gain| *gain == 100));
        assert_eq!(mix.pans[..4], [0, 100, 0, 100]);
        assert_eq!(mix.volume_percent, 100);
        assert_eq!(mix.max_level_percent, 100);
        assert!(!mix.muted);
        assert_eq!(mix.revision, 0);
    }

    #[test]
    fn a_mix_keeps_the_engines_revision_and_a_payload_without_one_reads_as_the_oldest() {
        let mix: Mix =
            serde_json::from_value(json!({ "revision": 41, "volumePercent": 42 })).unwrap();
        assert_eq!(mix.revision, 41);
        assert_eq!(mix.volume_percent, 42);
        let old: Mix = serde_json::from_value(json!({ "volumePercent": 42 })).unwrap();
        assert_eq!(old.revision, 0);
        assert_eq!(
            old.engineer_limit_percent, 100,
            "no limit from an older desk"
        );
        assert_eq!(old.locked_channels, vec![false; MAX_CHANNELS]);
    }

    #[test]
    fn edit_keys_name_a_whole_mix_field_or_one_entry_of_a_table() {
        let key = EditKey::new(MixField::ChannelGains, Some(3)).unwrap();
        assert_eq!(key.to_string(), "channelGains.3");
        assert_eq!(
            EditKey::new(MixField::VolumePercent, None)
                .unwrap()
                .to_string(),
            "volumePercent"
        );
        assert_eq!(EditKey::parse("channelGains.3"), Ok(key));
        assert_eq!(
            EditKey::parse("muted"),
            Ok(EditKey {
                field: MixField::Muted,
                index: None
            })
        );
        assert_eq!(
            EditKey::parse("groupLevels.15"),
            Ok(EditKey {
                field: MixField::GroupLevels,
                index: Some(15)
            })
        );
    }

    #[test]
    fn an_edit_the_engine_would_not_accept_fails_loudly() {
        for key in [
            "volume",
            "revision",
            "channelGains",
            "channelGains.32",
            "groupLevels.16",
            "muted.0",
            "pans.-1",
            "pans.x",
            "pans.1.2",
        ] {
            let error = EditKey::parse(key).expect_err(key);
            assert!(error.contains("mix edit"), "{key}: {error}");
        }
        assert!(EditKey::new(MixField::ChannelGains, None).is_err());
    }

    #[test]
    fn edits_show_on_top_of_a_mix_without_changing_it() {
        let mix = Mix {
            revision: 7,
            ..Mix::engine_default()
        };
        let shown = mix.with_edits(
            &edits([
                ("channelGains.2", 40.into()),
                ("muted", true.into()),
                ("pans.0", 50.into()),
            ])
            .unwrap(),
        );
        assert_eq!(shown.channel_gains[2], 40);
        assert_eq!(shown.pans[0], 50);
        assert!(shown.muted);
        assert_eq!(shown.channel_gains[1], 100);
        assert_eq!(shown.revision, 7, "the revision is the engine's");
        assert_eq!(mix.channel_gains[2], 100);
        assert_eq!(mix.pans[0], 0);
        assert!(!mix.muted);
    }

    #[test]
    fn an_answer_settles_the_edits_it_carried_and_keeps_those_changed_since() {
        let sent = edits([
            ("channelGains.0", 40.into()),
            ("channelGains.1", 50.into()),
            ("volumePercent", 80.into()),
        ])
        .unwrap();
        let now = edits([
            ("channelGains.0", 40.into()),
            ("channelGains.1", 55.into()),
            ("pans.0", 50.into()),
        ])
        .unwrap();
        assert_eq!(
            settle_edits(Some(&now), &sent),
            edits([("channelGains.1", 55.into()), ("pans.0", 50.into())]).unwrap()
        );
    }

    #[test]
    fn when_every_edit_was_answered_nothing_stays_pending() {
        let muted = edits([("muted", true.into())]).unwrap();
        assert!(settle_edits(Some(&muted), &muted).is_empty());
        assert!(settle_edits(None, &muted).is_empty());
    }

    #[test]
    fn the_engineers_controls_travel_apart_from_the_mix() {
        let both = edits([
            ("volumePercent", 80.into()),
            ("lockedChannels.0", true.into()),
        ])
        .unwrap();
        let (mix, engineer) = split_edits(&both);
        assert_eq!(mix, edits([("volumePercent", 80.into())]).unwrap());
        assert_eq!(
            engineer,
            edits([("lockedChannels.0", true.into())]).unwrap()
        );
    }
}
