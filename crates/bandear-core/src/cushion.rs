//! The playback cushion: how many packets of audio the player keeps queued ahead of what it is
//! playing, the rules that size and move it, and the controller one receive loop keeps it with.
//!
//! Ported from the iOS client's `CushionController.swift`, itself ported from the Android client's
//! `CushionController.kt` and the cushion rules of its `PlaybackLatency.kt`. The cushion *is* the
//! playback latency, so it is kept as small as this phone and this Wi-Fi can sustain: walked down
//! while the player reports no underruns, pushed back up the moment it does. Only the parts that
//! are about the cushion live here. What a platform measures off its own audio output (Android's
//! `AudioTrack` capacity rules, its timestamp backlog) stays with that output, which each platform
//! measures differently.
//!
//! Counts keep the iOS units and ranges: packets, frames, milliseconds and sample rates are `i64`,
//! as Swift's `Int` is on every phone it runs on.

/// The smallest cushion, in packets.
pub const MINIMUM_PACKETS: i64 = 1;
/// The largest cushion the adaptive controller will grow to, in packets.
pub const MAXIMUM_PACKETS: i64 = 4;
/// Where a new session starts, in packets.
pub const START_PACKETS: i64 = 2;
/// Clean intervals needed before the cushion may shrink by one packet, so one quiet second cannot
/// ratchet it to the floor and a burst right after does not undo it at once.
const CLEAN_INTERVALS_BEFORE_SHRINK: i64 = 3;
/// How much audio is played between two adjustments, in milliseconds.
pub const ADJUST_INTERVAL_MILLISECONDS: i64 = 1_000;

/// The cushion the desk set for this musician (`apps/server/src/protocol.rs:577-600`): the value
/// the `playback` control message decodes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlaybackBufferSetting {
    /// The phone adapts the cushion on its own.
    Auto,
    /// The phone keeps this cushion and stops adapting it.
    Fixed {
        /// The length the desk pinned, in milliseconds.
        milliseconds: u16,
    },
}

impl PlaybackBufferSetting {
    /// `MIN_FIXED_BUFFER_MS` (`apps/server/src/playback.rs:17`).
    pub const MINIMUM_FIXED_MILLISECONDS: u16 = 5;
    /// `MAX_FIXED_BUFFER_MS` (`apps/server/src/playback.rs:21`).
    pub const MAXIMUM_FIXED_MILLISECONDS: u16 = 500;
}

/// One adjustment's outcome: the cushion to use next and the clean intervals to carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CushionDecision {
    /// The cushion to use next, in packets.
    pub cushion_packets: i64,
    /// The clean intervals in a row to carry into the next adjustment.
    pub clean_intervals: i64,
}

/// The cushion after one adjustment interval: one packet more the moment an underrun happened,
/// one packet less after three clean intervals in a row (`CLEAN_INTERVALS_BEFORE_SHRINK`),
/// otherwise unchanged. At the floor the clean count keeps climbing rather than resetting, exactly
/// as Android's `nextCushion` does.
pub fn next_cushion(
    cushion_packets: i64,
    clean_intervals: i64,
    underruns_in_interval: i64,
) -> CushionDecision {
    if underruns_in_interval > 0 {
        return CushionDecision {
            cushion_packets: (cushion_packets + 1).min(MAXIMUM_PACKETS),
            clean_intervals: 0,
        };
    }
    let clean = clean_intervals + 1;
    if clean >= CLEAN_INTERVALS_BEFORE_SHRINK && cushion_packets > MINIMUM_PACKETS {
        return CushionDecision {
            cushion_packets: cushion_packets - 1,
            clean_intervals: 0,
        };
    }
    CushionDecision {
        cushion_packets,
        clean_intervals: clean,
    }
}

/// The cushion, in packets, for a buffer the desk pinned at `milliseconds`: rounded *up* to whole
/// packets of `packet_frames` at `sample_rate` so the phone never plays less than the engineer set,
/// at least one, and at most the whole packets a player of `capacity_frames` holds. One packet
/// before the first packet sized anything.
pub fn fixed_cushion_packets(
    milliseconds: i64,
    packet_frames: i64,
    sample_rate: i64,
    capacity_frames: i64,
) -> i64 {
    if packet_frames <= 0 || sample_rate <= 0 {
        return 1;
    }
    let packets = ceiling_division(milliseconds.max(0) * sample_rate, 1_000 * packet_frames);
    let most = (capacity_frames / packet_frames).max(1);
    packets.max(1).min(most)
}

