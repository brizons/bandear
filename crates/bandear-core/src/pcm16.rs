//! Local protection: the master volume, ceiling and mute the phone applies to the desk's audio
//! itself, for as long as the desk does not own this phone's mix.
//!
//! Ported from the iOS client's former `Pcm16Processor`, itself a port of the Android client's
//! former `Pcm16Processor`; this module replaced both. The audio the phone receives is the desk's
//! finished stereo mix, so there is nothing per source left to scale: only the master controls
//! apply, and the per-source faders and pans do nothing without a working control channel.
//!
//! - Volume is linear, `volume_percent` clamped to 0-100 over 100, with no dB curve and no ramp.
//! - The ceiling is full scale times `max_level_percent` clamped to 0-100, in integer division as
//!   Android's former processor computed it: 25 % of 32 767 is 8 191.
//! - Muted, the gain is 0 whatever the volume.
//! - Each sample is scaled, clamped to the ceiling, then rounded with a half going towards positive
//!   infinity, as Kotlin's `roundToInt` does (`Math.round`). Rust's `round()` sends a half away
//!   from zero instead, which would turn -1.5 into -2 where Android played -1. `(x + 0.5).floor()`
//!   agrees with `Math.round` on every sample at every whole-percent volume (all 6 619 136 of them,
//!   checked on iOS against a transcription of the JDK's).
//!
//! Where Android's former processor returned a new array per packet, the samples are processed
//! where they lie, so the receive path allocates nothing: the player hands over the region of its
//! own buffer a packet was copied into, as a `&mut [i16]`.

/// Applies the master controls to the interleaved PCM16 `samples`, in place. Neutral controls
/// (100 %, 100 %, not muted) leave every sample as it was.
pub fn apply(samples: &mut [i16], volume_percent: i32, max_level_percent: i32, muted: bool) {
    let gain = if muted {
        0.0
    } else {
        f64::from(clamped_percent(volume_percent)) / 100.0
    };
    let ceiling = f64::from(i32::from(i16::MAX) * clamped_percent(max_level_percent) / 100);
    for sample in samples.iter_mut() {
        let limited = (f64::from(*sample) * gain).clamp(-ceiling, ceiling);
        let rounded = (limited + 0.5).floor();
        // Within the 16-bit range after the clamp, so the cast only drops the zero fraction.
        *sample = rounded.clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16;
    }
}

/// The samples to play for one received packet, in place.
///
/// Once the desk has acknowledged a mix for this phone it owns that mix: it applies the gains,
/// pans, volume, ceiling and mute itself, and keeps applying them for the rest of the session,
/// even while the control channel is down. Applied here as well they would count twice, so
/// `server_owns_mix` leaves the samples untouched. Until then the desk plays its neutral default,
/// and only the phone can protect the musician's ears.
pub fn apply_local_protection(
    samples: &mut [i16],
    volume_percent: i32,
    max_level_percent: i32,
    muted: bool,
    server_owns_mix: bool,
) {
    if server_owns_mix {
        return;
    }
    apply(samples, volume_percent, max_level_percent, muted);
}

