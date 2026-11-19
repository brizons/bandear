//! The two layouts a wake-up crosses JNI in, version 1. `NativeReceivePipeline.kt` (package
//! `com.bandear.android.core`) mirrors both; a change here changes it and [`LAYOUT_VERSION`].
//!
//! # A wake-up in: 64-bit words
//!
//! Every field of the core's [`Wake`] but the datagram, in a `long[]` the Kotlin side reuses: one
//! word per field, in the order [`Wake`] declares them. An optional value takes two words, a
//! presence marker (0 absent, 1 present) and the value, which is ignored while absent; a flag is
//! 0 or 1. Times are nanoseconds of one monotonic clock, as unsigned 64-bit numbers in the word's
//! bits.
//!
//! | Word  | Field                                                         |
//! |-------|---------------------------------------------------------------|
//! | 0     | `started_ns`                                                  |
//! | 1     | `output_at_start` present                                     |
//! | 2-7   | `output_at_start`, as readings (below)                        |
//! | 8     | `applied_target_frames` present                               |
//! | 9     | `applied_target_frames`                                       |
//! | 10    | `received_ns`                                                 |
//! | 11    | `output_at_receive` present                                   |
//! | 12-17 | `output_at_receive`, as readings                              |
//! | 18    | `settings.volume_percent`                                     |
//! | 19    | `settings.max_level_percent`                                  |
//! | 20    | `settings.muted`                                              |
//! | 21    | `settings.server_owns_mix`                                    |
//! | 22    | `settings.playback_buffer` is `Fixed` (0 for `Auto`)          |
//! | 23    | its milliseconds, clamped to 0-65 535                         |
//! | 24    | `settings.redundancy_hold_packets`                            |
//!
//! [`OutputReadings`] take six words, here and alone for `on_output_opened`: 0 `writable_frames`
//! present, 1 `writable_frames`, 2 `underrun_count`, 3 `presented_frame` present,
//! 4 `presented_frame`, 5 `is_playing`.
//!
//! # An outcome out: bytes
//!
//! The core's [`WakeOutcome`], encoded into the Kotlin side's direct `ByteBuffer`. Every number is
//! little endian, and every record starts on an 8-byte boundary, so a write's samples do too.
//!
//! A 16-byte header: `u32` layout version ([`LAYOUT_VERSION`]), `u32` record count, `i64`
//! `report_due_at_ms`. Then the records, in the order the platform acts on them: the report, the
//! output actions in order, the stats in order, the error, the output to open. A record is a `u32`
//! kind, a `u32` length in bytes (its 8-byte head included, a multiple of 8), then its fields, 8
//! bytes each: integers as `i64`, real numbers as `f64`, flags as 0 or 1. A record with optional
//! values starts with a presence word, one bit per optional value in the order listed, and an
//! absent value's word is 0.
//!
//! | Kind | Record                | Fields                                                       |
//! |------|-----------------------|--------------------------------------------------------------|
//! | 1    | [`REPORT`]            | presence (bit 0 `jitter_ms`, 1 `cushion_ms`, 2 `backlog_ms`), `interval_ms`, `packets_received`, `packets_lost`, `underruns`, `receive_timeouts`, `jitter_ms`, `cushion_ms`, `backlog_ms`, `recovered` |
//! | 2    | [`WRITE`]             | the byte count, then the samples: interleaved PCM16, little endian, as the output takes them, zero-padded to 8 bytes |
//! | 3    | [`SET_TARGET_FRAMES`] | `frames`                                                     |
//! | 4    | [`STATS`]             | presence (bit 0 `inter_arrival_ms`, 1 `jitter_ms`, 2 `playback_backlog_ms`), `packets`, `lost`, `sample_rate`, `packets_per_second`, `inter_arrival_ms`, `jitter_ms`, `discarded_samples`, `packets_with_discarded_samples`, `playback_backlog_ms`, `output_playing` |
//! | 5    | [`ERROR`]             | a code and its two fields (below; an unused field is 0)      |
//! | 6    | [`OPEN_OUTPUT`]       | `sample_rate`, `packet_frames`                               |
//!
//! Error codes, a [`ReceiveError`] each and the [`DecodeError`] behind `Undecodable`, with their
//! fields: 1 header too short (`byte_count`); 2 unknown magic (its four bytes, the first in the
//! lowest byte); 3 unsupported version (`version`); 4 zero channels; 5 invalid sample rate
//! (`sample_rate`); 6 payload length mismatch (`declared_sample_count`, `payload_byte_count`);
//! 7 sample count not divisible by channels (`sample_count`, `channels`); 8 sample buffer too
//! small (`sample_count`, `capacity`); 9 too many channels (`channels`); 10 sample rate changed
//! (`from`, `to`).

