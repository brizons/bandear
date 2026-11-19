package com.bandear.android

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The playback rule that stays on Android: whether a large track kept Auto's latency. The capacity
 * the track opens with, the cushion and the backlog are tested in Rust
 * (`crates/bandear-core/src/cushion.rs`, `playback_latency.rs`).
 */
class PlaybackLatencyTest {
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