/// The frames played between two adjustments: [`ADJUST_INTERVAL_MILLISECONDS`] at `sample_rate`,
/// or at 48 kHz when the rate cannot be used. Counted in frames, so a second is a second whatever
/// block size the desk sends (256-frame blocks are 187.5 packets a second, 480-frame blocks 100).
pub fn adjust_interval_frames(sample_rate: i64) -> i64 {
    let rate = if sample_rate > 0 { sample_rate } else { 48_000 };
    (rate * ADJUST_INTERVAL_MILLISECONDS / 1_000).max(1)
}

/// The frames a player must be able to hold so the longest buffer the desk can pin fits, in whole
/// packets of `packet_frames` so its last packet fits too (just the frames when the packet size is
/// unknown).
///
/// Android opens its `AudioTrack` this large and then sets the buffer size *down*; the iOS player
/// sizes its own buffer with it. Either way room costs memory (about 94 KB at 48 kHz), not latency:
/// what is queued is the cushion, not the capacity.
pub fn capacity_frames(sample_rate: i64, packet_frames: i64) -> i64 {
    let frames = ceiling_division(
        i64::from(PlaybackBufferSetting::MAXIMUM_FIXED_MILLISECONDS) * sample_rate.max(0),
        1_000,
    );
    if packet_frames <= 0 {
        return frames;
    }
    ceiling_division(frames, packet_frames) * packet_frames
}

/// `value / divisor` rounded up, for a `value` that is not negative and a positive `divisor`.
fn ceiling_division(value: i64, divisor: i64) -> i64 {
    (value + divisor - 1) / divisor
}

/// What the receive loop must do after one packet. Android returns these as bit flags in an `Int`
/// and iOS as an option set; here each is a flag of its own, and both can be set at once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct CushionStep {
    /// The cushion changed: ask the player for it.
    pub apply: bool,
    /// A second of audio was played: read the player's underrun count and call
    /// [`CushionController::adjust`].
    pub adjust: bool,
}

/// The cushion one receive loop keeps, in packets: adapted on Auto, held at the desk's length
/// otherwise. Ported from the iOS client's `CushionController`, itself ported from the Android
/// client's `CushionController.kt`.
///
/// - Auto: once per second of audio played the controller takes the player's underrun count and
///   moves the cushion as [`next_cushion`] decides, within 1 to 4 packets.
/// - Fixed: the cushion is the desk's length in whole packets, and nothing moves it. Underruns are
///   still noted, so the ones a fixed buffer had never count against Auto afterwards.
/// - Back to Auto: adaptation resumes from the cushion played, brought within the adaptive bounds,
///   with its clean intervals counted anew.
///
/// Only the receive loop uses it: it holds plain values, never allocates (nothing per packet) and
/// never locks.
#[derive(Clone, Debug)]
pub struct CushionController {
    cushion_packets: i64,
    setting: PlaybackBufferSetting,
    clean_intervals: i64,
    last_underruns: i64,
    frames_since_adjust: i64,
}

impl Default for CushionController {
    /// A controller that starts a session at [`START_PACKETS`].
    fn default() -> Self {
        Self::new(START_PACKETS)
    }
}

impl CushionController {
    /// A controller whose cushion starts at `start_packets`, on Auto.
    pub fn new(start_packets: i64) -> Self {
        Self {
            cushion_packets: start_packets,
            setting: PlaybackBufferSetting::Auto,
            clean_intervals: 0,
            last_underruns: 0,
            frames_since_adjust: 0,
        }
    }

    /// The cushion asked of the player, in packets.
    pub fn cushion_packets(&self) -> i64 {
        self.cushion_packets
    }

    /// The desk's buffer in force.
    pub fn setting(&self) -> PlaybackBufferSetting {
        self.setting
    }

