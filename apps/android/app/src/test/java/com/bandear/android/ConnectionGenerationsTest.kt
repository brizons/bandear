package com.bandear.android

import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The socket-identity guard: a callback acts only for the connection attempt that is current, so a
 * late event from an older socket can never touch a newer connection.
 */
class ConnectionGenerationsTest {
    @Test
    fun aNewAttemptIsCurrent() {
        val generations = ConnectionGenerations()

        val first = generations.begin()

        assertTrue(generations.isCurrent(first))
    }

    @Test
    fun aNewerAttemptMakesTheOlderOneStale() {
        val generations = ConnectionGenerations()
        val old = generations.begin()

        val replacement = generations.begin()

        assertNotEquals(old, replacement)
        assertFalse(generations.isCurrent(old))
        assertTrue(generations.isCurrent(replacement))
    }

    @Test
    fun theEndOfTheCurrentConnectionIsHandledExactlyOnce() {
        val generations = ConnectionGenerations()
        val current = generations.begin()

        assertTrue(generations.end(current))
        assertFalse(generations.end(current))
        assertFalse(generations.isCurrent(current))
    }

    @Test
    fun aLateEndFromAnOlderSocketLeavesTheCurrentOneAlone() {
        val generations = ConnectionGenerations()
        val old = generations.begin()
        val replacement = generations.begin()

        assertFalse(generations.end(old))
        assertTrue(generations.isCurrent(replacement))
    }

    @Test
    fun anAttemptStartedAfterAnEndIsCurrentAgain() {
        val generations = ConnectionGenerations()
        val lost = generations.begin()
        generations.end(lost)

        val retry = generations.begin()

        assertTrue(generations.isCurrent(retry))
        assertFalse(generations.isCurrent(lost))
    }

    @Test
    fun invalidatingMakesEveryAttemptStale() {
        val generations = ConnectionGenerations()
        val current = generations.begin()

        generations.invalidate()

        assertFalse(generations.isCurrent(current))
        assertFalse(generations.end(current))
    }
}
