package com.bandear.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The playback rules that stay on Android: the capacity the track opens with and whether a large
 * track kept Auto's latency. The cushion and the backlog are tested in Rust
 * (`crates/bandear-core/src/cushion.rs`, `playback_latency.rs`).
 */
class PlaybackLatencyTest {
    @Test
    fun theTrackHoldsTheLongestFixedBufferInWholePackets() {
        // 500 ms at 48 kHz is 24 000 frames: 93.75 packets of 256, so 94 of them.
        assertEquals(94 * 256, PlaybackLatency.trackCapacityFrames(48_000, 256))
        assertEquals(24_000, PlaybackLatency.trackCapacityFrames(48_000, 240))
        assertEquals(22_050, PlaybackLatency.trackCapacityFrames(44_100, 0))
    }

    @Test
    fun aLargeTrackThatShrinksToTheWantedBufferIsKept() {
        // One packet of 256 frames wanted at 48 kHz; the minimum track would hold 1 920 frames.
        assertTrue(PlaybackLatency.keepsLowLatency(256, 256, 1_920, 48_000))
        // Rounded up by the platform within the tolerance (5 ms is 240 frames): still kept.
        assertTrue(PlaybackLatency.keepsLowLatency(256, 256 + 240, 100, 48_000))
        assertFalse(PlaybackLatency.keepsLowLatency(256, 256 + 241, 100, 48_000))
    }

    @Test
    fun aLargeTrackThatQueuesNoMoreThanTheMinimumTrackIsKept() {
        // Clamped up to the hardware minimum, but no more than a track opened at the platform
        // minimum (as before the desk could set a buffer) would queue: no worse than before.
        assertTrue(PlaybackLatency.keepsLowLatency(256, 1_920, 1_920, 48_000))
        assertFalse(PlaybackLatency.keepsLowLatency(256, 1_921, 1_920, 48_000))
    }

    @Test
    fun aLargeTrackThatWillNotShrinkIsReopened() {
        // The platform ignored the size: the whole 500 ms capacity stays queued.
        assertFalse(PlaybackLatency.keepsLowLatency(256, 94 * 256, 1_920, 48_000))
        // A refused call (an error code) or a size it cannot read proves nothing: reopen.
        assertFalse(PlaybackLatency.keepsLowLatency(256, -3, 1_920, 48_000))
        assertFalse(PlaybackLatency.keepsLowLatency(256, 0, 1_920, 48_000))
    }
}
