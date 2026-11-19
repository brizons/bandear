//! The receive loop's logic in one call per wake-up: from "a datagram (or a timeout) arrived" to
//! "write these samples, apply this buffer target, send this link report".
//!
//! A port of the iOS client's `ReceiveLoop.swift`, itself a port of the Android client's
//! `PmonPlayer.receiveLoop` (`PmonPlayer.kt:124-357`). The socket, the receive thread, the clock
//! and the audio output stay on each platform; this type holds every rule between them and reuses
//! the crate's units for each one: [`AudioDatagram`] reads the packet, [`HoldPlayout`] orders and
//! reports it, [`ArrivalTiming`] times it, [`pcm16`] protects it, [`CushionController`] sizes the
//! cushion, and [`playback_latency`] measures it.
//!
//! The binding is crossed once per wake-up, not once per unit, so the platform hands over
//! everything a wake-up needs in one [`Wake`] and acts on the [`WakeOutcome`] it gets back. Time is
//! passed in as nanoseconds of a monotonic clock, and what the output would have answered in the
//! middle of the loop is passed in as [`OutputReadings`], read where the loop would have read it:
//!
//! ```text
//! pipeline = ReceivePipeline::new(clock_ns)
//! due_at = pipeline.report_due_at_ms()
//! applied = None
//! while running:
//!     started = clock_ns
//!     at_start = (output open && started / 1_000_000 >= due_at) ? read(output) : None
//!     datagram = socket.receive()                       // None on a timeout
//!     if !running: break                                // what woke a stopped receive is not audio
//!     outcome = pipeline.on_wake(Wake { started_ns: started, output_at_start: at_start,
//!                                       applied_target_frames: take(applied), datagram,
//!                                       received_ns: clock_ns,
//!                                       output_at_receive: output.map(read), settings })
//!     loop:
//!         send(outcome.report); show(outcome.stats); log(outcome.error)
//!         for action in outcome.actions:                // in order
//!             Write(samples)       -> output.write(samples)
//!             SetTargetFrames(n)   -> applied = output.set_target(n) ?? 0
//!         due_at = outcome.report_due_at_ms
//!         match outcome.open_output:
//!             None         -> break
//!             Some(format) -> outcome = open(format) succeeded
//!                                 ? pipeline.on_output_opened(clock_ns, capacity, read(output))
//!                                 : pipeline.on_output_failed()   // the platform logs why
//! ```
//!
//! What moved with the boundary, compared with the iOS loop that asks its output as it goes:
//! - The output opens on the first packet played: the outcome asks for it, and the wake-up
//!   continues in [`ReceivePipeline::on_output_opened`] (or ends in
//!   [`ReceivePipeline::on_output_failed`]). The only wake-up that crosses the binding twice.
//! - The output is read once after the receive, not around each write: what it can take, its
//!   underruns, the frame it presents and whether it plays. A write then takes what the output
//!   said it could ([`OutputReadings::writable_frames`]), which is what the iOS ring takes, so the
//!   samples handed back are exactly the ones that count as written. The underruns are read on
//!   every wake-up for that, where iOS reads them only for a report or the cushion's second; the
//!   reading for a report alone is still taken only once one is due.
//! - A wake-up the platform ends because the session stopped never reaches the pipeline, so a
//!   report due at its start is not taken; iOS takes it before the receive.
//! - The frames a buffer target applied come back with the next wake-up
//!   ([`Wake::applied_target_frames`]), before the only rule that uses them: the next report.
//! - The clock is read once after the receive for every packet of the wake-up, where iOS reads it
//!   again for each packet played.
//!
//! Platform readings that feed arithmetic are brought into range first (a negative frame position
//! or underrun count reads as 0), so no reading can make the receive thread panic; decoding refuses
//! what no desk sends, and every count saturates.

use crate::arrival_timing::ArrivalTiming;
use crate::cushion::{CushionController, PlaybackBufferSetting};
use crate::hold_playout::HoldPlayout;
use crate::link_stats::{LinkStatsAccumulator, LinkStatsReport};
use crate::packet::{AudioDatagram, AudioDatagramHeader, DecodeError};
use crate::pcm16;
use crate::playback_latency;
use crate::reorder_hold::ReorderHold;
use std::fmt;

/// The most channels a packet may announce: Android's `MAX_CHANNELS` (`ClientMixState.kt:143`),
/// the source channels a mix can hold.
pub const MAXIMUM_CHANNELS: i64 = 32;

/// The output plays stereo: as on Android, a frame is two samples whatever the packet says.
const OUTPUT_CHANNELS: usize = 2;

/// The weight each new backlog reading gets in the smoothed one.
const BACKLOG_SMOOTHING: f64 = 0.1;

/// What the loop plays by that is set elsewhere: the musician's master controls, whether the desk
/// owns the mix, and the desk's playback and redundancy settings. Read once per wake-up, so every
/// packet of a wake-up plays by values set together.
///
/// The defaults are Android's `ClientMixState`: full volume, no cap, not muted, Auto, no hold, and
/// a desk that does not own the mix yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerSettings {
    /// The phone's master volume, in percent; clamped to 0-100.
    pub volume_percent: i64,
    /// The highest level the phone plays, in percent of full scale; clamped to 0-100.
    pub max_level_percent: i64,
    pub muted: bool,
    /// True once the desk acknowledged a mix: it then applies the master controls itself, so the
    /// phone leaves the audio as it comes ([`pcm16::apply_local_protection`]).
    pub server_owns_mix: bool,
    /// The desk's playback buffer, which the cushion follows.
    pub playback_buffer: PlaybackBufferSetting,
    /// The packets held back for the desk's delayed copies; clamped to the hold's range.
    pub redundancy_hold_packets: i64,
}

impl Default for PlayerSettings {
    fn default() -> Self {
        Self {
            volume_percent: 100,
            max_level_percent: 100,
            muted: false,
            server_owns_mix: false,
            playback_buffer: PlaybackBufferSetting::Auto,
            redundancy_hold_packets: 0,
        }
    }
}

/// What the platform's audio output says about itself, read in one go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

/// One wake-up of the platform's receive loop: everything the pipeline needs for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wake {
    /// The clock when the wake-up started, before its receive, in nanoseconds: the link report is
    /// taken then.
    pub started_ns: u64,
    /// The output read at `started_ns`, when one is open and `started_ns` in milliseconds reached
    /// the last [`WakeOutcome::report_due_at_ms`]; `None` otherwise. A report due while an output
    /// is open waits for it.
    pub output_at_start: Option<OutputReadings>,
    /// What the output answered to the last [`OutputAction::SetTargetFrames`] of the previous
    /// outcome: the frames it applied, 0 when it could not say. `None` when none was asked.
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

/// The format of the output to open: the first packet's rate and size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutputFormat {
    pub sample_rate: i64,
    /// Frames per packet, counted as stereo frames (the packet's samples over two).
    pub packet_frames: i64,
}

/// One thing to do to the output, in the order the outcome lists them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutputAction {
    /// Write these interleaved stereo PCM16 samples. One per packet played; shorter than the packet
    /// when the output could not take all of it, which then counts as discarded.
    Write { samples: Vec<i16> },
    /// Ask the output to queue this many frames before it plays: the cushion. Its answer goes back
    /// in the next [`Wake::applied_target_frames`].
    SetTargetFrames { frames: i64 },
}

/// The receiver's own metrics after a packet played, all since the pipeline started: Android's
/// nine `onStats` values and whether the output was playing.
///
/// A value not measured yet is `None`, where Android shows 0: the spacing and the jitter until a
/// second packet arrived, the backlog until the output said what it presented.
#[derive(Clone, Copy, Debug, PartialEq)]
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

/// Why a wake-up ended before it played everything due. What was due waits for the next wake-up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReceiveError {
    /// The datagram is not a packet this client plays.
    Undecodable { reason: DecodeError },
    /// The packet announced more channels than the desk mixes ([`MAXIMUM_CHANNELS`]).
    TooManyChannels { channels: i64 },
    /// The packet's rate is not the one the output opened at. The output is never reopened at the
    /// new rate: a desk does not change its rate mid-session.
    SampleRateChanged { from: i64, to: i64 },
}