use crate::receive_pipeline::count;
use bandear_core::cushion::PlaybackBufferSetting;
use bandear_core::link_stats::LinkStatsReport;
use bandear_core::packet::DecodeError;
use bandear_core::receive_pipeline::{
    OutputAction, OutputFormat, OutputReadings, PlayerSettings, ReceiveError, ReceiverStats, Wake,
    WakeOutcome,
};

/// The version both layouts are at; the outcome's header carries it.
pub(crate) const LAYOUT_VERSION: u32 = 1;

/// The words a wake-up takes.
pub(crate) const WAKE_WORDS: usize = 25;

/// The words output readings take.
pub(crate) const READINGS_WORDS: usize = 6;

/// The record kinds.
pub(crate) const REPORT: u32 = 1;
pub(crate) const WRITE: u32 = 2;
pub(crate) const SET_TARGET_FRAMES: u32 = 3;
pub(crate) const STATS: u32 = 4;
pub(crate) const ERROR: u32 = 5;
pub(crate) const OPEN_OUTPUT: u32 = 6;

/// The bytes of the outcome's header, of a record's head, and of a field.
const HEADER_BYTES: usize = 16;
const RECORD_HEAD_BYTES: usize = 8;
const FIELD_BYTES: usize = 8;

/// The fields of the records whose length does not vary.
const REPORT_FIELDS: usize = 10;
const SET_TARGET_FRAMES_FIELDS: usize = 1;
const STATS_FIELDS: usize = 11;
const ERROR_FIELDS: usize = 3;
const OPEN_OUTPUT_FIELDS: usize = 2;

/// The wake-up the Kotlin side packed into `words`, with the datagram it received.
pub(crate) fn wake(words: &[i64; WAKE_WORDS], datagram: Option<Vec<u8>>) -> Wake {
    let readings_at = |first: usize| {
        let mut packed = [0; READINGS_WORDS];
        packed.copy_from_slice(&words[first..first + READINGS_WORDS]);
        readings(&packed)
    };
    Wake {
        started_ns: words[0] as u64,
        output_at_start: present(words[1]).then(|| readings_at(2)),
        applied_target_frames: optional(words[8], words[9]),
        datagram,
        received_ns: words[10] as u64,
        output_at_receive: present(words[11]).then(|| readings_at(12)),
        settings: PlayerSettings {
            volume_percent: words[18],
            max_level_percent: words[19],
            muted: present(words[20]),
            server_owns_mix: present(words[21]),
            playback_buffer: if present(words[22]) {
                PlaybackBufferSetting::Fixed {
                    milliseconds: words[23].clamp(0, i64::from(u16::MAX)) as u16,
                }
            } else {
                PlaybackBufferSetting::Auto
            },
            redundancy_hold_packets: words[24],
        },
    }
}

/// Output readings packed into six words.
pub(crate) fn readings(words: &[i64; READINGS_WORDS]) -> OutputReadings {
    OutputReadings {
        writable_frames: optional(words[0], words[1]),
        underrun_count: words[2],
        presented_frame: optional(words[3], words[4]),
        is_playing: present(words[5]),
    }
}

/// A presence marker or a flag: anything but 0 is set.
fn present(word: i64) -> bool {
    word != 0
}

fn optional(marker: i64, value: i64) -> Option<i64> {
    present(marker).then_some(value)
}

/// The bytes `outcome` takes encoded.
pub(crate) fn encoded_len(outcome: &WakeOutcome) -> usize {
    let WakeOutcome {
        report,
        actions,
        stats,
        error,
        open_output,
        report_due_at_ms: _,
    } = outcome;
    let actions: usize = actions
        .iter()
        .map(|action| match action {
            OutputAction::Write { samples } => write_record_len(samples),
            OutputAction::SetTargetFrames { .. } => record_len(SET_TARGET_FRAMES_FIELDS),
        })
        .sum();
    HEADER_BYTES
        + report.map_or(0, |_| record_len(REPORT_FIELDS))
        + actions
        + stats.len() * record_len(STATS_FIELDS)
        + error.map_or(0, |_| record_len(ERROR_FIELDS))
        + open_output.map_or(0, |_| record_len(OPEN_OUTPUT_FIELDS))
}

