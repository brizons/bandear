//! What the editor offers and remembers (`processing-view.js`): the strips it can open and how
//! the arrows step them, the strip presets as listed, the A/B bypass's kept strips, and the
//! failure line where the latest failure wins.

use std::collections::BTreeMap;

use bandear_desktop_console::state::{ShownStrip, pair_units};
use bandear_desktop_console::types::{StripPresetDto, StripSettings};
use bandear_desktop_console::wire::Status;

use crate::strip::{STRIP_CHANNELS, Said, default_strip};

/// A strip the editor can open: one input, or a stereo pair on its first side.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StripChoice {
    /// The first side, 1-based: the strip's channel.
    pub channel: usize,
    /// Every side, 1-based.
    pub sides: Vec<usize>,
    /// "CH 1 · Kick", "CH 2", "CH 3–4 · Keys".
    pub label: String,
}

/// The strips the editor can open, from a status: one per input of the device (the first 32), a
/// stereo pair once on its first side.
pub fn strip_choices(status: Option<&Status>) -> Vec<StripChoice> {
    let Some(status) = status else {
        return Vec::new();
    };
    let count = status
        .source_channels
        .min(status.channels.len())
        .min(STRIP_CHANNELS);
    let channels = &status.channels[..count];
    pair_units(channels)
        .into_iter()
        .map(|unit| {
            let sides: Vec<usize> = unit.iter().map(|index| index + 1).collect();
            let input = if sides.len() == 2 {
                format!("CH {}\u{2013}{}", sides[0], sides[1])
            } else {
                format!("CH {}", sides[0])
            };
            let name = channels[unit[0]].name.trim();
            StripChoice {
                channel: sides[0],
                label: if name.is_empty() {
                    input
                } else {
                    format!("{input} \u{b7} {name}")
                },
                sides,
            }
        })
        .collect()
}

/// The channel of the choice `step` places from the one holding `channel` (held at either end);
/// `channel` itself when no choice holds it.
pub fn step_choice(choices: &[StripChoice], channel: usize, step: i32) -> usize {
    let Some(at) = choices
        .iter()
        .position(|choice| choice.sides.contains(&channel))
    else {
        return channel;
    };
    let last = choices.len() as i64 - 1;
    let next = (at as i64 + i64::from(step)).clamp(0, last) as usize;
    choices[next].channel
}

/// A strip preset as the editor lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresetOption {
    pub name: String,
    /// "Vocal (built-in)", or the engineer's own name.
    pub label: String,
    pub factory: bool,
}

/// The strip presets as the editor lists them, the built-in ones marked.
pub fn preset_options(presets: Option<&[StripPresetDto]>) -> Vec<PresetOption> {
    presets
        .unwrap_or(&[])
        .iter()
        .map(|preset| PresetOption {
            name: preset.name.clone(),
            label: if preset.factory {
                format!("{} (built-in)", preset.name)
            } else {
                preset.name.clone()
            },
            factory: preset.factory,
        })
        .collect()
}

/// The bypassed channels and the strip kept for each.
pub type Ab = BTreeMap<usize, StripSettings>;

/// A/B: bypasses `channel`, whose strip is `strip`; the next map and the strip to send (neutral).
pub fn ab_bypass(ab: &Ab, channel: usize, strip: &StripSettings) -> (Ab, StripSettings) {
    let mut next = ab.clone();
    next.insert(channel, *strip);
    (next, default_strip())
}

/// A/B: ends `channel`'s bypass; the next map and the kept strip to send, if one was kept.
pub fn ab_restore(ab: &Ab, channel: usize) -> (Ab, Option<StripSettings>) {
    let mut next = ab.clone();
    let kept = next.remove(&channel);
    (next, kept)
}

/// A/B: the map once the desk's strip of `channel` is known. A bypassed strip is neutral at the
/// desk; one the desk shows working, with no edit of the editor's on its way, was changed elsewhere
/// (a session load, a preset), so the bypass ends there and the kept strip is not sent back over it.
pub fn ab_settle(ab: &Ab, channel: usize, strip: &ShownStrip) -> Ab {
    if !ab.contains_key(&channel) || !strip.active || strip.pending {
        return ab.clone();
    }
    let mut next = ab.clone();
    next.remove(&channel);
    next
}

/// The editor's failure line: the latest failure wins. [FailureLine::sent] notes the answer to an
/// effect send of a channel; [FailureLine::text] is the line for the channel shown, its outcome and
/// the strips' error: a refused send of the channel shown, unless a failure of the editor's own
/// commands (or the strips' error) came after it. A refused send is forgotten once another channel
/// is shown, so it never stands over a channel it was not about.
#[derive(Clone, Debug, Default)]
pub struct FailureLine {
    order: u64,
    send: Option<(usize, String, u64)>,
    other: (String, u64),
}

impl FailureLine {
    /// The answer to a send of `channel`: a refusal is kept, a success clears it.
    pub fn sent(&mut self, channel: usize, outcome: &Said) {
        self.order += 1;
        self.send = (!outcome.ok).then(|| (channel, outcome.message.clone(), self.order));
    }

