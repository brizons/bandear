package com.bandear.android

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Pure JVM coverage for the group levels carried inside a [MixSnapshot]. The view wiring itself has
 * no meaningful JVM test; this covers only the `More of me` interaction. Their JSON is the shared
 * core's (`crates/bandear-core/src/control/mix_bank.rs`, and [SavedMixesCompatibilityTest]).
 */
class GroupSnapshotTest {
    @Test
    fun moreOfMeKeepsTheGroupLevelsUntouched() {
        val original = MixSnapshot(
            channelGains = intArrayOf(80, 100),
            groupLevels = intArrayOf(30, 60),
        )

        val derived = original.moreOfMe(1)!!

        assertArrayEquals(intArrayOf(30, 60), derived.groupLevels)
        assertArrayEquals(intArrayOf(30, 60), original.groupLevels)
    }

    @Test
    fun theDefaultSnapshotStartsWithNoGroups() {
        assertTrue(MixSnapshot().groupLevels.isEmpty())
    }
}