fn record_len(fields: usize) -> usize {
    RECORD_HEAD_BYTES + fields * FIELD_BYTES
}

/// A write's record: its head, the byte count, the samples padded to a whole field.
fn write_record_len(samples: &[i16]) -> usize {
    record_len(1) + std::mem::size_of_val(samples).next_multiple_of(FIELD_BYTES)
}

/// Encodes `outcome` at the start of `out`, answering the bytes written; when it does not fit,
/// answers the bytes it needs as the error and leaves `out` untouched.
pub(crate) fn encode(outcome: &WakeOutcome, out: &mut [u8]) -> Result<usize, usize> {
    let needed = encoded_len(outcome);
    let Some(out) = out.get_mut(..needed) else {
        return Err(needed);
    };
    let WakeOutcome {
        report,
        actions,
        stats,
        error,
        open_output,
        report_due_at_ms,
    } = outcome;
    let records = usize::from(report.is_some())
        + actions.len()
        + stats.len()
        + usize::from(error.is_some())
        + usize::from(open_output.is_some());

    let mut writer = Writer { out, at: 0 };
    writer.u32(LAYOUT_VERSION);
    writer.u32(records as u32);
    writer.int(*report_due_at_ms);
    if let Some(report) = report {
        writer.report(report);
    }
    for action in actions {
        match action {
            OutputAction::Write { samples } => writer.write(samples),
            OutputAction::SetTargetFrames { frames } => {
                writer.head(SET_TARGET_FRAMES, record_len(SET_TARGET_FRAMES_FIELDS));
                writer.int(*frames);
            }
        }
    }
    for stats in stats {
        writer.stats(stats);
    }
    if let Some(error) = error {
        let (code, first, second) = error_fields(*error);
        writer.head(ERROR, record_len(ERROR_FIELDS));
        writer.word(code);
        writer.word(first);
        writer.word(second);
    }
    if let Some(OutputFormat {
        sample_rate,
        packet_frames,
    }) = open_output
    {
        writer.head(OPEN_OUTPUT, record_len(OPEN_OUTPUT_FIELDS));
        writer.int(*sample_rate);
        writer.int(*packet_frames);
    }
    debug_assert_eq!(writer.at, needed, "encoded_len disagrees with encode");
    Ok(needed)
}

/// An error's code and fields (the module's table). Every case is matched, so a case added to the
/// core fails to compile here until the layout carries it.
fn error_fields(error: ReceiveError) -> (u64, u64, u64) {
    match error {
        ReceiveError::Undecodable { reason } => match reason {
            DecodeError::HeaderTooShort { byte_count } => (1, count(byte_count), 0),
            DecodeError::UnknownMagic(magic) => (2, u32::from_le_bytes(magic).into(), 0),
            DecodeError::UnsupportedVersion(version) => (3, version.into(), 0),
            DecodeError::ZeroChannels => (4, 0, 0),
            DecodeError::InvalidSampleRate(sample_rate) => (5, sample_rate.into(), 0),
            DecodeError::PayloadLengthMismatch {
                declared_sample_count,
                payload_byte_count,
            } => (6, count(declared_sample_count), count(payload_byte_count)),
            DecodeError::SampleCountNotDivisibleByChannels {
                sample_count,
                channels,
            } => (7, count(sample_count), channels.into()),
            DecodeError::SampleBufferTooSmall {
                sample_count,
                capacity,
            } => (8, count(sample_count), count(capacity)),
        },
        ReceiveError::TooManyChannels { channels } => (9, channels as u64, 0),
        ReceiveError::SampleRateChanged { from, to } => (10, from as u64, to as u64),
    }
}

/// Writes the layout's numbers in order into a slice [`encoded_len`] sized.
struct Writer<'a> {
    out: &'a mut [u8],
    at: usize,
}

