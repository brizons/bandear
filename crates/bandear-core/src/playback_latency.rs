//! What the phone measures about its own playback: the audio queued ahead of the output, the
//! cushion it aims for, and the two a link report ends its interval with.
//!
//! Ported from the iOS client's former `PlaybackLatency`, itself ported from the measures in the
//! Android client's `PlaybackLatency`; this module replaced both. One clock only, the output's own
//! render position: the frames written but not presented yet are the backlog, which is the cushion
//! plus the system's output buffer. No desk clock takes part, so there is nothing to synchronise
//! and no clock skew to corrupt the value.
//!
//! A measure that was not taken is `None`, where Android's former measures reported `NaN`.
//!
//! Not every helper of the Android client's `PlaybackLatency` was ported here:
//! - `trackCapacityFrames` is [`capacity_frames`](crate::cushion::capacity_frames) and the former
//!   `nextCushion` is [`next_cushion`](crate::cushion::next_cushion), ported with the cushion they
//!   size and move.
//! - `keepsLowLatency` and `SHRINK_TOLERANCE_MS` are not ported, and stay in the Android client's
//!   `PlaybackLatency.kt`. They decide whether an Android `AudioTrack`, opened with room for the
//!   longest buffer, honoured the smaller size set on it, and reopen it when it did not. That is a
//!   question about one platform's audio output, which stays native; the iOS player queues its
//!   cushion in a buffer of its own, so there is no platform buffer size to second-guess.
//!
//! Counts keep the iOS units and ranges: frames, packets and sample rates are `i64`, as Swift's
//! `Int` and `Int64` are on every phone it runs on; milliseconds are `f64`.

/// Frames written to the output but not presented yet. Never below zero: a render position that
/// briefly runs ahead of the write count is a reporting artifact, not negative latency.
pub fn pending_frames(frames_written: i64, frame_position: i64) -> i64 {
    (frames_written - frame_position).max(0)
}

/// The [`pending_frames`] in milliseconds at `sample_rate`, or 0 when the rate cannot be used.
pub fn backlog_milliseconds(frames_written: i64, frame_position: i64, sample_rate: i64) -> f64 {
    if sample_rate <= 0 {
        return 0.0;
    }
    pending_frames(frames_written, frame_position) as f64 * 1_000.0 / sample_rate as f64
}

/// The frames the player keeps queued for a cushion of `cushion_packets` packets of
/// `packet_frames` frames: at least one packet, and never zero frames.
///
/// What is queued *is* the playback latency, so this is kept as small as the cushion allows.
/// Measured on Android over the fast path, the backlog was linear in it: three packets gave
/// 53.9 ms, two 43.9 ms, one 33.9 ms, over a fixed pipeline of about 24 ms.
pub fn target_buffer_frames(packet_frames: i64, cushion_packets: i64) -> i64 {
    (packet_frames.max(0) * cushion_packets.max(1)).max(1)
}

/// The cushion the phone aims for, in milliseconds: the [`target_buffer_frames`] at the player's
/// `sample_rate`, so phones with different packet sizes read on one scale. Not measured (`None`)
/// before the first packet sized it or at an unusable rate.
pub fn cushion_milliseconds(
    packet_frames: i64,
    cushion_packets: i64,
    sample_rate: i64,
) -> Option<f64> {
    if packet_frames <= 0 || sample_rate <= 0 {
        return None;
    }
    Some(target_buffer_frames(packet_frames, cushion_packets) as f64 * 1_000.0 / sample_rate as f64)
}

/// The cushion and the backlog a link report ends its interval with, in milliseconds; `None` for
/// one that was not measured.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReportExtras {
    /// The audio held before playing: the player's buffer plus the packets the reorder hold keeps.
    pub cushion_milliseconds: Option<f64>,
    /// The audio queued in the output now.
    pub backlog_milliseconds: Option<f64>,
}