impl fmt::Display for ReceiveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Undecodable { reason } => reason.fmt(formatter),
            Self::TooManyChannels { channels } => write!(
                formatter,
                "PMON packet announces {channels} channels, the limit is {MAXIMUM_CHANNELS}"
            ),
            Self::SampleRateChanged { from, to } => {
                write!(formatter, "sample rate changed from {from} to {to}")
            }
        }
    }
}

impl std::error::Error for ReceiveError {}

/// What the platform does after a call, in this order: send the report, act on the output, show
/// the stats, log the error, then open the output when asked.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WakeOutcome {
    /// The link report due when the wake-up started, for the desk.
    pub report: Option<LinkStatsReport>,
    /// What to do to the output, in order.
    pub actions: Vec<OutputAction>,
    /// The metrics after each packet played, in order.
    pub stats: Vec<ReceiverStats>,
    /// Why the wake-up ended early, when it did.
    pub error: Option<ReceiveError>,
    /// Open an output in this format, then answer with [`ReceivePipeline::on_output_opened`] or
    /// [`ReceivePipeline::on_output_failed`]: the wake-up is not over until then.
    pub open_output: Option<OutputFormat>,
    /// The first instant, in milliseconds of the same clock, at which the next wake-up reads the
    /// output for a report ([`Wake::output_at_start`]).
    pub report_due_at_ms: i64,
}

/// The receive loop's state for one launch: single-threaded, on the receive thread.
///
/// Every input is plain data and every answer is plain data; it reads no clock, starts no thread
/// and calls nothing back.
pub struct ReceivePipeline {
    playout: HoldPlayout<HeldPacket>,
    cushion: CushionController,
    arrival: ArrivalTiming,
    started_ns: u64,

    /// Whether the platform opened an output ([`ReceivePipeline::on_output_opened`]).
    output_open: bool,
    /// The packet that asked for the output, with the settings of its wake-up, until the platform
    /// answers.
    pending_open: Option<PendingOpen>,
    /// The rate the output opened at (set as it is asked for, as iOS sets it before opening).
    source_sample_rate: Option<i64>,
    playback_rate: Option<i64>,
    packet_frames: i64,
    capacity_frames: i64,
    /// The target the output applied, as it answered (0 when it could not say).
    applied_target_frames: i64,
    frames_written: i64,

    packets_played: i64,
    last_sample_rate: i64,
    packets_per_second: f64,
    discarded_samples: i64,
    packets_with_discarded_samples: i64,
    backlog_ms: Option<f64>,
}

/// A packet the hold keeps until it plays: its rate and its samples, decoded once.
struct HeldPacket {
    sample_rate: u32,
    samples: Vec<i16>,
}

/// The wake-up that stopped to open the output.
struct PendingOpen {
    packet: HeldPacket,
    settings: PlayerSettings,
}

/// Why a packet did not play through.
enum Interruption {
    /// It was refused; the wake-up ends with this error.
    Refused(ReceiveError),
    /// It asked for the output; the wake-up waits for the platform's answer.
    Opening,
}

impl ReceivePipeline {
    /// A pipeline for a receiver started at `started_ns` on the monotonic clock every later call
    /// reads.
    pub fn new(started_ns: u64) -> Self {
        Self {
            playout: HoldPlayout::new(
                ReorderHold::new(),
                LinkStatsAccumulator::new(milliseconds(started_ns)),
            ),
            cushion: CushionController::default(),
            arrival: ArrivalTiming::new(),
            started_ns,
            output_open: false,
            pending_open: None,
            source_sample_rate: None,
            playback_rate: None,
            packet_frames: 0,
            capacity_frames: 0,
            applied_target_frames: 0,
            frames_written: 0,
            packets_played: 0,
            last_sample_rate: 0,
            packets_per_second: 0.0,
            discarded_samples: 0,
            packets_with_discarded_samples: 0,
            backlog_ms: None,
        }
    }

    /// The first instant, in milliseconds, at which a wake-up reads the output for a report;
    /// every outcome carries it after that.
    pub fn report_due_at_ms(&self) -> i64 {
        self.playout.link_stats().due_at_ms()
    }

    /// One wake-up: the report due at its start, then the datagram it received (or its timeout),
    /// then every packet now due.
    pub fn on_wake(&mut self, wake: Wake) -> WakeOutcome {
        // An open the platform never answered: as if it failed.
        self.pending_open = None;
        if let Some(applied) = wake.applied_target_frames {
            self.applied_target_frames = applied;
        }
        let mut outcome = WakeOutcome {
            report: self.report_if_due(wake.started_ns, wake.output_at_start),
            ..WakeOutcome::default()
        };
        if wake.datagram.is_none() {
            // Nothing came for a while: what is held is not worth waiting for any longer.
            self.playout.on_timeout();
        }
        let mut output = OutputView::new(wake.output_at_receive.filter(|_| self.output_open));
        self.offer_and_play(
            wake.datagram.as_deref(),
            wake.received_ns,
            &wake.settings,
            &mut output,
            &mut outcome,
        );
        self.finish(outcome)
    }

    /// The output asked for opened, holding `capacity_frames` and reading `output` now: the
    /// wake-up goes on with the packet that asked for it, then the others due at `now_ns`.
    pub fn on_output_opened(
        &mut self,
        now_ns: u64,
        capacity_frames: i64,
        output: OutputReadings,
    ) -> WakeOutcome {
        let mut outcome = WakeOutcome::default();
        let Some(PendingOpen { packet, settings }) = self.pending_open.take() else {
            return self.finish(outcome);
        };
        self.output_open = true;
        self.capacity_frames = capacity_frames.max(0);
        let rate = i64::from(packet.sample_rate);
        // A buffer the desk set before the audio started applies from the first packet, and the
        // output primes on the cushion before the first frame reaches it.
        let _ = self.cushion.follow(
            settings.playback_buffer,
            self.packet_frames,
            rate,
            self.capacity_frames,
        );
        outcome.actions.push(self.cushion_target());
        let mut output = OutputView::new(Some(output));
        let frames = self.packet_frames;
        self.write(packet, frames, rate, &settings, &mut output, &mut outcome);
        self.drain(now_ns, &settings, &mut output, &mut outcome);
        self.finish(outcome)
    }

    /// The output asked for did not open: the packet that asked for it is not played, the wake-up
    /// ends, and the next packet played asks again. The platform reports why.
    pub fn on_output_failed(&mut self) -> WakeOutcome {
        self.pending_open = None;
        self.finish(WakeOutcome::default())
    }

    fn finish(&self, mut outcome: WakeOutcome) -> WakeOutcome {
        outcome.report_due_at_ms = self.report_due_at_ms();
        outcome
    }

    /// The link report rides on the wake-ups, so it is at most one receive timeout late, and the
    /// output is read for it only once one is due. One due while an output is open but was not
    /// read waits for the next wake-up, rather than reporting no underruns.
    fn report_if_due(
        &mut self,
        started_ns: u64,
        output_at_start: Option<OutputReadings>,
    ) -> Option<LinkStatsReport> {
        let now_ms = milliseconds(started_ns);
        if !self.playout.link_stats().is_due(now_ms) {
            return None;
        }
        let (underruns, presented_frame) = if self.output_open {
            let readings = output_at_start?.in_range();
            (readings.underrun_count, readings.presented_frame)
        } else {
            (0, None)
        };
        let extras = playback_latency::report_extras(
            self.packet_frames,
            self.cushion.cushion_packets(),
            self.playback_rate,
            self.frames_written,
            presented_frame,
            self.applied_target_frames,
            self.playout.hold_packets(),
        );
        self.playout.poll(
            now_ms,
            underruns,
            extras.cushion_milliseconds,
            extras.backlog_milliseconds,
        )
    }