impl Writer<'_> {
    fn bytes(&mut self, bytes: &[u8]) {
        self.out[self.at..self.at + bytes.len()].copy_from_slice(bytes);
        self.at += bytes.len();
    }

    fn u32(&mut self, value: u32) {
        self.bytes(&value.to_le_bytes());
    }

    /// A field: a word of 8 bytes.
    fn word(&mut self, value: u64) {
        self.bytes(&value.to_le_bytes());
    }

    fn int(&mut self, value: i64) {
        self.word(value as u64);
    }

    fn real(&mut self, value: f64) {
        self.word(value.to_bits());
    }

    fn flag(&mut self, value: bool) {
        self.word(value.into());
    }

    /// An optional real number: its value, 0 when absent (its presence bit says which).
    fn optional_real(&mut self, value: Option<f64>) {
        self.word(value.map_or(0, f64::to_bits));
    }

    /// A record's head. A direct buffer holds at most `i32::MAX` bytes (its capacity is a Java
    /// `int`), so the length of a record written into one fits the `u32`, as does the count.
    fn head(&mut self, kind: u32, length: usize) {
        self.u32(kind);
        self.u32(length as u32);
    }

    fn report(&mut self, report: &LinkStatsReport) {
        let LinkStatsReport {
            interval_ms,
            packets_received,
            packets_lost,
            underruns,
            receive_timeouts,
            jitter_ms,
            cushion_ms,
            backlog_ms,
            recovered,
        } = *report;
        self.head(REPORT, record_len(REPORT_FIELDS));
        self.word(presence([jitter_ms, cushion_ms, backlog_ms]));
        self.int(interval_ms);
        self.int(packets_received);
        self.int(packets_lost);
        self.int(underruns);
        self.int(receive_timeouts);
        self.optional_real(jitter_ms);
        self.optional_real(cushion_ms);
        self.optional_real(backlog_ms);
        self.int(recovered);
    }

    fn write(&mut self, samples: &[i16]) {
        let length = write_record_len(samples);
        self.head(WRITE, length);
        self.word(count(std::mem::size_of_val(samples)));
        let start = self.at;
        let end = start + length - record_len(1);
        let (pcm, padding) = self.out[start..end].split_at_mut(std::mem::size_of_val(samples));
        for (bytes, sample) in pcm.chunks_exact_mut(2).zip(samples) {
            bytes.copy_from_slice(&sample.to_le_bytes());
        }
        // The buffer is reused, so the padding would otherwise hold an older outcome's bytes.
        padding.fill(0);
        self.at = end;
    }

    fn stats(&mut self, stats: &ReceiverStats) {
        let ReceiverStats {
            packets,
            lost,
            sample_rate,
            packets_per_second,
            inter_arrival_ms,
            jitter_ms,
            discarded_samples,
            packets_with_discarded_samples,
            playback_backlog_ms,
            output_playing,
        } = *stats;
        self.head(STATS, record_len(STATS_FIELDS));
        self.word(presence([inter_arrival_ms, jitter_ms, playback_backlog_ms]));
        self.int(packets);
        self.int(lost);
        self.int(sample_rate);
        self.real(packets_per_second);
        self.optional_real(inter_arrival_ms);
        self.optional_real(jitter_ms);
        self.int(discarded_samples);
        self.int(packets_with_discarded_samples);
        self.optional_real(playback_backlog_ms);
        self.flag(output_playing);
    }
}

/// A presence word: bit `i` set when the `i`th value is present.
fn presence<const N: usize>(values: [Option<f64>; N]) -> u64 {
    values
        .iter()
        .enumerate()
        .filter(|(_, value)| value.is_some())
        .fold(0, |word, (bit, _)| word | 1 << bit)
}

#[cfg(test)]
mod tests {
    use super::*;

    // A wake-up in.

