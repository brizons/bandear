package com.bandear.android

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class MixSnapshotTest {
    private fun sample() = MixSnapshot(
        channelGains = intArrayOf(80, 100, 40),
        channelPans = intArrayOf(0, 50, 100),
        channelMutes = booleanArrayOf(false, true, false),
        channelSolos = booleanArrayOf(true, false, false),
        volumePercent = 70,
        maxLevelPercent = 90,
        muted = true,
    )

    @Test
    fun moreOfMePushesTheMusicianChannelToFullAndHalvesTheRest() {
        val derived = sample().moreOfMe(1)!!

        assertEquals(40, derived.channelGains[0])
        assertEquals(100, derived.channelGains[1])
        assertEquals(20, derived.channelGains[2])
    }

    @Test
    fun moreOfMeLeavesEveryStoredValueUntouched() {
        val original = sample()

        original.moreOfMe(1)

        assertArrayEquals(intArrayOf(80, 100, 40), original.channelGains)
        assertArrayEquals(intArrayOf(0, 50, 100), original.channelPans)
        assertArrayEquals(booleanArrayOf(false, true, false), original.channelMutes)
        assertArrayEquals(booleanArrayOf(true, false, false), original.channelSolos)
        assertEquals(70, original.volumePercent)
        assertEquals(90, original.maxLevelPercent)
        assertTrue(original.muted)
    }

    @Test
    fun moreOfMeKeepsTheMasterControlsAndFlags() {
        val original = sample()

        val derived = original.moreOfMe(0)!!

        assertArrayEquals(original.channelPans, derived.channelPans)
        assertArrayEquals(original.channelMutes, derived.channelMutes)
        assertArrayEquals(original.channelSolos, derived.channelSolos)
        assertEquals(70, derived.volumePercent)
        assertEquals(90, derived.maxLevelPercent)
        assertTrue(derived.muted)
    }

    @Test
    fun moreOfMeDoesNothingWithoutADesignatedChannel() {
        assertNull(sample().moreOfMe(null))
    }

    @Test
    fun moreOfMeDoesNothingForAChannelOutsideTheMix() {
        assertNull(sample().moreOfMe(9))
    }
}

