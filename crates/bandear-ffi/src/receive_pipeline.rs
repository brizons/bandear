//! The receive pipeline across the binding: `bandear-core`'s `ReceivePipeline` behind a lock, and
//! every type its calls take and return.
//!
//! The core's plain types cross as they are, declared here as UniFFI remote types: their fields
//! are already types UniFFI carries, so they cross with no copy and no conversion code, and the
//! compiler refuses a declaration here that stops matching the core's. Four types cross as mirrors
//! converted with `From` instead, because the binding must differ from the core:
//! - [`DecodeError`]: UniFFI cannot carry its `usize` and `[u8; 4]` fields, so they cross as `u64`
//!   and bytes.
//! - [`OutputAction`]: the samples to write cross as little-endian PCM16 bytes, which Swift reads as
//!   `Data` and Kotlin as a `ByteArray`; `Vec<i16>` would reach Kotlin as a boxed `List<Short>` on
//!   the receive thread.
//! - [`ReceiveError`] and [`WakeOutcome`], which hold them.
//!
//! Every conversion destructures or matches the core type completely, so a field or a case added
//! to the core fails to compile here until the binding carries it.

use bandear_core::cushion::PlaybackBufferSetting;
use bandear_core::link_stats::LinkStatsReport;
use bandear_core::packet;
use bandear_core::receive_pipeline::{
    self as pipeline, OutputFormat, OutputReadings, PlayerSettings, ReceiverStats, Wake,
};
use std::sync::{Mutex, MutexGuard, PoisonError};

/// The receive loop's logic in one call per wake-up: from "a datagram (or a timeout) arrived" to
/// "write these samples, apply this buffer target, send this link report".
///
/// The socket, the receive thread, the clock and the audio output stay on the platform, which
/// passes time in as nanoseconds of one monotonic clock and acts on each outcome in order: send
/// the report, act on the output, show the stats, log the error, then open the output when asked.
/// One pipeline per receive session, called from its receive thread.
#[derive(uniffi::Object)]
pub struct ReceivePipeline {
    /// The calls take `&self` across the binding. Only the receive thread calls, so the lock is
    /// never contended.
    inner: Mutex<pipeline::ReceivePipeline>,
}

#[uniffi::export]
impl ReceivePipeline {
    /// A pipeline for a receiver started at `started_ns` on the monotonic clock every later call
    /// reads.
    #[uniffi::constructor]
    pub fn new(started_ns: u64) -> Self {
        Self {
            inner: Mutex::new(pipeline::ReceivePipeline::new(started_ns)),
        }
    }

    /// The first instant, in milliseconds, at which a wake-up reads the output for a report;
    /// every outcome carries it after that.
    pub fn report_due_at_ms(&self) -> i64 {
        self.lock().report_due_at_ms()
    }

    /// One wake-up: the report due at its start, then the datagram it received (or its timeout),
    /// then every packet now due.
    pub fn on_wake(&self, wake: Wake) -> WakeOutcome {
        self.lock().on_wake(wake).into()
    }

    /// The output the last outcome asked for opened, holding `capacity_frames` and reading
    /// `output` now: the wake-up goes on with the packet that asked for it, then the others due at
    /// `now_ns`.
    pub fn on_output_opened(
        &self,
        now_ns: u64,
        capacity_frames: i64,
        output: OutputReadings,
    ) -> WakeOutcome {
        self.lock()
            .on_output_opened(now_ns, capacity_frames, output)
            .into()
    }

    /// The output the last outcome asked for did not open: the packet that asked for it is not
    /// played, the wake-up ends, and the next packet played asks again. The platform reports why.
    pub fn on_output_failed(&self) -> WakeOutcome {
        self.lock().on_output_failed().into()
    }
}