    #[test]
    fn a_wake_up_reads_every_field_from_its_word() {
        let mut words = [0; WAKE_WORDS];
        words[0] = -1; // the clock's bits: u64::MAX
        words[1] = 1;
        words[2..8].copy_from_slice(&[1, 11, 12, 1, 14, 1]);
        words[8] = 1;
        words[9] = 256;
        words[10] = 2_000_000;
        words[11] = 1;
        words[12..18].copy_from_slice(&[0, 0, 22, 0, 0, 0]);
        words[18..25].copy_from_slice(&[80, 90, 1, 1, 1, 40, 3]);

        assert_eq!(
            wake(&words, Some(vec![1, 2, 3])),
            Wake {
                started_ns: u64::MAX,
                output_at_start: Some(OutputReadings {
                    writable_frames: Some(11),
                    underrun_count: 12,
                    presented_frame: Some(14),
                    is_playing: true,
                }),
                applied_target_frames: Some(256),
                datagram: Some(vec![1, 2, 3]),
                received_ns: 2_000_000,
                output_at_receive: Some(OutputReadings {
                    writable_frames: None,
                    underrun_count: 22,
                    presented_frame: None,
                    is_playing: false,
                }),
                settings: PlayerSettings {
                    volume_percent: 80,
                    max_level_percent: 90,
                    muted: true,
                    server_owns_mix: true,
                    playback_buffer: PlaybackBufferSetting::Fixed { milliseconds: 40 },
                    redundancy_hold_packets: 3,
                },
            }
        );
    }

    #[test]
    fn an_absent_value_reads_as_none_whatever_its_word_holds() {
        let mut words = [7; WAKE_WORDS];
        words[1] = 0;
        words[8] = 0;
        words[11] = 0;
        words[22] = 0;

        let wake = wake(&words, None);
        assert_eq!(wake.output_at_start, None);
        assert_eq!(wake.applied_target_frames, None);
        assert_eq!(wake.datagram, None);
        assert_eq!(wake.output_at_receive, None);
        assert_eq!(wake.settings.playback_buffer, PlaybackBufferSetting::Auto);
    }

    #[test]
    fn a_fixed_buffer_beyond_what_the_core_holds_is_clamped() {
        for (milliseconds, expected) in [(70_000, u16::MAX), (-5, 0), (5, 5)] {
            let mut words = [0; WAKE_WORDS];
            words[22] = 1;
            words[23] = milliseconds;
            assert_eq!(
                wake(&words, None).settings.playback_buffer,
                PlaybackBufferSetting::Fixed {
                    milliseconds: expected
                }
            );
        }
    }

    #[test]
    fn readings_read_from_their_six_words() {
        assert_eq!(
            readings(&[1, -3, 4, 1, 5, 1]),
            OutputReadings {
                writable_frames: Some(-3),
                underrun_count: 4,
                presented_frame: Some(5),
                is_playing: true,
            }
        );
        assert_eq!(
            readings(&[0, 9, 0, 0, 9, 0]),
            OutputReadings {
                writable_frames: None,
                underrun_count: 0,
                presented_frame: None,
                is_playing: false,
            }
        );
    }

    // An outcome out.

    #[test]
    fn an_empty_outcome_is_its_header_alone() {
        let outcome = WakeOutcome {
            report_due_at_ms: -2,
            ..WakeOutcome::default()
        };
        let bytes = encoded(&outcome);
        assert_eq!(bytes.len(), 16);
        assert_eq!(u32_at(&bytes, 0), LAYOUT_VERSION);
        assert_eq!(u32_at(&bytes, 4), 0);
        assert_eq!(word_at(&bytes, 8), -2);
    }

    #[test]
    fn records_follow_in_the_order_the_platform_acts_on_them() {
        let outcome = WakeOutcome {
            report: Some(report()),
            actions: vec![
                OutputAction::SetTargetFrames { frames: 256 },
                OutputAction::Write {
                    samples: vec![1, 2],
                },
                OutputAction::Write { samples: vec![3] },
            ],
            stats: vec![stats(1), stats(2)],
            error: Some(ReceiveError::TooManyChannels { channels: 33 }),
            open_output: Some(OutputFormat {
                sample_rate: 48_000,
                packet_frames: 128,
            }),
            report_due_at_ms: 1_000,
        };
        let bytes = encoded(&outcome);

        let records = records(&bytes);
        let kinds: Vec<u32> = records.iter().map(|(kind, _)| *kind).collect();
        assert_eq!(
            kinds,
            [
                REPORT,
                SET_TARGET_FRAMES,
                WRITE,
                WRITE,
                STATS,
                STATS,
                ERROR,
                OPEN_OUTPUT
            ]
        );
        assert_eq!(u32_at(&bytes, 4), 8);
        assert_eq!(word_at(&bytes, 8), 1_000);
        assert_eq!(words(records[1].1), [256]);
        assert_eq!(words(records[4].1)[1], 1);
        assert_eq!(words(records[5].1)[1], 2);
        assert_eq!(words(records[7].1), [48_000, 128]);
    }

