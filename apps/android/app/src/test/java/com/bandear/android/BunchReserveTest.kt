package com.bandear.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The depth of the Wi-Fi's recurring bunches ([BunchReserve]): fed with the frames that arrived
 * before each packet played, it measures each drought, the frames played while nothing arrived,
 * keeps for each second of audio played the second-longest drought that ended in it, and answers the
 * longest of the last [BUNCH_MEMORY_SECONDS] of them.
 */
class BunchReserveTest {
    private val reserve = BunchReserve(sampleRate = 48_000L)

    /**
     * Plays [seconds] of packets, [arrived] answering the frames that arrived before each one
     * (numbered from 0); answers the reserve after each play.
     */
    private fun play(seconds: Int, arrived: (Int) -> Long): List<Long> =
        (0 until seconds * PACKETS_PER_SECOND).map { played ->
            reserve.onPacket(arrived(played), FRAMES)
            reserve.reserveFrames
        }

    @Test
    fun oneDatagramAtATimeKeepsNoReserve() {
        val reserves = play(seconds = 3) { ONE }

        assertTrue(reserves.all { it == 0L })
    }

    @Test
    fun bunchesThatRecurKeepTheirDepthOnceTheSecondTheyStartInHasPlayed() {
        // 20 packets at once every 20 played, 100 ms: the 19 played before each bunch drain the queue.
        val reserves = play(seconds = 3) { if (it % 20 == 0) 20 * ONE else 0L }

        assertTrue(reserves.take(PACKETS_PER_SECOND - 1).all { it == 0L })
        assertTrue(reserves.drop(PACKETS_PER_SECOND - 1).all { it == 19 * ONE })
    }

    @Test
    fun aSingleDroughtIsNoPattern() {
        // A stall's: 30 played with nothing arriving, once, then its datagrams in two drains, among
        // packets that arrive one at a time.
        val reserves = play(seconds = 3) {
            when (it) {
                in 100..129 -> 0L
                130 -> 40 * ONE
                131 -> 30 * ONE
                else -> ONE
            }
        }

        assertTrue(reserves.all { it == 0L })
    }

    @Test
    fun theDepthIsKeptForTheMemoryThenGivenBack() {
        play(seconds = 2) { if (it % 20 == 0) 20 * ONE else 0L }
        val after = play(seconds = BUNCH_MEMORY_SECONDS) { ONE }

        // Kept while the last BUNCH_MEMORY_SECONDS still hold a second of bunches...
        assertTrue(after.take((BUNCH_MEMORY_SECONDS - 1) * PACKETS_PER_SECOND - 1).all { it == 19 * ONE })
        // ...and given back once they hold none.
        assertEquals(0L, after.last())
    }

    @Test
    fun stallsThatComeBackInBurstsAddNothing() {
        // A second of bunches, then a stall every 2 s whose datagrams come back in two drains after
        // 30 packets played with nothing arriving: the depth stays the bunches'.
        play(seconds = 1) { if (it % 20 == 0) 20 * ONE else 0L }
        val reserves = play(seconds = 10) {
            when (it % 400) {
                in 100..129 -> 0L
                130 -> 40 * ONE
                131 -> 30 * ONE
                else -> ONE
            }
        }

        assertTrue(reserves.all { it == 19 * ONE })
    }

    @Test
    fun theDeepestRecurringBunchesOfTheMemoryAreKept() {
        play(seconds = 1) { if (it % 20 == 0) 20 * ONE else 0L }
        val reserves = play(seconds = 2) { if (it % 10 == 0) 10 * ONE else 0L }

        assertTrue(reserves.all { it == 19 * ONE })
    }

    private companion object {
        /** 240-frame packets: 200 to a second of audio at 48 kHz. */
        const val FRAMES = 240
        const val PACKETS_PER_SECOND = 200
        const val ONE = FRAMES.toLong()
    }
}