impl ReceivePipeline {
    /// The pipeline, even after a call panicked while holding it. The core does not panic on any
    /// input (`never_panics_on_what_the_network_sends` in `bandear-core`), and its state is whole
    /// between calls, so a poisoned lock goes on with that state rather than failing every later
    /// call on the receive thread.
    fn lock(&self) -> MutexGuard<'_, pipeline::ReceivePipeline> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// One wake-up of the receive loop: everything the pipeline needs for it.
#[uniffi::remote(Record)]
pub struct Wake {
    /// The clock when the wake-up started, before its receive, in nanoseconds: the link report is
    /// taken then.
    pub started_ns: u64,
    /// The output read at `started_ns`, when one is open and `started_ns` in milliseconds reached
    /// the last `report_due_at_ms`; `None` otherwise. A report due while an output is open waits
    /// for it.
    pub output_at_start: Option<OutputReadings>,
    /// What the output answered to the last `SetTargetFrames` of the previous outcome: the frames
    /// it applied, 0 when it could not say. `None` when none was asked.
    pub applied_target_frames: Option<i64>,
    /// The datagram the receive answered, or `None` when it timed out.
    pub datagram: Option<Vec<u8>>,
    /// The clock once the receive answered, in nanoseconds: arrivals and plays are timed by it.
    pub received_ns: u64,
    /// The output read once the receive answered, while one is open.
    pub output_at_receive: Option<OutputReadings>,
    /// The settings now, read together.
    pub settings: PlayerSettings,
}

/// What the platform's audio output says about itself, read in one go.
#[uniffi::remote(Record)]
pub struct OutputReadings {
    /// The frames the output takes now without blocking (the iOS ring's free space); `None` when a
    /// write takes every frame it is given (Android's blocking `AudioTrack.write`).
    pub writable_frames: Option<i64>,
    /// The times the output ran dry since it opened.
    pub underrun_count: i64,
    /// The frame reaching the listener now, counted from the first frame written; `None` until the
    /// output knows it.
    pub presented_frame: Option<i64>,
    /// Whether the output is reaching the listener (`false` while the system holds it stopped).
    pub is_playing: bool,
}

/// What the receive loop plays by that is set elsewhere, read once per wake-up.
#[uniffi::remote(Record)]
pub struct PlayerSettings {
    /// The phone's master volume, in percent; clamped to 0-100.
    pub volume_percent: i64,
    /// The highest level the phone plays, in percent of full scale; clamped to 0-100.
    pub max_level_percent: i64,
    pub muted: bool,
    /// True once the desk acknowledged a mix: it then applies the master controls itself, so the
    /// phone leaves the audio as it comes.
    pub server_owns_mix: bool,
    /// The desk's playback buffer, which the cushion follows.
    pub playback_buffer: PlaybackBufferSetting,
    /// The packets held back for the desk's delayed copies; clamped to the hold's range.
    pub redundancy_hold_packets: i64,
}

/// The desk's playback buffer setting.
#[uniffi::remote(Enum)]
pub enum PlaybackBufferSetting {
    /// The phone adapts the cushion on its own.
    Auto,
    /// The phone keeps this cushion and stops adapting it.
    Fixed {
        /// The length the desk pinned, in milliseconds.
        milliseconds: u16,
    },
}

/// The format of the output to open: the first packet's rate and size.
#[uniffi::remote(Record)]
pub struct OutputFormat {
    pub sample_rate: i64,
    /// Frames per packet, counted as stereo frames (the packet's samples over two).
    pub packet_frames: i64,
}

/// The receiver's own metrics after a packet played, all since the pipeline started. A value not
/// measured yet is `None`.
#[uniffi::remote(Record)]
pub struct ReceiverStats {
    /// Packets played.
    pub packets: i64,
    /// Packets the stream skipped, each counted once.
    pub lost: i64,
    /// The rate of the packet played.
    pub sample_rate: i64,
    /// Packets played per second since the pipeline started.
    pub packets_per_second: f64,
    /// The smoothed spacing between arrivals.
    pub inter_arrival_ms: Option<f64>,
    /// The smoothed jitter around that spacing.
    pub jitter_ms: Option<f64>,
    /// Samples the output did not take, so never played.
    pub discarded_samples: i64,
    /// Packets that lost any samples that way.
    pub packets_with_discarded_samples: i64,
    /// The audio queued ahead of the listener, smoothed by a tenth per packet.
    pub playback_backlog_ms: Option<f64>,
    /// Whether the output was reaching the listener as the packet was written.
    pub output_playing: bool,
}

/// The link report the phone sends the desk about once a second.
#[uniffi::remote(Record)]
pub struct LinkStatsReport {
    pub interval_ms: i64,
    pub packets_received: i64,
    pub packets_lost: i64,
    pub underruns: i64,
    pub receive_timeouts: i64,
    pub jitter_ms: Option<f64>,
    pub cushion_ms: Option<f64>,
    pub backlog_ms: Option<f64>,
    pub recovered: i64,
}

/// What the platform does after a call, in this order: send the report, act on the output, show
/// the stats, log the error, then open the output when asked.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct WakeOutcome {
    /// The link report due when the wake-up started, for the desk.
    pub report: Option<LinkStatsReport>,
    /// What to do to the output, in order.
    pub actions: Vec<OutputAction>,
    /// The metrics after each packet played, in order.
    pub stats: Vec<ReceiverStats>,
    /// Why the wake-up ended early, when it did. What was due waits for the next wake-up.
    pub error: Option<ReceiveError>,
    /// Open an output in this format, then answer with `on_output_opened` or `on_output_failed`:
    /// the wake-up is not over until then.
    pub open_output: Option<OutputFormat>,
    /// The first instant, in milliseconds of the same clock, at which the next wake-up reads the
    /// output for a report (`Wake.output_at_start`).
    pub report_due_at_ms: i64,
}