    #[test]
    fn a_write_carries_its_samples_as_little_endian_pcm16_padded_to_eight_bytes() {
        let outcome = WakeOutcome {
            actions: vec![OutputAction::Write {
                samples: vec![0x0102, -2, i16::MIN],
            }],
            ..WakeOutcome::default()
        };
        // Stale bytes where the padding goes: it is written as zeros.
        let mut out = vec![0xAA; 64];
        let written = encode(&outcome, &mut out).expect("fits");
        assert_eq!(written, 16 + 24);

        let record = &out[16..written];
        assert_eq!(u32_at(record, 0), WRITE);
        assert_eq!(u32_at(record, 4), 24);
        assert_eq!(word_at(record, 8), 6);
        assert_eq!(&record[16..], [0x02, 0x01, 0xFE, 0xFF, 0x00, 0x80, 0, 0]);
        // Nothing past the outcome is touched.
        assert!(out[written..].iter().all(|&byte| byte == 0xAA));
    }

    #[test]
    fn a_write_of_no_samples_is_its_byte_count_alone() {
        let outcome = WakeOutcome {
            actions: vec![OutputAction::Write { samples: vec![] }],
            ..WakeOutcome::default()
        };
        let bytes = encoded(&outcome);
        let records = records(&bytes);
        assert_eq!(records.len(), 1);
        assert_eq!(words(records[0].1), [0]);
    }

    #[test]
    fn a_report_carries_every_field_and_marks_what_was_not_measured() {
        let measured = WakeOutcome {
            report: Some(report()),
            ..WakeOutcome::default()
        };
        assert_eq!(
            words(records(&encoded(&measured))[0].1),
            [
                0b111,
                1_000,
                99,
                1,
                2,
                3,
                0.5f64.to_bits() as i64,
                (-0.0f64).to_bits() as i64,
                12.25f64.to_bits() as i64,
                4,
            ]
        );

        let unmeasured = WakeOutcome {
            report: Some(LinkStatsReport {
                jitter_ms: None,
                cushion_ms: Some(6.25),
                backlog_ms: None,
                ..report()
            }),
            ..WakeOutcome::default()
        };
        let words = words(records(&encoded(&unmeasured))[0].1);
        assert_eq!(words[0], 0b010);
        assert_eq!(words[6..9], [0, 6.25f64.to_bits() as i64, 0]);
    }

    #[test]
    fn stats_carry_every_field_and_mark_what_was_not_measured() {
        let measured = WakeOutcome {
            stats: vec![stats(7)],
            ..WakeOutcome::default()
        };
        let bytes = encoded(&measured);
        assert_eq!(u32_at(&bytes[16..], 4), 96);
        assert_eq!(
            words(records(&bytes)[0].1),
            [
                0b111,
                7,
                2,
                48_000,
                375.0f64.to_bits() as i64,
                2.67f64.to_bits() as i64,
                0.25f64.to_bits() as i64,
                64,
                1,
                5.0f64.to_bits() as i64,
                1,
            ]
        );

        let unmeasured = WakeOutcome {
            stats: vec![ReceiverStats {
                inter_arrival_ms: None,
                jitter_ms: None,
                playback_backlog_ms: Some(1.5),
                output_playing: false,
                ..stats(7)
            }],
            ..WakeOutcome::default()
        };
        let words = words(records(&encoded(&unmeasured))[0].1);
        assert_eq!(words[0], 0b100);
        assert_eq!((words[5], words[6]), (0, 0));
        assert_eq!(words[9], 1.5f64.to_bits() as i64);
        assert_eq!(words[10], 0);
    }