    /// The line for `channel` shown with the editor's last `outcome` and the strips' `error`.
    pub fn text(&mut self, channel: usize, outcome: Option<&Said>, error: Option<&str>) -> String {
        let said = match outcome {
            Some(outcome) if !outcome.ok => outcome.message.clone(),
            _ => error.unwrap_or("").to_owned(),
        };
        if said != self.other.0 {
            self.order += 1;
            self.other = (said, self.order);
        }
        if self
            .send
            .as_ref()
            .is_some_and(|(sent, _, _)| *sent != channel)
        {
            self.send = None;
        }
        match &self.send {
            Some((_, message, at)) if self.other.0.is_empty() || *at > self.other.1 => {
                message.clone()
            }
            _ => self.other.0.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use bandear_desktop_console::wire::Channel;

    use super::*;
    use crate::strip::tests::compressed;

    #[test]
    fn the_channel_choices_are_the_devices_inputs_a_stereo_pair_once_and_prev_next_step_them() {
        let channel = |number: u8, name: &str, link: bool| Channel {
            number,
            name: name.to_owned(),
            link,
            ..Channel::default()
        };
        let status = Status {
            source_channels: 4,
            channels: vec![
                channel(1, "Kick", false),
                channel(2, "", false),
                channel(3, "Keys", true),
                channel(4, "Keys R", false),
                channel(5, "Spare", false),
            ],
            ..Status::default()
        };
        let choices = strip_choices(Some(&status));
        assert_eq!(
            choices,
            [
                StripChoice {
                    channel: 1,
                    sides: vec![1],
                    label: "CH 1 \u{b7} Kick".to_owned()
                },
                StripChoice {
                    channel: 2,
                    sides: vec![2],
                    label: "CH 2".to_owned()
                },
                StripChoice {
                    channel: 3,
                    sides: vec![3, 4],
                    label: "CH 3\u{2013}4 \u{b7} Keys".to_owned()
                },
            ]
        );
        assert_eq!(step_choice(&choices, 1, 1), 2);
        assert_eq!(
            step_choice(&choices, 4, -1),
            2,
            "the second side steps from its pair"
        );
        assert_eq!(step_choice(&choices, 3, 1), 3, "the last stays the last");
        assert_eq!(step_choice(&choices, 1, -1), 1);
        assert_eq!(step_choice(&[], 1, 1), 1);
        assert!(strip_choices(None).is_empty());
    }

    #[test]
    fn the_presets_are_listed_with_the_built_in_ones_marked() {
        let preset = |name: &str, factory| StripPresetDto {
            name: name.to_owned(),
            settings: StripSettings::default(),
            factory,
        };
        assert_eq!(
            preset_options(Some(&[preset("Vocal", true), preset("Mine", false)])),
            [
                PresetOption {
                    name: "Vocal".to_owned(),
                    label: "Vocal (built-in)".to_owned(),
                    factory: true
                },
                PresetOption {
                    name: "Mine".to_owned(),
                    label: "Mine".to_owned(),
                    factory: false
                },
            ]
        );
        assert!(preset_options(None).is_empty());
    }

    #[test]
    fn a_b_keeps_the_strip_sends_a_neutral_one_and_restores_the_kept_one() {
        let strip = compressed(-18.0);
        let (bypassed, send) = ab_bypass(&Ab::new(), 2, &strip);
        assert_eq!(send, default_strip());
        assert_eq!(bypassed, Ab::from([(2, strip)]));
        let (restored, kept) = ab_restore(&bypassed, 2);
        assert_eq!(kept, Some(strip));
        assert!(restored.is_empty());
        assert_eq!(
            ab_restore(&Ab::new(), 2).1,
            None,
            "nothing kept, nothing to send"
        );

        let shown = |active, pending| ShownStrip {
            channel: 2,
            settings: strip,
            active,
            pending,
        };
        assert!(
            ab_settle(&bypassed, 2, &shown(true, false)).is_empty(),
            "changed elsewhere while bypassed: the bypass ends"
        );
        assert_eq!(ab_settle(&bypassed, 2, &shown(false, false)), bypassed);
        assert_eq!(ab_settle(&bypassed, 2, &shown(true, true)), bypassed);
    }

    #[test]
    fn the_editors_failure_line_says_the_latest_failure_and_forgets_a_send_on_another_channel() {
        let mut line = FailureLine::default();
        let refused = Said::refused("CH 5's reverb send was not changed: the engine is stopping.");
        assert_eq!(line.text(5, None, None), "", "nothing failed");

        line.sent(5, &refused);
        assert_eq!(line.text(5, None, None), refused.message);
        let preset = Said::refused("\"Vocal\" was not applied: the engine is stopping.");
        assert_eq!(
            line.text(5, Some(&preset), None),
            preset.message,
            "a later failure stands over the send"
        );
        line.sent(5, &refused);
        assert_eq!(line.text(5, Some(&preset), None), refused.message);
        line.sent(5, &Said::ok("CH 5 is sending 20% to the reverb."));
        assert_eq!(line.text(5, Some(&preset), None), preset.message);

        line.sent(5, &refused);
        assert_eq!(line.text(6, None, None), "");
        assert_eq!(line.text(5, None, None), "", "it does not come back");
        assert_eq!(
            line.text(5, None, Some("The channel processing could not be read.")),
            "The channel processing could not be read."
        );
    }
}