    fn offer_and_play(
        &mut self,
        datagram: Option<&[u8]>,
        received_ns: u64,
        settings: &PlayerSettings,
        output: &mut OutputView,
        outcome: &mut WakeOutcome,
    ) {
        // The desk's hold, read once per wake-up, so a change applies from the next packet.
        self.playout.wake(settings.redundancy_hold_packets);
        if let Some(bytes) = datagram {
            let (sequence, packet) = match decode(bytes) {
                Ok(decoded) => decoded,
                Err(error) => {
                    outcome.error = Some(error);
                    return;
                }
            };
            // A duplicate, or a packet that arrived after a newer one was played or given up on:
            // playing it now would be out of order, and its gap is already counted. As Android's
            // `continue` does, this ends the wake-up: what is due plays on the next one.
            if !self.playout.offer(sequence, packet) {
                return;
            }
            // The arrival's spacing, as it arrived, whatever the hold does with it.
            if let Some(deviation) = self.arrival.on_arrival(received_ns) {
                self.playout.on_arrival_deviation(deviation);
            }
        }
        self.drain(received_ns, settings, output, outcome);
    }

    /// Every packet now due, in sequence order: at once with no hold, else once the hold has
    /// passed or a late copy filled its gap. A refused packet leaves what is due for the next
    /// wake-up.
    fn drain(
        &mut self,
        now_ns: u64,
        settings: &PlayerSettings,
        output: &mut OutputView,
        outcome: &mut WakeOutcome,
    ) {
        while let Some(packet) = self.playout.next_to_play(milliseconds(now_ns)) {
            match self.play(packet, now_ns, settings, output, outcome) {
                Ok(()) => {}
                Err(Interruption::Refused(error)) => {
                    outcome.error = Some(error);
                    return;
                }
                Err(Interruption::Opening) => return,
            }
        }
    }

    fn play(
        &mut self,
        packet: HeldPacket,
        played_ns: u64,
        settings: &PlayerSettings,
        output: &mut OutputView,
        outcome: &mut WakeOutcome,
    ) -> Result<(), Interruption> {
        self.packets_played = self.packets_played.saturating_add(1);
        let rate = i64::from(packet.sample_rate);
        self.last_sample_rate = rate;
        // The bits of the wrapping difference read as signed, as iOS reads them.
        let elapsed_seconds = played_ns.wrapping_sub(self.started_ns) as i64 as f64 / 1e9;
        self.packets_per_second = if elapsed_seconds > 0.0 {
            self.packets_played as f64 / elapsed_seconds
        } else {
            0.0
        };
        let frames = (packet.samples.len() / OUTPUT_CHANNELS) as i64;

        // The output opens once, at the first packet's rate and size; a packet at any other rate
        // is an error, every one of them, and the output is never reopened at the new rate.
        if self.output_open {
            if self.source_sample_rate != Some(rate) {
                return Err(Interruption::Refused(ReceiveError::SampleRateChanged {
                    from: self.source_sample_rate.unwrap_or(0),
                    to: rate,
                }));
            }
        } else {
            self.source_sample_rate = Some(rate);
            self.playback_rate = Some(rate);
            self.packet_frames = frames;
            outcome.open_output = Some(OutputFormat {
                sample_rate: rate,
                packet_frames: frames,
            });
            self.pending_open = Some(PendingOpen {
                packet,
                settings: *settings,
            });
            return Err(Interruption::Opening);
        }

        self.write(packet, frames, rate, settings, output, outcome);
        Ok(())
    }

    /// Writes one packet through the open output, measures it, reports it and moves the cushion.
    fn write(
        &mut self,
        packet: HeldPacket,
        frames: i64,
        rate: i64,
        settings: &PlayerSettings,
        output: &mut OutputView,
        outcome: &mut WakeOutcome,
    ) {
        let HeldPacket { mut samples, .. } = packet;
        let count = samples.len();
        // A short write is audio lost instead of played. Only what the output takes counts as
        // written: counting the rest would understate how far the writer ran ahead of the
        // listener. What it does not take is not handed over, so what it is handed is what counts.
        let taken_frames = output.take(frames);
        let taken_samples = taken_frames as usize * OUTPUT_CHANNELS;
        samples.truncate(taken_samples);
        pcm16::apply_local_protection(
            &mut samples,
            percent(settings.volume_percent),
            percent(settings.max_level_percent),
            settings.muted,
            settings.server_owns_mix,
        );
        outcome.actions.push(OutputAction::Write { samples });
        if taken_samples < count {
            self.discarded_samples = self
                .discarded_samples
                .saturating_add((count - taken_samples) as i64);
            self.packets_with_discarded_samples =
                self.packets_with_discarded_samples.saturating_add(1);
        }
        self.frames_written = self.frames_written.saturating_add(taken_frames);

        if let Some(presented) = output.presented_frame() {
            let instant = playback_latency::backlog_milliseconds(
                self.frames_written,
                presented,
                self.playback_rate.unwrap_or(rate),
            );
            self.backlog_ms = Some(self.backlog_ms.map_or(instant, |backlog| {
                backlog + BACKLOG_SMOOTHING * (instant - backlog)
            }));
        }

        outcome.stats.push(ReceiverStats {
            packets: self.packets_played,
            lost: self.playout.lost(),
            sample_rate: self.last_sample_rate,
            packets_per_second: self.packets_per_second,
            inter_arrival_ms: self.arrival.inter_arrival_ms(),
            jitter_ms: self.arrival.jitter_ms(),
            discarded_samples: self.discarded_samples,
            packets_with_discarded_samples: self.packets_with_discarded_samples,
            playback_backlog_ms: self.backlog_ms,
            output_playing: output.is_playing(),
        });

        // The desk's buffer applies at once; once per second of audio the output's underruns move
        // an Auto cushion; a fixed one never moves.
        let step = self.cushion.on_packet(
            settings.playback_buffer,
            self.packet_frames,
            frames,
            self.playback_rate,
            rate,
            self.capacity_frames,
        );
        let mut resize = step.apply;
        if step.adjust {
            if let Some(underruns) = output.underrun_count() {
                if self.cushion.adjust(underruns) {
                    resize = true;
                }
            }
        }
        if resize {
            outcome.actions.push(self.cushion_target());
        }
    }

    /// The cushion, asked of the output before it plays.
    fn cushion_target(&self) -> OutputAction {
        OutputAction::SetTargetFrames {
            frames: playback_latency::target_buffer_frames(
                self.packet_frames,
                self.cushion.cushion_packets(),
            ),
        }
    }
}

/// Decodes a datagram into the packet the hold keeps, with its sequence: refused when it is not
/// PMON or announces more channels than the desk mixes.
fn decode(bytes: &[u8]) -> Result<(u64, HeldPacket), ReceiveError> {
    // Exactly the declared samples when the datagram is whole (its length is checked against the
    // header), and never more than the datagram holds when it is not.
    let mut samples = vec![0; bytes.len().saturating_sub(AudioDatagramHeader::BYTE_COUNT) / 2];
    let header = AudioDatagram::decode_into(bytes, &mut samples)
        .map_err(|reason| ReceiveError::Undecodable { reason })?;
    let channels = i64::from(header.channels());
    if channels > MAXIMUM_CHANNELS {
        return Err(ReceiveError::TooManyChannels { channels });
    }
    samples.truncate(header.sample_count());
    Ok((
        header.sequence(),
        HeldPacket {
            sample_rate: header.sample_rate(),
            samples,
        },
    ))
}

/// A percentage for local protection, which clamps it to 0-100 anyway.
fn percent(value: i64) -> i32 {
    value.clamp(0, 100) as i32
}

/// `nanoseconds` in whole milliseconds, as iOS's `nowMilliseconds`.
fn milliseconds(nanoseconds: u64) -> i64 {
    (nanoseconds / 1_000_000) as i64
}

impl OutputReadings {
    /// The readings with what no output can report brought into range: no count or frame below
    /// zero.
    fn in_range(self) -> Self {
        Self {
            writable_frames: self.writable_frames.map(|frames| frames.max(0)),
            underrun_count: self.underrun_count.max(0),
            presented_frame: self.presented_frame.map(|frame| frame.max(0)),
            is_playing: self.is_playing,
        }
    }
}

/// The output as one wake-up sees it: its readings after the receive, and what it can still take
/// as the wake-up's packets are written. Without readings (no output, or one the platform did not
/// read) a write takes everything, nothing is presented, it plays, and the cushion is not adjusted.
struct OutputView {
    readings: Option<OutputReadings>,
    writable_frames: Option<i64>,
}