    /// The receive loop's step for one packet: follows the desk's `setting` (read once per packet)
    /// at the player's `playback_rate`, or the packet's `packet_rate` while no player set one, in a
    /// player of `capacity_frames`, and counts the `played_frames` played.
    ///
    /// `packet_frames` is the packet size the player opened with, the unit the cushion is counted
    /// in; `played_frames` is the size of the packet just played, which counts the time. They differ
    /// once the desk changes its block size mid-stream. Android passes the packet's samples for the
    /// second and divides them by a fixed two channels; the frame count from the datagram header
    /// already carries the real channel count, so counting samples as frames cannot be written.
    #[must_use]
    pub fn on_packet(
        &mut self,
        setting: PlaybackBufferSetting,
        packet_frames: i64,
        played_frames: i64,
        playback_rate: Option<i64>,
        packet_rate: i64,
        capacity_frames: i64,
    ) -> CushionStep {
        let rate = playback_rate.unwrap_or(packet_rate);
        let apply = self.follow(setting, packet_frames, rate, capacity_frames);
        let adjust = self.on_played(played_frames, rate);
        CushionStep { apply, adjust }
    }

    /// Follows the desk's `setting` with packets of `packet_frames` at `sample_rate` in a player of
    /// `capacity_frames`. Answers whether the cushion changed; the same setting again changes
    /// nothing.
    #[must_use]
    pub fn follow(
        &mut self,
        setting: PlaybackBufferSetting,
        packet_frames: i64,
        sample_rate: i64,
        capacity_frames: i64,
    ) -> bool {
        if setting == self.setting {
            return false;
        }
        let before = self.cushion_packets;
        self.setting = setting;
        self.clean_intervals = 0;
        self.cushion_packets = match setting {
            PlaybackBufferSetting::Auto => before.clamp(MINIMUM_PACKETS, MAXIMUM_PACKETS),
            PlaybackBufferSetting::Fixed { milliseconds } => fixed_cushion_packets(
                i64::from(milliseconds),
                packet_frames,
                sample_rate,
                capacity_frames,
            ),
        };
        self.cushion_packets != before
    }

    /// Counts `frames` played at `sample_rate`; answers true once a whole adjustment interval has
    /// been played since the last one. What runs past the interval counts towards the next, so the
    /// adjustments keep to the second.
    #[must_use]
    pub fn on_played(&mut self, frames: i64, sample_rate: i64) -> bool {
        self.frames_since_adjust += frames.max(0);
        let interval = adjust_interval_frames(sample_rate);
        if self.frames_since_adjust < interval {
            return false;
        }
        self.frames_since_adjust -= interval;
        true
    }