/// What the link report due now ends its interval with, from the receive loop's player inputs.
///
/// - The cushion is the buffer the player applied (`applied_buffer_frames`, when it reports one;
///   0 when it does not) at its `playback_rate`, or else the buffer asked of it
///   ([`cushion_milliseconds`] of `cushion_packets` packets of `packet_frames` frames). The
///   `hold_packets` held back for delayed redundant copies (the [reorder hold](crate::reorder_hold),
///   0 when it holds none) count in it too: they are audio held before playing, as the player's
///   buffer is. Not measured before the first packet sized it, or while no player set the rate
///   (`None`).
/// - The backlog is the audio queued now ([`backlog_milliseconds`]): the `frames_written` past the
///   `presented_frame` the output reported. Not measured when no frame could be read (`None`). A
///   frame read with no rate reads as 0, as the backlog does at an unusable rate.
///
/// The Android client's former measure took a `timestampRead` flag beside the frame; here the frame
/// is optional, so a frame that was not read cannot be passed at all. The iOS client's former port
/// defaulted `applied_buffer_frames` and `hold_packets` to 0; here callers pass 0.
pub fn report_extras(
    packet_frames: i64,
    cushion_packets: i64,
    playback_rate: Option<i64>,
    frames_written: i64,
    presented_frame: Option<i64>,
    applied_buffer_frames: i64,
    hold_packets: i64,
) -> ReportExtras {
    let rate = playback_rate.unwrap_or(0);
    let player = if applied_buffer_frames > 0 && packet_frames > 0 && rate > 0 {
        Some(applied_buffer_frames as f64 * 1_000.0 / rate as f64)
    } else {
        cushion_milliseconds(packet_frames, cushion_packets, rate)
    };
    // No packet, no cushion, held or not.
    let held = if hold_packets > 0 {
        cushion_milliseconds(packet_frames, hold_packets, rate)
    } else {
        Some(0.0)
    };
    let cushion = player.and_then(|player| held.map(|held| player + held));
    let backlog = presented_frame.map(|frame| backlog_milliseconds(frames_written, frame, rate));
    ReportExtras {
        cushion_milliseconds: cushion,
        backlog_milliseconds: backlog,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cushion::START_PACKETS;

    // From the iOS client's former `PlaybackLatencyTests`: what the playback measures do beyond the
    // Android suite below, where `None` comes from, now that it stands in for Android's `NaN`.

    /// Reads a presented frame with no rate as no backlog, not as unmeasured.
    ///
    /// The Android client's former `PlaybackLatency`: "a read timestamp with no rate only happens
    /// in a test and reads as 0, as `backlogMs` reads it". The backlog is unmeasured only when no
    /// frame was read; an unknown rate costs the cushion, not the backlog.
    #[test]
    fn backlog_without_a_rate() {
        let extras = report_extras(256, 2, None, 2_400, Some(480), 0, 0);

        assert_eq!(extras.backlog_milliseconds, Some(0.0));
        assert_eq!(extras.cushion_milliseconds, None);
    }

    // The Android client's own tests for its playback measures, ported case for case and in order
    // from its `PlaybackLatencyTest` through the iOS client's former `PlaybackLatencyParityTests`:
    // same inputs, same expected values, to the same 1e-9. Each test keeps the Android test's name
    // in snake case; the ported tests left the Android file with this port.
    //
    // Not every test of that file is here:
    // - The five `nextCushion` tests are in `cushion`, with the policy they test.
    // - The three `keepsLowLatency` tests are not ported, because the helper is not: it decides
    //   whether an Android `AudioTrack` honoured the buffer size set on it, and its tests stay in
    //   the Android client's `PlaybackLatencyTest.kt`.
    // - `trackCapacityFrames` had no test in that file; its Android test lived in the former
    //   `CushionControllerTest` and is ported in `cushion`, against `capacity_frames`.
    //
    // Two translations, both deliberate (made by the iOS port and kept here):
    // - A measure Android reported as `NaN` (not measured) is `None` here.
    // - Android's `timestampRead` flag and its `presentedFrame` become one optional
    //   `presented_frame`: `timestampRead = false` reads `None`, whatever frame Android passed
    //   beside it.

    /// Android's `assertEquals(expected, actual, 1e-9)`, for a measure that must have been taken.
    #[track_caller]
    fn expect_milliseconds(actual: Option<f64>, expected: f64) {
        let value = actual.expect("not measured");
        assert!(
            (value - expected).abs() <= 1e-9,
            "{value} is not {expected}"
        );
    }

    #[test]
    fn counts_frames_written_but_not_presented() {
        assert_eq!(pending_frames(1_000, 520), 480);
    }

    #[test]
    fn clamps_when_the_render_position_runs_ahead_of_the_writer() {
        assert_eq!(pending_frames(100, 250), 0);
    }

    #[test]
    fn converts_pending_frames_to_milliseconds_at_48k() {
        // 480 frames at 48 kHz is exactly 10 ms.
        expect_milliseconds(Some(backlog_milliseconds(1_000, 520, 48_000)), 10.0);
    }

    #[test]
    fn reports_zero_backlog_when_the_track_is_fully_drained() {
        expect_milliseconds(Some(backlog_milliseconds(1_000, 1_000, 48_000)), 0.0);
    }

    #[test]
    fn reports_zero_for_an_unusable_sample_rate() {
        expect_milliseconds(Some(backlog_milliseconds(1_000, 520, 0)), 0.0);
    }

    #[test]
    fn buffer_is_the_cushion_times_the_packet() {
        assert_eq!(target_buffer_frames(480, 2), 960);
        assert_eq!(target_buffer_frames(480, 1), 480);
    }

    #[test]
    fn buffer_is_never_zero_frames() {
        assert_eq!(target_buffer_frames(0, 2), 1);
    }

    #[test]
    fn the_cushion_in_milliseconds_is_the_buffer_it_asks_for_at_the_tracks_rate() {
        // 240-frame packets at 48 kHz are 5 ms each.
        expect_milliseconds(cushion_milliseconds(240, 2, 48_000), 10.0);
        expect_milliseconds(cushion_milliseconds(240, 1, 48_000), 5.0);
        expect_milliseconds(cushion_milliseconds(480, 4, 48_000), 40.0);
    }

    #[test]
    fn the_cushion_is_not_measured_before_the_first_packet_or_at_an_unusable_rate() {
        assert_eq!(cushion_milliseconds(0, 2, 48_000), None);
        assert_eq!(cushion_milliseconds(240, 2, 0), None);
    }

    #[test]
    fn a_report_ends_with_the_cushion_asked_of_the_track_and_the_audio_queued() {
        // Distinct packet frames and cushion packets: 240-frame packets, 3 of them, are 15 ms; using
        // one value twice would give 1200 ms or 0.19 ms.
        let extras = report_extras(240, 3, Some(48_000), 1_000, Some(520), 0, 0);
        expect_milliseconds(extras.cushion_milliseconds, 15.0);
        // 480 frames queued at 48 kHz.
        expect_milliseconds(extras.backlog_milliseconds, 10.0);
    }

    #[test]
    fn the_cushion_reported_counts_the_packets_held_back_for_delayed_copies() {
        // 240-frame packets at 48 kHz are 5 ms each: a 3-packet cushion and a 2-packet hold are
        // 25 ms; swapped they would read the same, so the track's applied size is used too: 600
        // frames and 2 held packets are 12.5 + 10 ms.
        let asked = report_extras(240, 3, Some(48_000), 0, None, 0, 2);
        expect_milliseconds(asked.cushion_milliseconds, 25.0);
        let applied = report_extras(240, 3, Some(48_000), 0, None, 600, 2);
        expect_milliseconds(applied.cushion_milliseconds, 22.5);
        assert_eq!(
            report_extras(0, 3, Some(48_000), 0, None, 0, 2).cushion_milliseconds,
            None,
            "no packet, no cushion, held or not"
        );
    }

    #[test]
    fn a_report_before_the_first_packet_has_no_cushion_whatever_the_cushion_packets() {
        // No packet sized the buffer yet: the start cushion of 2 packets is not a cushion. Swapped,
        // 2-frame packets would read as a tiny cushion instead of none.
        let extras = report_extras(0, START_PACKETS, Some(48_000), 0, None, 0, 0);
        assert_eq!(extras.cushion_milliseconds, None);
        assert_eq!(extras.backlog_milliseconds, None);
    }

    #[test]
    fn the_backlog_counts_frames_written_past_the_presented_frame_not_the_other_way_round() {
        // Swapped, the written count would sit behind the render position and read as 0 ms.
        let extras = report_extras(256, 2, Some(48_000), 2_400, Some(480), 0, 0);
        expect_milliseconds(extras.backlog_milliseconds, 40.0);
    }

    #[test]
    fn without_a_timestamp_the_backlog_is_not_measured_and_the_cushion_still_is() {
        // The Android test passed `framesWritten = 2_400L, presentedFrame = 480L` with
        // `timestampRead = false`: a frame that was not read is not passed at all here.
        let extras = report_extras(256, 2, Some(48_000), 2_400, None, 0, 0);
        assert_eq!(extras.backlog_milliseconds, None);
        expect_milliseconds(extras.cushion_milliseconds, 512.0 * 1_000.0 / 48_000.0);
    }

    #[test]
    fn without_a_track_rate_the_cushion_is_not_measured() {
        let extras = report_extras(256, 2, None, 0, None, 0, 0);
        assert_eq!(extras.cushion_milliseconds, None);
        assert_eq!(extras.backlog_milliseconds, None);
    }

    #[test]
    fn a_report_carries_the_buffer_the_track_applied_when_it_is_known() {
        // Asked for 2 packets of 256 frames (512), the platform kept 768: the phone plays 16 ms, not
        // the 10.67 ms it asked for.
        let extras = report_extras(256, 2, Some(48_000), 0, None, 768, 0);
        expect_milliseconds(extras.cushion_milliseconds, 16.0);
        // Unknown (0) falls back to what was asked; before the first packet it is still not
        // measured.
        expect_milliseconds(
            report_extras(256, 2, Some(48_000), 0, None, 0, 0).cushion_milliseconds,
            512.0 * 1_000.0 / 48_000.0,
        );
        assert_eq!(
            report_extras(0, 2, Some(48_000), 0, None, 768, 0).cushion_milliseconds,
            None
        );
    }
}