impl OutputView {
    fn new(readings: Option<OutputReadings>) -> Self {
        let readings = readings.map(OutputReadings::in_range);
        Self {
            writable_frames: readings.and_then(|readings| readings.writable_frames),
            readings,
        }
    }

    /// The frames of a `frames`-frame write the output takes, taken from what it can still take.
    fn take(&mut self, frames: i64) -> i64 {
        match &mut self.writable_frames {
            None => frames,
            Some(writable) => {
                let taken = frames.min(*writable);
                *writable -= taken;
                taken
            }
        }
    }

    fn presented_frame(&self) -> Option<i64> {
        self.readings.and_then(|readings| readings.presented_frame)
    }

    fn is_playing(&self) -> bool {
        self.readings.is_none_or(|readings| readings.is_playing)
    }

    fn underrun_count(&self) -> Option<i64> {
        self.readings.map(|readings| readings.underrun_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cushion::START_PACKETS;

    // Datagrams.

    /// The bytes of one audio datagram as the desk sends it (`apps/server/src/protocol.rs:476-498`).
    fn datagram(sequence: u64, samples: &[i16], channels: u8, sample_rate: u32) -> Vec<u8> {
        let mut bytes = b"PMON".to_vec();
        bytes.push(1);
        bytes.push(channels);
        bytes.extend_from_slice(&sample_rate.to_le_bytes());
        bytes.extend_from_slice(&sequence.to_le_bytes());
        bytes.extend_from_slice(&(samples.len() as u16).to_le_bytes());
        for sample in samples {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        bytes
    }

    /// `frames` stereo frames, every sample `value`.
    fn stereo(frames: usize, value: i16) -> Vec<i16> {
        vec![value; frames * 2]
    }

    /// A 10 ms packet at 48 kHz, the size the tests play unless they say otherwise.
    const PACKET_FRAMES: usize = 480;
    const PACKET_FRAMES_I64: i64 = PACKET_FRAMES as i64;

    /// Packet `sequence`: 10 ms of stereo at 48 kHz, every sample `value`.
    fn packet(sequence: u64, value: i16) -> Vec<u8> {
        datagram(sequence, &stereo(PACKET_FRAMES, value), 2, 48_000)
    }

    // Fakes.

    /// The platform's output as the tests set it, and everything the pipeline asked of it.
    struct FakeOutput {
        capacity_frames: i64,
        /// The frames each wake-up's reading says the output takes; `None` takes every frame.
        writable_frames: Option<i64>,
        /// What a target answers: the target itself unless a test says otherwise.
        applies: fn(i64) -> Option<i64>,
        underruns: i64,
        presented_frame: Option<i64>,
        is_playing: bool,

        writes: Vec<Vec<i16>>,
        targets: Vec<i64>,
        /// Readings taken at a wake-up's start, for a report.
        reads_for_report: usize,
    }

    impl FakeOutput {
        fn readings(&self) -> OutputReadings {
            OutputReadings {
                writable_frames: self.writable_frames,
                underrun_count: self.underruns,
                presented_frame: self.presented_frame,
                is_playing: self.is_playing,
            }
        }
    }

    /// The platform's receive loop with every collaborator faked, started at 0 ms, driven one
    /// wake-up at a time exactly as the module's pseudo-loop runs it.
    struct Rig {
        pipeline: ReceivePipeline,
        settings: PlayerSettings,
        output: FakeOutput,
        opened: bool,
        /// How many opens fail before one succeeds.
        open_failures: usize,
        /// The outputs opened.
        opens: Vec<OutputFormat>,
        /// The opens that failed, which the platform reports itself.
        failed_opens: usize,
        stats: Vec<ReceiverStats>,
        reports: Vec<LinkStatsReport>,
        errors: Vec<ReceiveError>,
        applied: Option<i64>,
        report_due_at_ms: i64,
    }

    impl Rig {
        fn new() -> Self {
            Self::with(PlayerSettings::default())
        }

        fn with(settings: PlayerSettings) -> Self {
            let pipeline = ReceivePipeline::new(0);
            let report_due_at_ms = pipeline.report_due_at_ms();
            Self {
                pipeline,
                settings,
                output: FakeOutput {
                    capacity_frames: 24_000,
                    writable_frames: None,
                    applies: Some,
                    underruns: 0,
                    presented_frame: None,
                    is_playing: true,
                    writes: Vec::new(),
                    targets: Vec::new(),
                    reads_for_report: 0,
                },
                opened: false,
                open_failures: 0,
                opens: Vec::new(),
                failed_opens: 0,
                stats: Vec::new(),
                reports: Vec::new(),
                errors: Vec::new(),
                applied: None,
                report_due_at_ms,
            }
        }

        /// One wake-up at `milliseconds` on which `bytes` arrived.
        fn arrive_bytes(&mut self, bytes: Vec<u8>, milliseconds: i64) {
            self.wake(Some(bytes), milliseconds);
        }

        /// One wake-up at `milliseconds` on which packet `sequence` arrived, 10 ms of stereo at
        /// 48 kHz, every sample 1 000.
        fn arrive(&mut self, sequence: u64, milliseconds: i64) {
            self.arrive_valued(sequence, milliseconds, 1_000);
        }

        fn arrive_valued(&mut self, sequence: u64, milliseconds: i64, value: i16) {
            self.wake(Some(packet(sequence, value)), milliseconds);
        }

        /// One wake-up at `milliseconds` on which nothing arrived.
        fn time_out(&mut self, milliseconds: i64) {
            self.wake(None, milliseconds);
        }

        fn wake(&mut self, datagram: Option<Vec<u8>>, milliseconds: i64) {
            let now_ns = milliseconds as u64 * 1_000_000;
            let output_at_start = if self.opened && milliseconds >= self.report_due_at_ms {
                self.output.reads_for_report += 1;
                Some(self.output.readings())
            } else {
                None
            };
            let wake = Wake {
                started_ns: now_ns,
                output_at_start,
                applied_target_frames: self.applied.take(),
                datagram,
                received_ns: now_ns,
                output_at_receive: self.opened.then(|| self.output.readings()),
                settings: self.settings,
            };
            let mut outcome = self.pipeline.on_wake(wake);
            while let Some(format) = self.act_on(outcome) {
                outcome = if self.open_failures > 0 {
                    self.open_failures -= 1;
                    self.failed_opens += 1;
                    self.pipeline.on_output_failed()
                } else {
                    self.opened = true;
                    self.opens.push(format);
                    self.pipeline.on_output_opened(
                        now_ns,
                        self.output.capacity_frames,
                        self.output.readings(),
                    )
                };
            }
        }

        /// Acts on `outcome` as the platform does, answering the output to open, if any.
        fn act_on(&mut self, outcome: WakeOutcome) -> Option<OutputFormat> {
            if outcome.open_output.is_some() {
                assert!(
                    outcome.actions.is_empty() && outcome.stats.is_empty(),
                    "nothing is written before the output opens"
                );
            }
            self.reports.extend(outcome.report);
            for action in outcome.actions {
                match action {
                    OutputAction::Write { samples } => self.output.writes.push(samples),
                    OutputAction::SetTargetFrames { frames } => {
                        self.output.targets.push(frames);
                        self.applied = Some((self.output.applies)(frames).unwrap_or(0));
                    }
                }
            }
            self.stats.extend(outcome.stats);
            self.errors.extend(outcome.error);
            self.report_due_at_ms = outcome.report_due_at_ms;
            outcome.open_output
        }

        fn last_stats(&self) -> ReceiverStats {
            *self.stats.last().expect("no packet played")
        }

        fn first_report(&self) -> LinkStatsReport {
            *self.reports.first().expect("no report was handed over")
        }
    }

    fn fixed(milliseconds: u16) -> PlaybackBufferSetting {
        PlaybackBufferSetting::Fixed { milliseconds }
    }

    /// The receive loop's tests, ported case for case from `ReceiveLoopTests.swift`, wake-up by
    /// wake-up, against a fake output and a clock the test sets. Android has no test of its own
    /// for the loop, so each case pins one rule of the Kotlin as it reads. Swift's `sink` is the
    /// output here; what the iOS rig set on its fake sink, this one sets on the readings.
    ///
    /// The five `Running` cases stay with the platforms: the session flag, the socket's failure or
    /// close, the stop's wake-up datagram and closing the output are the receive thread's, which
    /// stays native.
    mod receive_loop {
        use super::*;

        // The output.

        #[test]
        fn the_first_packet_opens_the_sink_at_its_rate_and_packet_frames() {
            let mut rig = Rig::new();

            rig.arrive_bytes(datagram(1, &stereo(441, 1_000), 2, 44_100), 0);

            assert_eq!(
                rig.opens,
                [OutputFormat {
                    sample_rate: 44_100,
                    packet_frames: 441
                }]
            );
            assert_eq!(rig.output.writes.len(), 1);
        }

        #[test]
        fn the_first_packet_asks_the_sink_for_the_start_cushion() {
            let mut rig = Rig::new();

            rig.arrive(1, 0);

            assert_eq!(rig.output.targets, [START_PACKETS * PACKET_FRAMES_I64]);
        }

        #[test]
        fn a_buffer_the_desk_set_before_the_audio_started_applies_from_the_first_packet() {
            // 30 ms at 48 kHz in 480-frame packets: three packets.
            let mut rig = Rig::with(PlayerSettings {
                playback_buffer: fixed(30),
                ..PlayerSettings::default()
            });

            rig.arrive(1, 0);

            assert_eq!(rig.output.targets, [3 * PACKET_FRAMES_I64]);
        }

        /// Android follows the desk's buffer in packets of the size the track opened with, and
        /// counts time in the frames of the packet played (`PmonPlayer.kt:320-323`). 30 ms at
        /// 48 kHz is three of the opening 480-frame packets, 1 440 frames, even when a later packet
        /// carries 240; sizing it in the later packet would ask for six of the opening ones.
        #[test]
        fn a_buffer_the_desk_sets_later_is_sized_in_the_packets_the_sink_opened_with() {
            let mut rig = Rig::new();

            rig.arrive(1, 0);
            rig.settings.playback_buffer = fixed(30);
            rig.arrive_bytes(datagram(2, &stereo(240, 1_000), 2, 48_000), 10);

            assert_eq!(
                rig.output.targets,
                [START_PACKETS * PACKET_FRAMES_I64, 3 * PACKET_FRAMES_I64]
            );
        }

        #[test]
        fn later_packets_play_through_the_sink_the_first_opened() {
            let mut rig = Rig::new();

            rig.arrive(1, 0);
            rig.arrive(2, 10);
            rig.arrive(3, 20);

            assert_eq!(rig.opens.len(), 1);
            assert_eq!(rig.output.writes.len(), 3);
        }

        /// The open's error is the platform's to report; the pipeline writes nothing and asks
        /// again on the next packet.
        #[test]
        fn a_sink_that_fails_to_open_is_an_error_and_the_next_packet_opens_it_again() {
            let mut rig = Rig::new();
            rig.open_failures = 1;

            rig.arrive(1, 0);
            assert_eq!(rig.failed_opens, 1);
            assert!(rig.errors.is_empty());
            assert!(rig.output.writes.is_empty());

            rig.arrive(2, 10);
            assert_eq!(rig.opens.len(), 1);
            assert_eq!(rig.output.writes.len(), 1);
        }

        // Local protection.

        #[test]
        fn local_protection_scales_the_packet_while_the_desk_does_not_own_the_mix() {
            let mut rig = Rig::with(PlayerSettings {
                volume_percent: 50,
                server_owns_mix: false,
                ..PlayerSettings::default()
            });

            rig.arrive_valued(1, 0, 1_000);

            assert_eq!(rig.output.writes.first(), Some(&stereo(PACKET_FRAMES, 500)));
        }

        #[test]
        fn the_packet_plays_as_sent_once_the_desk_owns_the_mix() {
            let mut rig = Rig::with(PlayerSettings {
                volume_percent: 50,
                server_owns_mix: true,
                ..PlayerSettings::default()
            });

            rig.arrive_valued(1, 0, 1_000);

            assert_eq!(
                rig.output.writes.first(),
                Some(&stereo(PACKET_FRAMES, 1_000))
            );
        }

        #[test]
        fn a_control_changed_on_another_thread_applies_from_the_next_wake_up() {
            let mut rig = Rig::new();

            rig.arrive_valued(1, 0, 1_000);
            rig.settings.muted = true;
            rig.arrive_valued(2, 10, 1_000);

            assert_eq!(rig.output.writes.len(), 2);
            assert_eq!(rig.output.writes.last(), Some(&stereo(PACKET_FRAMES, 0)));
        }

        // The hold.

        #[test]
        fn a_duplicate_packet_is_skipped() {
            let mut rig = Rig::new();

            rig.arrive(1, 0);
            rig.arrive(1, 5);

            assert_eq!(rig.output.writes.len(), 1);
            assert_eq!(rig.stats.len(), 1);
        }

        #[test]
        fn a_packet_arriving_after_a_newer_one_was_played_is_skipped() {
            let mut rig = Rig::new();

            rig.arrive_valued(2, 0, 2);
            rig.arrive_valued(1, 5, 1);

            assert_eq!(rig.output.writes, [stereo(PACKET_FRAMES, 2)]);
        }

        #[test]
        fn a_skipped_packet_is_not_timed_as_an_arrival() {
            let mut rig = Rig::new();

            rig.arrive(1, 1_000);
            rig.arrive(1, 1_003);
            rig.arrive(2, 1_010);

            assert_eq!(rig.last_stats().inter_arrival_ms, Some(10.0));
        }

        #[test]
        fn the_redundancy_hold_is_read_from_the_controls() {
            let mut rig = Rig::with(PlayerSettings {
                redundancy_hold_packets: 1,
                ..PlayerSettings::default()
            });

            rig.arrive_valued(1, 0, 1);
            assert!(rig.output.writes.is_empty());

            rig.arrive_valued(2, 10, 2);
            assert_eq!(rig.output.writes, [stereo(PACKET_FRAMES, 1)]);
        }

        #[test]
        fn a_timeout_releases_what_the_hold_keeps() {
            let mut rig = Rig::with(PlayerSettings {
                redundancy_hold_packets: 1,
                ..PlayerSettings::default()
            });

            rig.arrive(1, 0);
            rig.time_out(250);

            assert_eq!(rig.output.writes.len(), 1);
        }

        #[test]
        fn a_timeout_is_counted_in_the_next_report() {
            let mut rig = Rig::new();

            rig.arrive(1, 0);
            rig.time_out(500);
            rig.time_out(1_000);

            assert_eq!(rig.first_report().receive_timeouts, 1);
        }

        // Link reports.

        #[test]
        fn no_report_is_handed_over_before_one_is_due() {
            let mut rig = Rig::new();

            rig.arrive(1, 0);
            rig.time_out(999);

            assert!(rig.reports.is_empty());
        }

        #[test]
        fn a_due_report_carries_the_cushion_the_sink_applied_and_the_backlog_it_presented() {
            let mut rig = Rig::new();
            rig.output.applies = |_| Some(1_200);

            rig.arrive(1, 0);
            rig.output.presented_frame = Some(240);
            rig.time_out(1_000);

            let report = rig.first_report();
            assert_eq!(rig.reports.len(), 1);
            // 1 200 frames applied at 48 kHz, and 480 written less 240 presented.
            assert_eq!(report.cushion_ms, Some(25.0));
            assert_eq!(report.backlog_ms, Some(5.0));
            assert_eq!(report.interval_ms, 1_000);
            assert_eq!(report.packets_received, 1);
        }

        #[test]
        fn a_report_has_no_backlog_while_the_sink_has_presented_nothing() {
            let mut rig = Rig::new();

            rig.arrive(1, 0);
            rig.time_out(1_000);

            let report = rig.first_report();
            assert_eq!(report.backlog_ms, None);
            assert_eq!(report.cushion_ms, Some(20.0));
        }

        #[test]
        fn a_report_carries_the_sinks_underruns() {
            let mut rig = Rig::new();

            rig.arrive(1, 0);
            rig.output.underruns = 3;
            rig.time_out(1_000);

            assert_eq!(rig.first_report().underruns, 3);
        }

        #[test]
        fn a_report_is_taken_before_the_wake_up_receives() {
            let mut rig = Rig::new();

            rig.arrive(1, 0);
            rig.arrive(2, 1_000);

            assert_eq!(rig.first_report().packets_received, 1);
            assert_eq!(rig.output.writes.len(), 2);
        }

        /// The iOS loop reads the output's underruns only once a report is due. Here the
        /// platform reads the output at a wake-up's start only from
        /// [`WakeOutcome::report_due_at_ms`] on, and that instant is the one the report is due at.
        /// (The cushion's own reading, after the receive, is taken on every wake-up instead: see
        /// the module's notes.)
        #[test]
        fn the_sinks_underruns_are_read_only_once_a_report_is_due() {
            let mut rig = Rig::new();

            for sequence in 1..=5 {
                rig.arrive(sequence, sequence as i64 * 10);
            }
            assert_eq!(rig.output.reads_for_report, 0);

            rig.time_out(1_010);
            assert_eq!(rig.output.reads_for_report, 1);
            assert_eq!(rig.reports.len(), 1);
        }

        // Per-packet stats.

        #[test]
        fn each_played_packet_reports_the_receivers_counts() {
            let mut rig = Rig::new();

            rig.arrive(1, 1_000);
            // Packet 2 never comes.
            rig.arrive(3, 1_010);

            assert_eq!(rig.stats.len(), 2);
            assert_eq!(
                rig.last_stats(),
                ReceiverStats {
                    packets: 2,
                    lost: 1,
                    sample_rate: 48_000,
                    packets_per_second: 2.0 / 1.01,
                    inter_arrival_ms: Some(10.0),
                    jitter_ms: Some(0.0),
                    discarded_samples: 0,
                    packets_with_discarded_samples: 0,
                    playback_backlog_ms: None,
                    output_playing: true,
                }
            );
        }

        #[test]
        fn each_packets_stats_say_whether_the_output_was_playing() {
            let mut rig = Rig::new();

            rig.arrive(1, 0);
            rig.output.is_playing = false;
            rig.arrive(2, 10);

            let playing: Vec<bool> = rig.stats.iter().map(|s| s.output_playing).collect();
            assert_eq!(playing, [true, false]);
        }

        #[test]
        fn the_backlog_is_seeded_with_its_first_value_then_smoothed_by_a_tenth() {
            let mut rig = Rig::new();
            rig.output.presented_frame = Some(0);

            // 480 frames queued is 10 ms, then 960 is 20 ms.
            rig.arrive(1, 0);
            rig.arrive(2, 10);

            let backlog: Vec<Option<f64>> =
                rig.stats.iter().map(|s| s.playback_backlog_ms).collect();
            assert_eq!(backlog, [Some(10.0), Some(11.0)]);
        }

        #[test]
        fn no_backlog_is_measured_while_the_sink_has_presented_nothing() {
            let mut rig = Rig::new();

            rig.arrive(1, 0);

            assert_eq!(rig.last_stats().playback_backlog_ms, None);
        }

        // Short writes.

        #[test]
        fn a_short_write_counts_the_samples_it_discarded() {
            let mut rig = Rig::new();
            rig.output.writable_frames = Some(100);

            rig.arrive(1, 0);
            rig.arrive(2, 10);

            let stats = rig.last_stats();
            // 960 samples sent, 200 taken, each time.
            assert_eq!(stats.discarded_samples, 1_520);
            assert_eq!(stats.packets_with_discarded_samples, 2);
        }

        #[test]
        fn only_the_frames_the_sink_took_count_as_written() {
            let mut rig = Rig::new();
            rig.output.writable_frames = Some(120);
            rig.output.presented_frame = Some(0);

            rig.arrive(1, 0);

            // 120 frames at 48 kHz.
            assert_eq!(rig.last_stats().playback_backlog_ms, Some(2.5));
            assert_eq!(rig.output.writes, [stereo(120, 1_000)]);
        }

        // Errors.

        #[test]
        fn a_sample_rate_change_is_an_error_for_every_later_packet_and_never_reopens_the_sink() {
            let mut rig = Rig::new();

            rig.arrive_bytes(datagram(1, &stereo(480, 1_000), 2, 48_000), 0);
            rig.arrive_bytes(datagram(2, &stereo(441, 1_000), 2, 44_100), 10);
            rig.arrive_bytes(datagram(3, &stereo(441, 1_000), 2, 44_100), 20);

            let changed = ReceiveError::SampleRateChanged {
                from: 48_000,
                to: 44_100,
            };
            assert_eq!(rig.errors, [changed, changed]);
            assert_eq!(rig.opens.len(), 1);
            assert_eq!(rig.output.writes.len(), 1);
        }

        #[test]
        fn a_packet_with_more_than_32_channels_is_rejected() {
            let mut rig = Rig::new();

            rig.arrive_bytes(datagram(1, &vec![1; 33 * 10], 33, 48_000), 0);

            assert_eq!(rig.errors, [ReceiveError::TooManyChannels { channels: 33 }]);
            assert!(rig.opens.is_empty());
        }

        #[test]
        fn a_packet_with_32_channels_is_played() {
            let mut rig = Rig::new();

            rig.arrive_bytes(datagram(1, &vec![1; 32 * 30], 32, 48_000), 0);

            assert!(rig.errors.is_empty());
            assert_eq!(rig.output.writes.len(), 1);
        }

        #[test]
        fn an_empty_datagram_while_the_session_runs_is_an_error() {
            let mut rig = Rig::new();

            rig.arrive_bytes(Vec::new(), 0);

            assert_eq!(
                rig.errors,
                [ReceiveError::Undecodable {
                    reason: DecodeError::HeaderTooShort { byte_count: 0 }
                }]
            );
        }

        #[test]
        fn an_error_in_one_packet_does_not_stop_the_loop() {
            let mut rig = Rig::new();

            rig.arrive_bytes(b"not audio".to_vec(), 0);
            rig.arrive(1, 10);

            assert_eq!(rig.errors.len(), 1);
            assert!(matches!(rig.errors[0], ReceiveError::Undecodable { .. }));
            assert_eq!(rig.output.writes.len(), 1);
        }

        // The cushion.

        #[test]
        fn a_second_of_audio_adjusts_the_cushion_with_the_sinks_underrun_count() {
            let mut rig = Rig::new();
            rig.output.underruns = 1;

            // 100 packets of 10 ms: one second of audio, so one adjustment.
            for sequence in 1..=100 {
                rig.arrive(sequence, sequence as i64 * 10);
            }

            assert_eq!(
                rig.output.targets,
                [2 * PACKET_FRAMES_I64, 3 * PACKET_FRAMES_I64]
            );
        }

        #[test]
        fn a_clean_second_leaves_the_cushion_where_it_is() {
            let mut rig = Rig::new();

            for sequence in 1..=100 {
                rig.arrive(sequence, sequence as i64 * 10);
            }

            assert_eq!(rig.output.targets, [2 * PACKET_FRAMES_I64]);
        }

        #[test]
        fn a_buffer_the_desk_sets_while_playing_is_asked_of_the_sink() {
            let mut rig = Rig::new();

            rig.arrive(1, 0);
            rig.settings.playback_buffer = fixed(40);
            rig.arrive(2, 10);

            assert_eq!(
                rig.output.targets,
                [2 * PACKET_FRAMES_I64, 4 * PACKET_FRAMES_I64]
            );
        }
    }

    // What only the one-call boundary has to answer.

    /// The open asked for comes first, alone: a wake-up whose packet opens the output writes
    /// nothing until the platform answers, and then asks for the cushion before the first write,
    /// so the output primes on it.
    #[test]
    fn the_cushion_is_asked_before_the_first_write() {
        let mut pipeline = ReceivePipeline::new(0);

        let first = pipeline.on_wake(wake_with(Some(packet(1, 7)), 0, None));
        assert_eq!(
            first.open_output,
            Some(OutputFormat {
                sample_rate: 48_000,
                packet_frames: PACKET_FRAMES_I64
            })
        );
        assert!(first.actions.is_empty());

        let opened = pipeline.on_output_opened(0, 24_000, readings());
        assert_eq!(
            opened.actions,
            [
                OutputAction::SetTargetFrames {
                    frames: START_PACKETS * PACKET_FRAMES_I64
                },
                OutputAction::Write {
                    samples: stereo(PACKET_FRAMES, 7)
                },
            ]
        );
        assert_eq!(opened.open_output, None);
    }

    /// An open nobody answered counts as failed once the next wake-up comes: the packet that
    /// asked is not played and the next one asks again. Answering when nothing was asked changes
    /// nothing.
    #[test]
    fn an_open_never_answered_counts_as_failed() {
        let mut pipeline = ReceivePipeline::new(0);

        assert!(pipeline
            .on_output_opened(0, 24_000, readings())
            .actions
            .is_empty());
        assert!(pipeline.on_output_failed().actions.is_empty());

        let first = pipeline.on_wake(wake_with(Some(packet(1, 1)), 0, None));
        assert!(first.open_output.is_some());
        let second = pipeline.on_wake(wake_with(Some(packet(2, 2)), 10, None));
        assert!(second.open_output.is_some());

        let opened = pipeline.on_output_opened(10, 24_000, readings());
        let writes: Vec<&OutputAction> = opened
            .actions
            .iter()
            .filter(|action| matches!(action, OutputAction::Write { .. }))
            .collect();
        assert_eq!(
            writes,
            [&OutputAction::Write {
                samples: stereo(PACKET_FRAMES, 2)
            }]
        );
        assert_eq!(opened.stats.last().map(|s| s.packets), Some(2));
    }

    /// The packets a wake-up plays share what the output said it could take: the iOS ring's free
    /// space shrinks with every write.
    #[test]
    fn a_wake_ups_writes_share_what_the_output_can_take() {
        let mut rig = Rig::with(PlayerSettings {
            redundancy_hold_packets: 2,
            ..PlayerSettings::default()
        });
        rig.arrive(1, 0);
        rig.arrive(2, 10);
        rig.arrive(3, 20);
        rig.output.writable_frames = Some(PACKET_FRAMES_I64 + 100);

        // The timeout releases 2 and 3 at once.
        rig.time_out(270);

        let written: Vec<usize> = rig.output.writes.iter().map(Vec::len).collect();
        assert_eq!(written, [PACKET_FRAMES * 2, PACKET_FRAMES * 2, 200]);
        assert_eq!(
            rig.last_stats().discarded_samples,
            (PACKET_FRAMES as i64 - 100) * 2
        );
    }

    /// A report due while an output is open waits for the platform to read the output, rather
    /// than reporting no underruns and counting them all in the next one.
    #[test]
    fn a_due_report_waits_for_the_outputs_reading() {
        let mut pipeline = ReceivePipeline::new(0);
        let _ = pipeline.on_wake(wake_with(Some(packet(1, 1)), 0, None));
        let opened = pipeline.on_output_opened(0, 24_000, readings());
        assert_eq!(opened.report_due_at_ms, 1_000);

        let unread = pipeline.on_wake(wake_with(None, 1_000, None));
        assert_eq!(unread.report, None);
        assert_eq!(unread.report_due_at_ms, 1_000);

        let read = pipeline.on_wake(wake_with(
            None,
            1_250,
            Some(OutputReadings {
                underrun_count: 2,
                ..readings()
            }),
        ));
        assert_eq!(read.report.map(|r| r.underruns), Some(2));
        assert_eq!(read.report_due_at_ms, 2_250);
    }

    /// Readings the platform got wrong are brought into range: a frame position or an underrun
    /// count below zero reads as 0, and so does a capacity or writable count below zero.
    #[test]
    fn readings_out_of_range_read_as_zero() {
        let mut pipeline = ReceivePipeline::new(0);
        let _ = pipeline.on_wake(wake_with(Some(packet(1, 1)), 0, None));
        let opened = pipeline.on_output_opened(
            0,
            -5,
            OutputReadings {
                writable_frames: Some(-3),
                underrun_count: i64::MIN,
                presented_frame: Some(i64::MIN),
                is_playing: true,
            },
        );

        let stats = opened.stats[0];
        assert_eq!(stats.discarded_samples, PACKET_FRAMES as i64 * 2);
        assert_eq!(stats.playback_backlog_ms, Some(0.0));
    }

    /// The pipeline is handed to the platform's receive thread whole, behind the binding's lock.
    #[test]
    fn the_pipeline_moves_between_threads() {
        fn assert_send<T: Send>() {}
        assert_send::<ReceivePipeline>();
        assert_send::<WakeOutcome>();
    }

    /// Fresh output readings: takes everything, nothing presented, no underruns, playing.
    fn readings() -> OutputReadings {
        OutputReadings {
            writable_frames: None,
            underrun_count: 0,
            presented_frame: None,
            is_playing: true,
        }
    }

    /// A wake-up at `milliseconds` with default settings, reading `at_start` for a report and
    /// fresh readings after the receive.
    fn wake_with(
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
            settings: PlayerSettings::default(),
        }
    }

    // Whatever the network sends.

    /// SplitMix64: a seeded generator, so a failing run reproduces from its seed alone.
    struct SplitMix64 {
        state: u64,
    }

    impl SplitMix64 {
        fn next(&mut self) -> u64 {
            self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut mixed = self.state;
            mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            mixed ^ (mixed >> 31)
        }

        /// A value in `0..bound`, `bound` at least 1; the slight bias does not matter here.
        fn below(&mut self, bound: u64) -> u64 {
            self.next() % bound
        }

        fn chance(&mut self, percent: u64) -> bool {
            self.below(100) < percent
        }

        fn pick<T: Copy>(&mut self, values: &[T]) -> T {
            values[self.below(values.len() as u64) as usize]
        }

        /// An `i64` that is often one of the edges.
        fn edgy_i64(&mut self) -> i64 {
            if self.chance(50) {
                self.pick(&[i64::MIN, -1, 0, 1, 100, 480, i64::MAX - 1, i64::MAX])
            } else {
                self.next() as i64
            }
        }
    }

    /// One datagram as the network might deliver it: noise, a truncated header, or a header with
    /// any fields, including the sequences no stream reaches, behind a payload that may not match.
    fn garbage_datagram(generator: &mut SplitMix64, last_sequence: &mut u64) -> Vec<u8> {
        match generator.below(6) {
            0 => (0..generator.below(64))
                .map(|_| generator.next() as u8)
                .collect(),
            1 => {
                let mut bytes = packet(generator.next(), 1);
                bytes.truncate(generator.below(20) as usize);
                bytes
            }
            _ => {
                let sequence = match generator.below(8) {
                    0 => generator.pick(&[
                        i64::MAX as u64 - 1,
                        i64::MAX as u64,
                        i64::MAX as u64 + 1,
                        u64::MAX,
                        0,
                    ]),
                    1 => *last_sequence,
                    2 => last_sequence.wrapping_sub(generator.below(300)),
                    3 => generator.next(),
                    _ => last_sequence.wrapping_add(generator.below(8)),
                };
                *last_sequence = sequence;
                // Mostly the stream's own format, so the garbage reaches the play path too.
                let channels = generator.pick(&[0, 1, 2, 2, 2, 2, 2, 2, 32, 33, 255]);
                let sample_rate = if generator.chance(75) {
                    48_000
                } else {
                    generator.pick(&[0, 1, 44_100, i32::MAX as u32, i32::MAX as u32 + 1, u32::MAX])
                };
                let frames = generator.pick(&[0, 1, 64, 128, 481]);
                // At most a few thousand samples, so the run stays quick; a count that is then no
                // whole number of frames is garbage too.
                let count = (frames * usize::from(channels.max(1))).min(4_096);
                let samples = vec![generator.next() as i16; count];
                let mut bytes = datagram(sequence, &samples, channels, sample_rate);
                if generator.chance(15) {
                    // A payload that is not the declared samples.
                    let keep = generator.below(bytes.len() as u64 + 1) as usize;
                    bytes.truncate(keep);
                    bytes.extend((0..generator.below(4)).map(|_| generator.next() as u8));
                }
                bytes
            }
        }
    }

    fn garbage_readings(generator: &mut SplitMix64) -> OutputReadings {
        OutputReadings {
            writable_frames: generator.chance(50).then(|| generator.edgy_i64()),
            underrun_count: generator.edgy_i64(),
            presented_frame: generator.chance(70).then(|| generator.edgy_i64()),
            is_playing: generator.chance(80),
        }
    }

    fn garbage_settings(generator: &mut SplitMix64) -> PlayerSettings {
        PlayerSettings {
            volume_percent: generator.edgy_i64(),
            max_level_percent: generator.edgy_i64(),
            muted: generator.chance(20),
            server_owns_mix: generator.chance(50),
            playback_buffer: if generator.chance(50) {
                PlaybackBufferSetting::Auto
            } else {
                fixed(generator.next() as u16)
            },
            redundancy_hold_packets: generator.edgy_i64(),
        }
    }

    /// What the garbage reached, so a generator that only ever produced refusals fails the test.
    #[derive(Debug, Default)]
    struct Seen {
        played: usize,
        reports: usize,
        errors: usize,
    }

    /// What every outcome keeps to, whatever came in: a write per packet played, with its stats,
    /// and nothing to write while an output is being asked for.
    #[track_caller]
    fn check(outcome: &WakeOutcome, seed: u64, seen: &mut Seen) {
        seen.played += outcome.stats.len();
        seen.reports += usize::from(outcome.report.is_some());
        seen.errors += usize::from(outcome.error.is_some());
        let writes = outcome
            .actions
            .iter()
            .filter(|action| matches!(action, OutputAction::Write { .. }))
            .count();
        assert_eq!(writes, outcome.stats.len(), "seed {seed}");
        if outcome.open_output.is_some() {
            assert!(outcome.actions.is_empty(), "seed {seed}");
        }
    }

    /// The pipeline never panics on what the network sends (L11): noise, truncated headers, any
    /// header field, huge and repeated sequences, timeouts and settings at their extremes, and
    /// output readings and open answers the platform got wrong, in any order. A panic would reach
    /// the apps as a binding error on the receive thread. Deterministic: a failing run reproduces
    /// from its seed.
    #[test]
    fn never_panics_on_what_the_network_sends() {
        let mut seen = Seen::default();
        for seed in 0..64_u64 {
            let mut generator = SplitMix64 { state: seed };
            let mut clock_ns: u64 = generator.below(1 << 40);
            let mut pipeline = ReceivePipeline::new(clock_ns);
            let mut last_sequence = generator.next();

            for _ in 0..1_500 {
                // Mostly forward by a packet's worth, sometimes stalled or far ahead.
                clock_ns = clock_ns.wrapping_add(match generator.below(10) {
                    0 => 0,
                    1 => generator.below(5_000_000_000),
                    _ => generator.below(20_000_000),
                });
                let started_ns = clock_ns;
                clock_ns = clock_ns.wrapping_add(generator.below(300_000_000));
                let wake = Wake {
                    started_ns,
                    output_at_start: generator
                        .chance(50)
                        .then(|| garbage_readings(&mut generator)),
                    applied_target_frames: generator.chance(30).then(|| generator.edgy_i64()),
                    datagram: generator
                        .chance(85)
                        .then(|| garbage_datagram(&mut generator, &mut last_sequence)),
                    received_ns: clock_ns,
                    output_at_receive: generator
                        .chance(90)
                        .then(|| garbage_readings(&mut generator)),
                    settings: garbage_settings(&mut generator),
                };
                let mut outcome = pipeline.on_wake(wake);
                check(&outcome, seed, &mut seen);
                while outcome.open_output.is_some() {
                    outcome = match generator.below(4) {
                        0 => pipeline.on_output_failed(),
                        // Not answered: the next wake-up comes anyway.
                        1 => break,
                        _ => pipeline.on_output_opened(
                            clock_ns,
                            generator.edgy_i64(),
                            garbage_readings(&mut generator),
                        ),
                    };
                    check(&outcome, seed, &mut seen);
                }
                if generator.chance(2) {
                    // Answers nobody asked for.
                    check(&pipeline.on_output_failed(), seed, &mut seen);
                    let readings = garbage_readings(&mut generator);
                    check(
                        &pipeline.on_output_opened(clock_ns, generator.edgy_i64(), readings),
                        seed,
                        &mut seen,
                    );
                }
            }
        }
        // Every path was reached, not only the refusals.
        assert!(seen.played > 3_000, "{seen:?}");
        assert!(seen.reports > 100, "{seen:?}");
        assert!(seen.errors > 1_000, "{seen:?}");
    }

    /// S8: what one wake-up costs in steady state, for a stereo 128-frame packet at 48 kHz
    /// (2.67 ms of audio), with local protection on, a report every second and the cushion's
    /// adjustment every second of audio. The budget is 5 % of the packet, about 133 µs; the
    /// binding's own crossing is not in this number.
    ///
    /// `cargo test --release -p bandear-core -- --ignored --nocapture`
    #[test]
    #[ignore = "a measurement: run it in release with --ignored --nocapture"]
    fn measure_one_wake_up() {
        use std::hint::black_box;
        use std::time::Instant;

        const FRAMES: usize = 128;
        const PACKET_NS: u64 = 2_666_667;
        const WARM_UP: u64 = 20_000;
        const MEASURED: u64 = 500_000;

        let samples: Vec<i16> = (0..FRAMES * 2).map(|i| (i as i16) * 97).collect();
        let settings = PlayerSettings {
            volume_percent: 80,
            ..PlayerSettings::default()
        };
        let output = OutputReadings {
            writable_frames: Some(24_000),
            underrun_count: 0,
            presented_frame: Some(0),
            is_playing: true,
        };
        let mut pipeline = ReceivePipeline::new(0);
        let mut applied = None;
        let mut due_at = pipeline.report_due_at_ms();
        // One wake-up as the platform runs it, from its received bytes on; what the outcome asks
        // of the output is only noted, as a real output would only copy the samples.
        let mut run =
            |sequence: u64, bytes: Vec<u8>, applied: &mut Option<i64>, due_at: &mut i64| {
                let now_ns = sequence * PACKET_NS;
                let wake = Wake {
                    started_ns: now_ns,
                    output_at_start: ((now_ns / 1_000_000) as i64 >= *due_at).then_some(output),
                    applied_target_frames: applied.take(),
                    datagram: Some(bytes),
                    received_ns: now_ns,
                    output_at_receive: Some(output),
                    settings,
                };
                let mut outcome = pipeline.on_wake(black_box(wake));
                if outcome.open_output.is_some() {
                    outcome = pipeline.on_output_opened(now_ns, 24_000, output);
                }
                for action in &outcome.actions {
                    if let OutputAction::SetTargetFrames { frames } = action {
                        *applied = Some(*frames);
                    }
                }
                *due_at = outcome.report_due_at_ms;
                black_box(outcome)
            };

        for sequence in 0..WARM_UP {
            run(
                sequence,
                datagram(sequence, &samples, 2, 48_000),
                &mut applied,
                &mut due_at,
            );
        }
        let mut elapsed = std::time::Duration::ZERO;
        let mut reports = 0;
        for sequence in WARM_UP..WARM_UP + MEASURED {
            // The bytes are built outside the timing: the platform's socket delivers them.
            let bytes = datagram(sequence, &samples, 2, 48_000);
            let started = Instant::now();
            let outcome = run(sequence, bytes, &mut applied, &mut due_at);
            elapsed += started.elapsed();
            reports += usize::from(outcome.report.is_some());
        }
        let per_call = elapsed.as_nanos() as f64 / MEASURED as f64;
        let budget = PACKET_NS as f64 * 0.05;
        println!(
            "S8: {per_call:.0} ns per wake-up over {MEASURED} wake-ups ({reports} reports), \
             {:.3} % of a 2.67 ms packet; budget {budget:.0} ns (5 %)",
            per_call / PACKET_NS as f64 * 100.0
        );
        assert!(
            per_call < budget,
            "{per_call} ns per wake-up is over the budget"
        );
    }
}