    /// One adjustment, with the player's *total* underrun count. On Auto the cushion moves as
    /// [`next_cushion`] decides; while fixed it never does, but the count is still taken so a fixed
    /// buffer's underruns never land on Auto later. Answers whether the cushion changed.
    #[must_use]
    pub fn adjust(&mut self, underruns_total: i64) -> bool {
        let underruns = underruns_total - self.last_underruns;
        self.last_underruns = underruns_total;
        if self.setting != PlaybackBufferSetting::Auto {
            return false;
        }
        let decision = next_cushion(self.cushion_packets, self.clean_intervals, underruns);
        self.clean_intervals = decision.clean_intervals;
        let changed = decision.cushion_packets != self.cushion_packets;
        self.cushion_packets = decision.cushion_packets;
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const APPLY: CushionStep = CushionStep {
        apply: true,
        adjust: false,
    };
    const ADJUST: CushionStep = CushionStep {
        apply: false,
        adjust: true,
    };

    fn fixed(milliseconds: u16) -> PlaybackBufferSetting {
        PlaybackBufferSetting::Fixed { milliseconds }
    }

    // CushionControllerTests.swift: what the cushion does beyond the Android parity suite, the
    // bounds it keeps, and the step flags composing when one packet both changes the cushion and
    // completes a second.

    /// Keeps the bounds the Android client uses.
    #[test]
    fn bounds() {
        assert_eq!(MINIMUM_PACKETS, 1);
        assert_eq!(MAXIMUM_PACKETS, 4);
        assert_eq!(START_PACKETS, 2);
        assert_eq!(ADJUST_INTERVAL_MILLISECONDS, 1_000);
    }

    /// Judges a second at 48 kHz when the rate cannot be used.
    #[test]
    fn adjust_interval_falls_back_to_48k() {
        for rate in [0, -1] {
            assert_eq!(adjust_interval_frames(rate), 48_000, "rate {rate}");
        }
    }

    /// Sizes the player for the longest buffer the desk can pin.
    #[test]
    fn capacity_follows_the_desks_longest_buffer() {
        let frames = capacity_frames(48_000, 0);

        assert_eq!(
            frames,
            i64::from(PlaybackBufferSetting::MAXIMUM_FIXED_MILLISECONDS) * 48
        );
    }

    /// Counts the cushion in the player's packets and the time in the packet played.
    ///
    /// Android passes the track's packet frames to `follow` and the played packet's samples to
    /// `onPlayed` (`PmonPlayer.kt:320-323`); a desk that changes its block size mid-stream makes
    /// them differ. 40 ms in 256-frame packets is 8 packets, whatever the packet played; 24 000
    /// played frames at 48 kHz is half a second, so the second packet completes one.
    #[test]
    fn cushion_and_time_take_their_own_frames() {
        fn step(cushion: &mut CushionController) -> CushionStep {
            cushion.on_packet(fixed(40), 256, 24_000, Some(48_000), 48_000, 96_000)
        }
        let mut cushion = CushionController::default();

        assert_eq!(step(&mut cushion), APPLY);
        assert_eq!(cushion.cushion_packets(), 8);
        assert_eq!(step(&mut cushion), ADJUST);
        assert_eq!(cushion.cushion_packets(), 8);
    }

    /// Reports both steps when one packet changes the cushion and completes a second.
    ///
    /// Android returns these as bit flags in an `Int`; the step has to keep both when both happen
    /// on the same packet, or the caller would apply the new cushion and miss the adjustment.
    #[test]
    fn steps_compose() {
        let mut cushion = CushionController::default();
        let step = cushion.on_packet(fixed(40), 48_000, 48_000, Some(48_000), 48_000, 96_000);

        assert_eq!(
            step,
            CushionStep {
                apply: true,
                adjust: true
            }
        );
    }

    // CushionParityTests.swift: the Android client's own tests for the playback cushion, ported
    // case for case: every test in `CushionControllerTest.kt` and the `nextCushion` tests in
    // `PlaybackLatencyTest.kt` (`apps/android/app/src/test/java/com/bandear/android/`). Same
    // inputs, same expected outputs.
    //
    // Two translations, both deliberate (made by the iOS port and kept here):
    // - Android's `fixedMs` is an `Int` where 0 means Auto. Here it is `PlaybackBufferSetting`, the
    //   value the `playback` message decodes to, so `fixedMs == 40` reads `fixed(40)` and
    //   `DESK_BUFFER_AUTO` reads `PlaybackBufferSetting::Auto`.
    // - Android's `onPacket` takes `packetFrames` (to size the cushion) and `packetSamples` (to
    //   count time, divided by a fixed `CHANNELS = 2`). Here it takes frames only, read off the
    //   datagram header with its real channel count. Counting samples as frames, which two Android
    //   tests exist to catch, can no longer be written. Those two tests are ported by what they
    //   assert about time (how many frames make a second) rather than by their now-impossible
    //   argument lists.

    /// The player holds the longest fixed buffer at 48 kHz in 256-frame packets.
    fn capacity() -> i64 {
        capacity_frames(48_000, 256)
    }

    // CushionControllerTest.kt

    #[test]
    fn the_track_holds_the_longest_fixed_buffer_in_whole_packets() {
        // 500 ms at 48 kHz is 24 000 frames: 93.75 packets of 256, so 94 of them.
        assert_eq!(capacity_frames(48_000, 256), 94 * 256);
        assert_eq!(capacity_frames(48_000, 240), 24_000);
        assert_eq!(capacity_frames(44_100, 0), 22_050);
    }

    #[test]
    fn a_fixed_length_is_rounded_up_to_whole_packets() {
        fn packets(milliseconds: i64, frames: i64) -> i64 {
            fixed_cushion_packets(milliseconds, frames, 48_000, capacity())
        }
        // 40 ms at 48 kHz is 1 920 frames: 7.5 packets of 256 frames, so 8; exactly 8 of 240.
        assert_eq!(packets(40, 256), 8);
        assert_eq!(packets(40, 240), 8);
        // One frame past a whole packet is one packet more.
        assert_eq!(packets(6, 256), 2);
        // Never below one packet.
        assert_eq!(packets(5, 512), 1);
        // The longest fits whole.
        assert_eq!(packets(500, 256), 94);
    }

    #[test]
    fn a_fixed_length_is_clamped_to_what_the_track_holds() {
        // A player that holds 4 800 frames holds 18 whole packets of 256.
        assert_eq!(fixed_cushion_packets(500, 256, 48_000, 4_800), 18);
        // Never below one packet, even for a player smaller than a packet.
        assert_eq!(fixed_cushion_packets(500, 256, 48_000, 100), 1);
        // Before the first packet sized anything there is nothing to convert: one packet.
        assert_eq!(fixed_cushion_packets(40, 0, 48_000, capacity()), 1);
    }

    #[test]
    fn an_adjustment_is_due_once_a_second_of_audio_was_played_whatever_the_packet_size() {
        let mut small = CushionController::default();
        // 256-frame packets at 48 kHz are 187.5 a second: the 188th completes the second.
        for packet in 1..=187 {
            assert!(!small.on_played(256, 48_000), "packet {packet}");
        }
        assert!(small.on_played(256, 48_000));
        // What passed the second counts towards the next: 375 packets are exactly two seconds.
        for packet in 189..=374 {
            assert!(!small.on_played(256, 48_000), "packet {packet}");
        }
        assert!(small.on_played(256, 48_000));

        let mut large = CushionController::default();
        for _ in 1..=99 {
            assert!(!large.on_played(480, 48_000));
        }
        assert!(large.on_played(480, 48_000), "100 packets of 10 ms");
    }

    #[test]
    fn on_auto_the_cushion_adapts_as_before() {
        let mut cushion = CushionController::default();
        assert_eq!(cushion.cushion_packets(), START_PACKETS);
        assert_eq!(cushion.setting(), PlaybackBufferSetting::Auto);

        assert!(cushion.adjust(1), "an underrun grows it");
        assert_eq!(cushion.cushion_packets(), 3);
        assert!(!cushion.adjust(1));
        assert!(!cushion.adjust(1));
        assert!(cushion.adjust(1), "three clean seconds shrink it");
        assert_eq!(cushion.cushion_packets(), 2);
    }

    #[test]
    fn a_fixed_buffer_holds_the_cushion_whatever_the_underruns() {
        let mut cushion = CushionController::default();

        assert!(cushion.follow(fixed(40), 256, 48_000, capacity()));
        assert_eq!(cushion.cushion_packets(), 8);
        assert_eq!(cushion.setting(), fixed(40));
        for underruns in [3, 9, 9, 9, 9, 20] {
            assert!(!cushion.adjust(underruns));
            assert_eq!(cushion.cushion_packets(), 8);
        }
        assert!(
            !cushion.follow(fixed(40), 256, 48_000, capacity()),
            "the same length again changes nothing"
        );
        assert!(cushion.follow(fixed(100), 256, 48_000, capacity()));
        assert_eq!(cushion.cushion_packets(), 19);
    }

    #[test]
    fn a_packet_follows_the_desks_buffer_with_its_own_frames_rate_and_track() {
        let mut cushion = CushionController::default();
        // 40 ms at 48 kHz in 256-frame packets is 8 packets; swapping the packet frames with the
        // player's capacity, or the rate with either, would give 1 packet or the whole player.
        let step = cushion.on_packet(fixed(40), 256, 256, Some(48_000), 44_100, 4_800);
        assert!(step.apply);
        assert_eq!(cushion.cushion_packets(), 8);
        assert_eq!(cushion.setting(), fixed(40));
        // A player that holds 4 800 frames clamps a long buffer to its 18 whole packets.
        assert!(
            cushion
                .on_packet(fixed(500), 256, 256, Some(48_000), 44_100, 4_800)
                .apply
        );
        assert_eq!(cushion.cushion_packets(), 18);
    }

    #[test]
    fn the_same_desk_buffer_again_asks_nothing_of_the_track() {
        fn step(controller: &mut CushionController, setting: PlaybackBufferSetting) -> CushionStep {
            controller.on_packet(setting, 256, 256, Some(48_000), 48_000, capacity())
        }
        let mut cushion = CushionController::default();
        assert!(step(&mut cushion, fixed(40)).apply);
        assert!(!step(&mut cushion, fixed(40)).apply);
        // Auto from the start changes nothing either: the controller already adapts.
        assert!(
            !step(
                &mut CushionController::default(),
                PlaybackBufferSetting::Auto
            )
            .apply
        );
    }

    /// Android: `onPacket(AUTO, packetFrames = 24_000, packetSamples = 48_000, …)`. Here the frames
    /// are all there is, so this pins the half of the test that is about time: 24 000 frames at
    /// 48 kHz is half a second, so the second packet completes it.
    #[test]
    fn a_second_is_counted_in_frames_of_the_packet_not_its_samples() {
        fn step(cushion: &mut CushionController) -> CushionStep {
            cushion.on_packet(
                PlaybackBufferSetting::Auto,
                24_000,
                24_000,
                Some(48_000),
                48_000,
                capacity(),
            )
        }
        let mut cushion = CushionController::default();
        assert!(!step(&mut cushion).adjust);
        assert!(step(&mut cushion).adjust);
    }

    /// Android passes `packetSamples = 44_100`, which is 22 050 stereo frames; that is the frame
    /// count used here. Half a second at the packet's 44.1 kHz, so with no player rate the second
    /// packet completes the second; at a 22.05 kHz player rate one does.
    #[test]
    fn without_a_track_rate_the_second_is_the_packets_rate() {
        fn no_track_step(cushion: &mut CushionController) -> CushionStep {
            cushion.on_packet(
                PlaybackBufferSetting::Auto,
                22_050,
                22_050,
                None,
                44_100,
                capacity(),
            )
        }
        let mut no_track = CushionController::default();
        assert!(!no_track_step(&mut no_track).adjust);
        assert!(no_track_step(&mut no_track).adjust);

        let mut slow_track = CushionController::default();
        assert!(
            slow_track
                .on_packet(
                    PlaybackBufferSetting::Auto,
                    22_050,
                    22_050,
                    Some(22_050),
                    44_100,
                    capacity(),
                )
                .adjust
        );
    }

    #[test]
    fn auto_resumes_adapting_from_the_cushion_played_within_its_bounds() {
        let mut cushion = CushionController::default();
        let _ = cushion.follow(fixed(40), 256, 48_000, capacity());
        // Underruns while fixed are the fixed buffer's: Auto does not grow for them.
        let _ = cushion.adjust(5);

        assert!(cushion.follow(PlaybackBufferSetting::Auto, 256, 48_000, capacity()));
        assert_eq!(cushion.cushion_packets(), MAXIMUM_PACKETS);
        assert_eq!(cushion.setting(), PlaybackBufferSetting::Auto);
        assert!(!cushion.adjust(5));
        assert!(!cushion.adjust(5));
        assert!(cushion.adjust(5), "shrinks after three clean seconds");
        assert_eq!(cushion.cushion_packets(), MAXIMUM_PACKETS - 1);

        let mut small = CushionController::default();
        let _ = small.follow(fixed(5), 512, 48_000, capacity());
        assert_eq!(small.cushion_packets(), 1);
        assert!(
            !small.follow(PlaybackBufferSetting::Auto, 512, 48_000, capacity()),
            "one packet is within the bounds already"
        );
        assert_eq!(small.cushion_packets(), 1);
    }

    // PlaybackLatencyTest.kt, nextCushion

    #[test]
    fn grows_the_cushion_when_an_underrun_happened() {
        assert_eq!(
            next_cushion(2, 0, 1),
            CushionDecision {
                cushion_packets: 3,
                clean_intervals: 0
            }
        );
    }

    #[test]
    fn caps_the_cushion_at_the_maximum() {
        assert_eq!(
            next_cushion(MAXIMUM_PACKETS, 0, 7),
            CushionDecision {
                cushion_packets: MAXIMUM_PACKETS,
                clean_intervals: 0
            }
        );
    }

    #[test]
    fn waits_for_clean_intervals_before_shrinking() {
        assert_eq!(
            next_cushion(2, 0, 0),
            CushionDecision {
                cushion_packets: 2,
                clean_intervals: 1
            }
        );
        assert_eq!(
            next_cushion(2, 1, 0),
            CushionDecision {
                cushion_packets: 2,
                clean_intervals: 2
            }
        );
    }

    #[test]
    fn shrinks_after_enough_clean_intervals() {
        assert_eq!(
            next_cushion(2, 2, 0),
            CushionDecision {
                cushion_packets: 1,
                clean_intervals: 0
            }
        );
    }

    #[test]
    fn never_shrinks_below_the_floor() {
        assert_eq!(
            next_cushion(MINIMUM_PACKETS, 2, 0),
            CushionDecision {
                cushion_packets: MINIMUM_PACKETS,
                clean_intervals: 3
            }
        );
    }
}