    #[test]
    fn every_receive_error_has_its_code_and_fields() {
        let undecodable = |reason| ReceiveError::Undecodable { reason };
        let cases = [
            (
                undecodable(DecodeError::HeaderTooShort { byte_count: 5 }),
                [1, 5, 0],
            ),
            (
                undecodable(DecodeError::UnknownMagic(*b"PMOX")),
                [2, i64::from(u32::from_le_bytes(*b"PMOX")), 0],
            ),
            (undecodable(DecodeError::UnsupportedVersion(2)), [3, 2, 0]),
            (undecodable(DecodeError::ZeroChannels), [4, 0, 0]),
            (
                undecodable(DecodeError::InvalidSampleRate(u32::MAX)),
                [5, i64::from(u32::MAX), 0],
            ),
            (
                undecodable(DecodeError::PayloadLengthMismatch {
                    declared_sample_count: 256,
                    payload_byte_count: 511,
                }),
                [6, 256, 511],
            ),
            (
                undecodable(DecodeError::SampleCountNotDivisibleByChannels {
                    sample_count: 3,
                    channels: 2,
                }),
                [7, 3, 2],
            ),
            (
                undecodable(DecodeError::SampleBufferTooSmall {
                    sample_count: 300,
                    capacity: 256,
                }),
                [8, 300, 256],
            ),
            (ReceiveError::TooManyChannels { channels: 33 }, [9, 33, 0]),
            (
                ReceiveError::SampleRateChanged {
                    from: 48_000,
                    to: -44_100,
                },
                [10, 48_000, -44_100],
            ),
        ];
        for (error, expected) in cases {
            let outcome = WakeOutcome {
                error: Some(error),
                ..WakeOutcome::default()
            };
            let bytes = encoded(&outcome);
            let records = records(&bytes);
            assert_eq!(records[0].0, ERROR, "{error:?}");
            assert_eq!(words(records[0].1), expected, "{error:?}");
        }
    }

    #[test]
    fn an_outcome_that_does_not_fit_asks_for_the_bytes_it_needs_and_writes_nothing() {
        let outcome = WakeOutcome {
            actions: vec![OutputAction::Write {
                samples: vec![7; 256],
            }],
            stats: vec![stats(1)],
            report_due_at_ms: 1_000,
            ..WakeOutcome::default()
        };
        let needed = 16 + (16 + 512) + 96;
        assert_eq!(encoded_len(&outcome), needed);

        let mut short = vec![0xAA; needed - 1];
        assert_eq!(encode(&outcome, &mut short), Err(needed));
        assert!(short.iter().all(|&byte| byte == 0xAA));

        let mut exact = vec![0; needed];
        assert_eq!(encode(&outcome, &mut exact), Ok(needed));
    }

    // Helpers.

    fn encoded(outcome: &WakeOutcome) -> Vec<u8> {
        let mut out = vec![0; encoded_len(outcome)];
        let written = encode(outcome, &mut out).expect("an outcome fits its own length");
        assert_eq!(written, out.len());
        out
    }

    /// Each record of an encoded outcome: its kind and its fields' bytes, checked against the
    /// header's count, the 8-byte boundaries and the outcome's end.
    fn records(bytes: &[u8]) -> Vec<(u32, &[u8])> {
        let mut records = Vec::new();
        let mut at = 16;
        while at < bytes.len() {
            let length = u32_at(bytes, at + 4) as usize;
            assert!(
                length >= 8 && length.is_multiple_of(8),
                "record length {length}"
            );
            records.push((u32_at(bytes, at), &bytes[at + 8..at + length]));
            at += length;
        }
        assert_eq!(at, bytes.len());
        assert_eq!(records.len(), u32_at(bytes, 4) as usize);
        records
    }

    fn words(fields: &[u8]) -> Vec<i64> {
        fields
            .chunks_exact(8)
            .map(|word| i64::from_le_bytes(word.try_into().unwrap()))
            .collect()
    }

    fn u32_at(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    fn word_at(bytes: &[u8], at: usize) -> i64 {
        i64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
    }

    fn report() -> LinkStatsReport {
        LinkStatsReport {
            interval_ms: 1_000,
            packets_received: 99,
            packets_lost: 1,
            underruns: 2,
            receive_timeouts: 3,
            jitter_ms: Some(0.5),
            cushion_ms: Some(-0.0),
            backlog_ms: Some(12.25),
            recovered: 4,
        }
    }

    fn stats(packets: i64) -> ReceiverStats {
        ReceiverStats {
            packets,
            lost: 2,
            sample_rate: 48_000,
            packets_per_second: 375.0,
            inter_arrival_ms: Some(2.67),
            jitter_ms: Some(0.25),
            discarded_samples: 64,
            packets_with_discarded_samples: 1,
            playback_backlog_ms: Some(5.0),
            output_playing: true,
        }
    }
}