impl From<pipeline::WakeOutcome> for WakeOutcome {
    fn from(outcome: pipeline::WakeOutcome) -> Self {
        let pipeline::WakeOutcome {
            report,
            actions,
            stats,
            error,
            open_output,
            report_due_at_ms,
        } = outcome;
        Self {
            report,
            actions: actions.into_iter().map(OutputAction::from).collect(),
            stats,
            error: error.map(ReceiveError::from),
            open_output,
            report_due_at_ms,
        }
    }
}

/// One thing to do to the output, in the order the outcome lists them.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum OutputAction {
    /// Write these interleaved stereo PCM16 samples, two bytes each, little endian: the bytes as
    /// the output takes them. One per packet played; shorter than the packet when the output
    /// could not take all of it, which then counts as discarded.
    Write { pcm16_le_bytes: Vec<u8> },
    /// Ask the output to queue this many frames before it plays: the cushion. Its answer goes back
    /// in the next `Wake.applied_target_frames`.
    SetTargetFrames { frames: i64 },
}

impl From<pipeline::OutputAction> for OutputAction {
    fn from(action: pipeline::OutputAction) -> Self {
        match action {
            pipeline::OutputAction::Write { samples } => Self::Write {
                pcm16_le_bytes: pcm16_le_bytes(&samples),
            },
            pipeline::OutputAction::SetTargetFrames { frames } => Self::SetTargetFrames { frames },
        }
    }
}

/// `samples` as the bytes PCM16 little endian lays them out in.
fn pcm16_le_bytes(samples: &[i16]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(std::mem::size_of_val(samples));
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

/// Why a wake-up ended before it played everything due.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ReceiveError {
    /// The datagram is not a packet this client plays.
    Undecodable { reason: DecodeError },
    /// The packet announced more channels than the desk mixes (32).
    TooManyChannels { channels: i64 },
    /// The packet's rate is not the one the output opened at. The output is never reopened at the
    /// new rate: a desk does not change its rate mid-session.
    SampleRateChanged { from: i64, to: i64 },
}

impl From<pipeline::ReceiveError> for ReceiveError {
    fn from(error: pipeline::ReceiveError) -> Self {
        match error {
            pipeline::ReceiveError::Undecodable { reason } => Self::Undecodable {
                reason: reason.into(),
            },
            pipeline::ReceiveError::TooManyChannels { channels } => {
                Self::TooManyChannels { channels }
            }
            pipeline::ReceiveError::SampleRateChanged { from, to } => {
                Self::SampleRateChanged { from, to }
            }
        }
    }
}