fn clamped_percent(percent: i32) -> i32 {
    percent.clamp(0, 100)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Kotlin's `roundToInt` is `Math.round`, which sends a half towards positive infinity. Rust's
    /// `round()` sends it away from zero, so the two disagree on every negative half: -1.5 is -1 on
    /// Android's former processor and would be -2 here.
    #[test]
    fn halves_round_up() {
        // At half volume these are -1.5, -0.5, 0.5 and 1.5.
        let mut samples: [i16; 4] = [-3, -1, 1, 3];

        apply(&mut samples, 50, 100, false);

        assert_eq!(samples, [-1, 0, 1, 2]);
    }

    #[test]
    fn array_in_place() {
        let mut samples: Vec<i16> = vec![20_000, -20_000];
        let before = samples.as_ptr();

        apply_local_protection(&mut samples, 50, 100, false, false);

        assert_eq!(samples, [10_000, -10_000]);
        assert_eq!(samples.as_ptr(), before);
    }

    /// The player hands over the region of its own buffer a packet was copied into: only that
    /// region may change.
    #[test]
    fn buffer_region() {
        let mut buffer: [i16; 6] = [1_000, 2_000, 3_000, 4_000, 5_000, 6_000];

        apply_local_protection(&mut buffer[2..4], 50, 100, false, false);

        assert_eq!(buffer, [1_000, 2_000, 1_500, 2_000, 5_000, 6_000]);
    }

    #[test]
    fn buffer_pass_through() {
        let mut buffer: [i16; 3] = [-16_000, 0, 16_000];

        apply_local_protection(&mut buffer, 0, 25, true, true);

        assert_eq!(buffer, [-16_000, 0, 16_000]);
    }

    /// Unclamped, a volume of 150 % would amplify a mix that local protection may only attenuate,
    /// and a ceiling of 150 % would be 49 150, past full scale.
    #[test]
    fn controls_are_clamped() {
        let mut loud: [i16; 4] = [i16::MIN, -10_000, 10_000, i16::MAX];
        apply(&mut loud, 150, 150, false);
        assert_eq!(loud, [-32_767, -10_000, 10_000, 32_767]);

        let mut silent: [i16; 4] = [i16::MIN, -10_000, 10_000, i16::MAX];
        apply(&mut silent, -20, 100, false);
        assert_eq!(silent, [0, 0, 0, 0]);

        let mut capped: [i16; 4] = [i16::MIN, -10_000, 10_000, i16::MAX];
        apply(&mut capped, 100, -5, false);
        assert_eq!(capped, [0, 0, 0, 0]);
    }

    /// The Android client's own tests for local protection, ported case for case and in order from
    /// its former `Pcm16ProcessorTest` through the iOS client's former `Pcm16ProcessorParityTests`,
    /// both deleted with this port; each test keeps the Android test's name in snake case. Same
    /// samples, same controls, same expected samples.
    ///
    /// One translation, deliberate: Android's `apply` returned a new `ShortArray` and left its
    /// input alone. Here the samples are processed where they lie, so the receive path allocates
    /// nothing per packet, and each test reads the expected values back out of the buffer it passed
    /// in.
    mod android_parity {
        use super::super::*;

        #[test]
        fn applies_volume_and_hard_maximum_without_clipping() {
            let mut samples: [i16; 4] = [i16::MIN, -16_000, 16_000, i16::MAX];

            apply(&mut samples, 50, 25, false);

            assert_eq!(samples, [-8_191, -8_000, 8_000, 8_191]);
        }

        #[test]
        fn mute_overrides_volume_and_maximum() {
            let mut samples: [i16; 3] = [i16::MIN, 0, i16::MAX];

            apply(&mut samples, 100, 100, true);

            assert_eq!(samples, [0, 0, 0]);
        }

        #[test]
        fn already_mixed_stereo_stream_is_only_affected_by_master_controls() {
            // The received stream is the server's finished stereo mix, so the local fallback must
            // scale every sample by the same master volume instead of applying per-channel gains.
            let mut samples: [i16; 4] = [20_000, 10_000, -20_000, -10_000];

            apply(&mut samples, 50, 100, false);

            assert_eq!(samples, [10_000, 5_000, -10_000, -5_000]);
        }

        #[test]
        fn leaves_an_already_mixed_stream_unchanged_when_master_controls_are_neutral() {
            let original: [i16; 4] = [12_000, 4_000, -12_000, -4_000];
            let mut samples = original;

            apply(&mut samples, 100, 100, false);

            assert_eq!(samples, original);
        }

        #[test]
        fn skips_local_processing_when_the_server_owns_the_mix() {
            let original: [i16; 3] = [-16_000, 0, 16_000];
            let mut samples = original;

            apply_local_protection(&mut samples, 0, 25, true, true);

            assert_eq!(samples, original);
        }

        #[test]
        fn applies_local_processing_when_the_server_never_owned_the_mix() {
            let mut samples: [i16; 2] = [-16_000, 16_000];

            apply_local_protection(&mut samples, 50, 100, false, false);

            assert_eq!(samples, [-8_000, 8_000]);
        }
    }
}