/// Why a datagram was refused, naming what disagreed.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum DecodeError {
    /// Fewer bytes arrived than the 20-byte header needs.
    HeaderTooShort { byte_count: u64 },
    /// The first four bytes were not `PMON`; something else is on the port.
    UnknownMagic { magic: Vec<u8> },
    /// A protocol version this client was not written against.
    UnsupportedVersion { version: u8 },
    /// A channel count of zero, which would make the frame count meaningless.
    ZeroChannels,
    /// A sample rate of 0, or above `i32::MAX`.
    InvalidSampleRate { sample_rate: u32 },
    /// The payload is not exactly `declared_sample_count` PCM16 samples long.
    PayloadLengthMismatch {
        declared_sample_count: u64,
        payload_byte_count: u64,
    },
    /// The sample count leaves a frame half finished, so the stream would go out of phase.
    SampleCountNotDivisibleByChannels { sample_count: u64, channels: u8 },
    /// The caller's buffer holds fewer samples than the datagram declares.
    SampleBufferTooSmall { sample_count: u64, capacity: u64 },
}

impl From<packet::DecodeError> for DecodeError {
    fn from(error: packet::DecodeError) -> Self {
        match error {
            packet::DecodeError::HeaderTooShort { byte_count } => Self::HeaderTooShort {
                byte_count: count(byte_count),
            },
            packet::DecodeError::UnknownMagic(magic) => Self::UnknownMagic {
                magic: magic.to_vec(),
            },
            packet::DecodeError::UnsupportedVersion(version) => {
                Self::UnsupportedVersion { version }
            }
            packet::DecodeError::ZeroChannels => Self::ZeroChannels,
            packet::DecodeError::InvalidSampleRate(sample_rate) => {
                Self::InvalidSampleRate { sample_rate }
            }
            packet::DecodeError::PayloadLengthMismatch {
                declared_sample_count,
                payload_byte_count,
            } => Self::PayloadLengthMismatch {
                declared_sample_count: count(declared_sample_count),
                payload_byte_count: count(payload_byte_count),
            },
            packet::DecodeError::SampleCountNotDivisibleByChannels {
                sample_count,
                channels,
            } => Self::SampleCountNotDivisibleByChannels {
                sample_count: count(sample_count),
                channels,
            },
            packet::DecodeError::SampleBufferTooSmall {
                sample_count,
                capacity,
            } => Self::SampleBufferTooSmall {
                sample_count: count(sample_count),
                capacity: count(capacity),
            },
        }
    }
}

/// A `usize` count as `u64`, which holds every `usize` of the platforms the apps run on; one that
/// did not fit would read as `u64::MAX` rather than panic.
pub(crate) fn count(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bandear_core::packet::DecodeError as CoreDecodeError;
    use bandear_core::receive_pipeline::{
        OutputAction as CoreOutputAction, ReceiveError as CoreReceiveError,
        WakeOutcome as CoreWakeOutcome,
    };
    use std::sync::Arc;

    // The conversions.

    #[test]
    fn every_decode_error_crosses_with_its_fields() {
        let cases = [
            (
                CoreDecodeError::HeaderTooShort { byte_count: 5 },
                DecodeError::HeaderTooShort { byte_count: 5 },
            ),
            (
                CoreDecodeError::UnknownMagic(*b"PMOX"),
                DecodeError::UnknownMagic {
                    magic: b"PMOX".to_vec(),
                },
            ),
            (
                CoreDecodeError::UnsupportedVersion(2),
                DecodeError::UnsupportedVersion { version: 2 },
            ),
            (CoreDecodeError::ZeroChannels, DecodeError::ZeroChannels),
            (
                CoreDecodeError::InvalidSampleRate(u32::MAX),
                DecodeError::InvalidSampleRate {
                    sample_rate: u32::MAX,
                },
            ),
            (
                CoreDecodeError::PayloadLengthMismatch {
                    declared_sample_count: 256,
                    payload_byte_count: 511,
                },
                DecodeError::PayloadLengthMismatch {
                    declared_sample_count: 256,
                    payload_byte_count: 511,
                },
            ),
            (
                CoreDecodeError::SampleCountNotDivisibleByChannels {
                    sample_count: 255,
                    channels: 2,
                },
                DecodeError::SampleCountNotDivisibleByChannels {
                    sample_count: 255,
                    channels: 2,
                },
            ),
            (
                CoreDecodeError::SampleBufferTooSmall {
                    sample_count: 256,
                    capacity: usize::MAX,
                },
                DecodeError::SampleBufferTooSmall {
                    sample_count: 256,
                    capacity: u64::MAX,
                },
            ),
        ];

        for (core, expected) in cases {
            assert_eq!(DecodeError::from(core), expected, "{core:?}");
        }
    }

    #[test]
    fn every_receive_error_crosses_with_its_fields() {
        let cases = [
            (
                CoreReceiveError::Undecodable {
                    reason: CoreDecodeError::ZeroChannels,
                },
                ReceiveError::Undecodable {
                    reason: DecodeError::ZeroChannels,
                },
            ),
            (
                CoreReceiveError::TooManyChannels { channels: 33 },
                ReceiveError::TooManyChannels { channels: 33 },
            ),
            (
                CoreReceiveError::SampleRateChanged {
                    from: 48_000,
                    to: 44_100,
                },
                ReceiveError::SampleRateChanged {
                    from: 48_000,
                    to: 44_100,
                },
            ),
        ];

        for (core, expected) in cases {
            assert_eq!(ReceiveError::from(core), expected, "{core:?}");
        }
    }

    #[test]
    fn a_write_crosses_as_little_endian_pcm16_bytes() {
        let write = CoreOutputAction::Write {
            samples: vec![0x0102, -2, i16::MIN, i16::MAX, 0],
        };

        assert_eq!(
            OutputAction::from(write),
            OutputAction::Write {
                pcm16_le_bytes: vec![0x02, 0x01, 0xFE, 0xFF, 0x00, 0x80, 0xFF, 0x7F, 0x00, 0x00],
            }
        );
    }

    #[test]
    fn an_empty_write_crosses_as_no_bytes() {
        let write = CoreOutputAction::Write {
            samples: Vec::new(),
        };

        assert_eq!(
            OutputAction::from(write),
            OutputAction::Write {
                pcm16_le_bytes: Vec::new()
            }
        );
    }

    #[test]
    fn a_buffer_target_crosses_unchanged() {
        assert_eq!(
            OutputAction::from(CoreOutputAction::SetTargetFrames { frames: 960 }),
            OutputAction::SetTargetFrames { frames: 960 }
        );
    }

    #[test]
    fn an_outcome_crosses_with_every_field_in_order() {
        let core = CoreWakeOutcome {
            report: Some(report()),
            actions: vec![
                CoreOutputAction::SetTargetFrames { frames: 256 },
                CoreOutputAction::Write {
                    samples: vec![1, -1],
                },
                CoreOutputAction::Write { samples: vec![2] },
            ],
            stats: vec![stats(1), stats(2)],
            error: Some(CoreReceiveError::TooManyChannels { channels: 40 }),
            open_output: Some(OutputFormat {
                sample_rate: 48_000,
                packet_frames: 128,
            }),
            report_due_at_ms: 4_000,
        };

        assert_eq!(
            WakeOutcome::from(core),
            WakeOutcome {
                report: Some(report()),
                actions: vec![
                    OutputAction::SetTargetFrames { frames: 256 },
                    OutputAction::Write {
                        pcm16_le_bytes: vec![0x01, 0x00, 0xFF, 0xFF],
                    },
                    OutputAction::Write {
                        pcm16_le_bytes: vec![0x02, 0x00],
                    },
                ],
                stats: vec![stats(1), stats(2)],
                error: Some(ReceiveError::TooManyChannels { channels: 40 }),
                open_output: Some(OutputFormat {
                    sample_rate: 48_000,
                    packet_frames: 128,
                }),
                report_due_at_ms: 4_000,
            }
        );
    }

    #[test]
    fn an_empty_outcome_crosses_empty() {
        assert_eq!(
            WakeOutcome::from(CoreWakeOutcome::default()),
            WakeOutcome {
                report: None,
                actions: Vec::new(),
                stats: Vec::new(),
                error: None,
                open_output: None,
                report_due_at_ms: 0,
            }
        );
    }

    /// The core's own types cross as they are (UniFFI remote types): what the apps send is read
    /// back exactly as it was written.
    #[test]
    fn what_the_apps_send_reads_back_unchanged() {
        let wake = Wake {
            started_ns: u64::MAX,
            output_at_start: Some(readings()),
            applied_target_frames: Some(-1),
            datagram: Some(vec![0, 1, 2, 255]),
            received_ns: 7,
            output_at_receive: Some(OutputReadings {
                writable_frames: None,
                underrun_count: i64::MIN,
                presented_frame: None,
                is_playing: false,
            }),
            settings: PlayerSettings {
                volume_percent: 80,
                max_level_percent: 90,
                muted: true,
                server_owns_mix: true,
                playback_buffer: PlaybackBufferSetting::Fixed { milliseconds: 30 },
                redundancy_hold_packets: 2,
            },
        };
        assert_eq!(read_back(wake.clone()), wake);

        let timeout = Wake {
            datagram: None,
            output_at_start: None,
            ..wake
        };
        assert_eq!(read_back(timeout.clone()), timeout);

        assert_eq!(
            read_back(PlaybackBufferSetting::Auto),
            PlaybackBufferSetting::Auto
        );
        assert_eq!(read_back(readings()), readings());
    }

    #[test]
    fn what_the_apps_receive_reads_back_unchanged() {
        let outcome = WakeOutcome {
            report: Some(report()),
            actions: vec![
                OutputAction::SetTargetFrames { frames: 256 },
                OutputAction::Write {
                    pcm16_le_bytes: vec![1, 0, 255, 255],
                },
            ],
            stats: vec![stats(1)],
            error: Some(ReceiveError::Undecodable {
                reason: DecodeError::UnknownMagic {
                    magic: b"PMOX".to_vec(),
                },
            }),
            open_output: Some(OutputFormat {
                sample_rate: 44_100,
                packet_frames: 441,
            }),
            report_due_at_ms: i64::MAX,
        };

        assert_eq!(read_back(outcome.clone()), outcome);
    }

    // The object, as the receive loop drives it.

    #[test]
    fn a_packet_opens_the_output_then_plays_through_it() {
        let pipeline = ReceivePipeline::new(0);
        assert_eq!(pipeline.report_due_at_ms(), 3_000);

        let first = pipeline.on_wake(wake(Some(datagram(1, &samples(0))), 0, None));
        assert_eq!(
            first.open_output,
            Some(OutputFormat {
                sample_rate: 48_000,
                packet_frames: FRAMES as i64,
            })
        );
        assert!(first.actions.is_empty());

        let opened = pipeline.on_output_opened(0, 24_000, readings());
        assert_eq!(
            opened.actions,
            [
                OutputAction::SetTargetFrames {
                    frames: 2 * FRAMES as i64
                },
                OutputAction::Write {
                    pcm16_le_bytes: little_endian(&samples(0)),
                },
            ]
        );
        assert_eq!(opened.open_output, None);
        assert_eq!(opened.report_due_at_ms, 1_000);

        let second = pipeline.on_wake(wake(Some(datagram(2, &samples(1))), 10, None));
        assert_eq!(
            second.actions,
            [OutputAction::Write {
                pcm16_le_bytes: little_endian(&samples(1)),
            }]
        );
        assert_eq!(second.stats.len(), 1);
        assert_eq!(second.stats[0].packets, 2);
        assert_eq!(second.error, None);

        // Nothing came for a while, and the report fell due.
        let timeout = pipeline.on_wake(wake(None, 1_000, Some(readings())));
        assert!(timeout.actions.is_empty());
        assert!(timeout.stats.is_empty());
        assert_eq!(timeout.error, None);
        assert_eq!(timeout.open_output, None);
        let report = timeout.report.expect("the report due at 1 000 ms");
        assert_eq!(report.packets_received, 2);
        assert_eq!(report.packets_lost, 0);
        assert_eq!(timeout.report_due_at_ms, 2_000);
    }

    #[test]
    fn an_undecodable_datagram_is_a_receive_error() {
        let pipeline = ReceivePipeline::new(0);

        let mut magic = datagram(1, &samples(0));
        magic[3] = b'X';
        assert_eq!(
            pipeline.on_wake(wake(Some(magic), 0, None)).error,
            Some(ReceiveError::Undecodable {
                reason: DecodeError::UnknownMagic {
                    magic: b"PMOX".to_vec()
                },
            })
        );

        assert_eq!(
            pipeline.on_wake(wake(Some(vec![b'P'; 5]), 10, None)).error,
            Some(ReceiveError::Undecodable {
                reason: DecodeError::HeaderTooShort { byte_count: 5 },
            })
        );
    }

    #[test]
    fn a_failed_open_ends_the_wake_up_and_the_next_packet_asks_again() {
        let pipeline = ReceivePipeline::new(0);
        pipeline.on_wake(wake(Some(datagram(1, &samples(0))), 0, None));

        let failed = pipeline.on_output_failed();
        assert!(failed.actions.is_empty());
        assert_eq!(failed.open_output, None);

        let next = pipeline.on_wake(wake(Some(datagram(2, &samples(1))), 10, None));
        assert!(next.open_output.is_some());
    }

    /// A panic while the lock is held poisons it. The pipeline's state is still whole between
    /// calls, so the next call goes on with it rather than failing every call after.
    #[test]
    fn a_poisoned_lock_does_not_stop_the_pipeline() {
        let pipeline = Arc::new(ReceivePipeline::new(0));
        pipeline.on_wake(wake(Some(datagram(1, &samples(0))), 0, None));
        pipeline.on_output_opened(0, 24_000, readings());
        let poisoner = Arc::clone(&pipeline);
        let _ = std::thread::spawn(move || {
            let _guard = poisoner.inner.lock();
            panic!("a panic while the lock is held poisons it");
        })
        .join();
        assert!(pipeline.inner.is_poisoned());

        assert_eq!(pipeline.report_due_at_ms(), 1_000);
        let next = pipeline.on_wake(wake(Some(datagram(2, &samples(1))), 10, None));
        assert_eq!(
            next.actions,
            [OutputAction::Write {
                pcm16_le_bytes: little_endian(&samples(1)),
            }]
        );
    }

    /// The binding adds no arithmetic of its own, so the extremes of every number the apps pass
    /// reach the core, which brings them into range.
    #[test]
    fn the_extremes_of_every_input_do_not_panic() {
        for (time_ns, reading) in [(0, i64::MIN), (u64::MAX, i64::MAX), (u64::MAX / 2, -1)] {
            let pipeline = ReceivePipeline::new(time_ns);
            let readings = OutputReadings {
                writable_frames: Some(reading),
                underrun_count: reading,
                presented_frame: Some(reading),
                is_playing: true,
            };
            let settings = PlayerSettings {
                volume_percent: reading,
                max_level_percent: reading,
                muted: false,
                server_owns_mix: false,
                playback_buffer: PlaybackBufferSetting::Fixed {
                    milliseconds: u16::MAX,
                },
                redundancy_hold_packets: reading,
            };
            for sequence in [0, 1, u64::MAX, 2] {
                let outcome = pipeline.on_wake(Wake {
                    started_ns: time_ns,
                    output_at_start: Some(readings),
                    applied_target_frames: Some(reading),
                    datagram: Some(datagram(sequence, &samples(3))),
                    received_ns: time_ns,
                    output_at_receive: Some(readings),
                    settings,
                });
                if outcome.open_output.is_some() {
                    pipeline.on_output_opened(time_ns, reading, readings);
                }
                pipeline.on_wake(Wake {
                    datagram: None,
                    started_ns: time_ns,
                    output_at_start: Some(readings),
                    applied_target_frames: Some(reading),
                    received_ns: time_ns,
                    output_at_receive: Some(readings),
                    settings,
                });
            }
            pipeline.on_output_failed();
            pipeline.report_due_at_ms();
        }
    }

    // Helpers.

    /// Stereo frames per packet: the S8 packet, 128 frames at 48 kHz.
    const FRAMES: usize = 128;

    /// One PMON datagram as the desk sends it (`crates/bandear-core/src/packet.rs`): stereo at
    /// 48 kHz.
    fn datagram(sequence: u64, samples: &[i16]) -> Vec<u8> {
        let mut bytes = b"PMON".to_vec();
        bytes.push(1);
        bytes.push(2);
        bytes.extend_from_slice(&48_000u32.to_le_bytes());
        bytes.extend_from_slice(&sequence.to_le_bytes());
        bytes.extend_from_slice(&u16::try_from(samples.len()).unwrap().to_le_bytes());
        bytes.extend_from_slice(&little_endian(samples));
        bytes
    }

    /// A packet's interleaved samples: both extremes, a sign and a byte order to read, then a ramp
    /// that differs per `seed`.
    fn samples(seed: i16) -> Vec<i16> {
        let mut samples = vec![0x0102, -2, i16::MIN, i16::MAX];
        samples.extend((4..FRAMES as i16 * 2).map(|i| i.wrapping_mul(97).wrapping_add(seed)));
        samples
    }

    fn little_endian(samples: &[i16]) -> Vec<u8> {
        samples
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect()
    }

    /// A wake-up at `milliseconds` that plays the packet as sent (the desk owns the mix), with
    /// fresh readings after the receive.
    fn wake(
        datagram: Option<Vec<u8>>,
        milliseconds: u64,
        at_start: Option<OutputReadings>,
    ) -> Wake {
        Wake {
            started_ns: milliseconds * 1_000_000,
            output_at_start: at_start,
            applied_target_frames: None,
            datagram,
            received_ns: milliseconds * 1_000_000,
            output_at_receive: Some(readings()),
            settings: PlayerSettings {
                server_owns_mix: true,
                ..PlayerSettings::default()
            },
        }
    }

    /// An output that takes everything, has presented nothing, never ran dry, and plays.
    fn readings() -> OutputReadings {
        OutputReadings {
            writable_frames: None,
            underrun_count: 0,
            presented_frame: None,
            is_playing: true,
        }
    }

    fn report() -> LinkStatsReport {
        LinkStatsReport {
            interval_ms: 1_000,
            packets_received: 99,
            packets_lost: 1,
            underruns: 2,
            receive_timeouts: 3,
            jitter_ms: Some(0.5),
            cushion_ms: None,
            backlog_ms: Some(12.25),
            recovered: 4,
        }
    }

    fn stats(packets: i64) -> ReceiverStats {
        ReceiverStats {
            packets,
            lost: 0,
            sample_rate: 48_000,
            packets_per_second: 375.0,
            inter_arrival_ms: Some(2.67),
            jitter_ms: None,
            discarded_samples: 0,
            packets_with_discarded_samples: 0,
            playback_backlog_ms: Some(5.0),
            output_playing: true,
        }
    }

    /// `value` lowered into a buffer the way the bindings receive it, then lifted back from it the
    /// way the bindings send it.
    fn read_back<T>(value: T) -> T
    where
        T: uniffi::Lower<crate::UniFfiTag> + uniffi::Lift<crate::UniFfiTag>,
    {
        let mut buffer = Vec::new();
        <T as uniffi::Lower<crate::UniFfiTag>>::write(value, &mut buffer);
        let mut unread = buffer.as_slice();
        let value = <T as uniffi::Lift<crate::UniFfiTag>>::try_read(&mut unread)
            .expect("what was written reads back");
        assert!(unread.is_empty(), "{} bytes left unread", unread.len());
        value
    }
}
